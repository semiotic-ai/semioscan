//! Transfer amount calculation from ERC-20 token events
//!
//! This module provides tools for calculating the total amount of ERC-20 tokens
//! transferred from one address to another over a given block range.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::{AmountCalculator, SemioscanConfig};
//! use alloy_provider::ProviderBuilder;
//!
//! let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
//! let root_provider = provider.root().clone();
//! let config = SemioscanConfig::default(); // Includes rate limiting for Base, Sonic
//! let calculator = AmountCalculator::new(root_provider, config);
//!
//! let result = calculator
//!     .calculate_transfer_amount_between_blocks(
//!         chain_id,
//!         from_addr,
//!         to_addr,
//!         token_addr,
//!         start_block,
//!         end_block,
//!     )
//!     .await?;
//!
//! println!("Total transferred: {} (raw amount)", result.amount);
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::{keccak256, Address, BlockNumber, B256, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use tokio::time::sleep;
use tracing::{info, trace, warn};

use crate::{SemioscanConfig, Transfer, TRANSFER_EVENT_SIGNATURE};

/// Result of transfer amount calculation
///
/// Contains the total amount of a specific token transferred from one address
/// to another over a block range.
///
/// # Units
///
/// The `amount` field is the raw token amount (not normalized for decimals).
/// To get the human-readable amount, divide by 10^decimals for the token.
///
/// # Example
///
/// ```rust,ignore
/// // For USDC (6 decimals), raw amount of 1_000_000 = 1.0 USDC
/// let human_readable = result.amount / U256::from(10u128.pow(6));
/// ```
pub struct AmountResult {
    /// Chain ID where the transfers occurred
    pub chain: NamedChain,
    /// Address that received the tokens
    pub to: Address,
    /// Token contract address
    pub token: Address,
    /// Total amount transferred (raw, not normalized for decimals)
    pub amount: U256,
}

/// Calculator for ERC-20 token transfer amounts
///
/// Scans blockchain logs for ERC-20 `Transfer` events between two addresses
/// and aggregates the total amount transferred.
///
/// # Rate Limiting
///
/// The calculator uses [`SemioscanConfig`] to control rate limiting behavior.
/// Use default configuration for common RPC providers or customize for specific needs.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::{AmountCalculator, SemioscanConfig};
/// use alloy_provider::ProviderBuilder;
///
/// let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
/// let config = SemioscanConfig::default(); // Includes rate limiting for Base, Sonic
///
/// let calculator = AmountCalculator::new(provider.root().clone(), config);
/// ```
pub struct AmountCalculator<P> {
    provider: P,
    config: SemioscanConfig,
}

impl<P: Provider> AmountCalculator<P> {
    /// Creates a new `AmountCalculator` with the given provider and configuration
    ///
    /// # Arguments
    ///
    /// * `provider` - Alloy provider for blockchain RPC calls
    /// * `config` - Configuration controlling rate limiting and block range behavior
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::{AmountCalculator, SemioscanConfig};
    /// use alloy_provider::ProviderBuilder;
    ///
    /// let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    /// let root_provider = provider.root().clone();
    ///
    /// // Use defaults (includes rate limiting for Base, Sonic)
    /// let calculator = AmountCalculator::new(root_provider.clone(), SemioscanConfig::default());
    ///
    /// // Or customize for premium RPC (no delays)
    /// let premium_calculator = AmountCalculator::new(root_provider, SemioscanConfig::minimal());
    /// ```
    pub fn new(provider: P, config: SemioscanConfig) -> Self {
        Self { provider, config }
    }

    /// Calculate total ERC-20 token transfers from one address to another
    ///
    /// Scans all blocks in the range `[from_block, to_block]` for ERC-20 `Transfer`
    /// events where:
    /// - `from` is the sender
    /// - `to` is the recipient
    /// - `token` is the token contract
    ///
    /// # Arguments
    ///
    /// * `chain_id` - Chain ID where the transfers occurred
    /// * `from` - Address that sent the tokens
    /// * `to` - Address that received the tokens
    /// * `token` - Token contract address
    /// * `from_block` - Starting block number (inclusive)
    /// * `to_block` - Ending block number (inclusive)
    ///
    /// # Returns
    ///
    /// An [`AmountResult`] containing the total amount transferred (raw, not normalized).
    ///
    /// # Block Range Chunking
    ///
    /// The calculation automatically chunks large block ranges into 500-block segments
    /// to avoid RPC request size limits.
    ///
    /// # Rate Limiting
    ///
    /// Rate limiting behavior is controlled by the [`SemioscanConfig`] passed to [`AmountCalculator::new`].
    /// The calculator will automatically delay between chunks according to the configuration.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::{AmountCalculator, SemioscanConfig, SemioscanConfigBuilder};
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// // Custom rate limiting for specific chain
    /// let config = SemioscanConfigBuilder::new()
    ///     .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
    ///     .build();
    ///
    /// let calculator = AmountCalculator::new(provider, config);
    /// let result = calculator
    ///     .calculate_transfer_amount_between_blocks(
    ///         NamedChain::Arbitrum,
    ///         from_addr,
    ///         to_addr,
    ///         token_addr,
    ///         start_block,
    ///         end_block,
    ///     )
    ///     .await?;
    /// ```
    pub async fn calculate_transfer_amount_between_blocks(
        &self,
        chain: NamedChain,
        from: Address,
        to: Address,
        token: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
    ) -> anyhow::Result<AmountResult> {
        let mut result = AmountResult {
            chain,
            to,
            token,
            amount: U256::ZERO,
        };

        let contract_address = token;

        let transfer_topic = B256::from_slice(&*keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes()));

        // Get rate limit configuration for this chain
        let rate_limit = self.config.get_rate_limit_delay(chain);

        let mut current_block = from_block;

        while current_block <= to_block {
            let end_chunk_block = std::cmp::min(current_block + 499, to_block);

            let filter = Filter::new()
                .from_block(current_block)
                .to_block(end_chunk_block)
                .address(contract_address)
                .event_signature(vec![transfer_topic])
                .topic1(from)
                .topic2(to);

            let logs = self.provider.get_logs(&filter).await?;

            for log in logs {
                match Transfer::decode_log(&log.into()) {
                    Ok(event) => {
                        info!(
                            chain = ?chain,
                            to = ?to,
                            token = ?token,
                            amount = ?event.value,
                            block = ?current_block,
                            current_total_amount = ?result.amount,
                            "Adding transfer amount to result"
                        );
                        result.amount = result.amount.saturating_add(event.value);
                    }
                    Err(e) => {
                        warn!(error = ?e, "Failed to decode Transfer log");
                    }
                }
            }

            current_block = end_chunk_block + 1;

            // Apply rate limiting if configured (don't sleep after last chunk)
            if let Some(delay) = rate_limit {
                if current_block <= to_block {
                    trace!(
                        chain = ?chain,
                        delay_ms = delay.as_millis(),
                        "Applying rate limit delay between chunks"
                    );
                    sleep(delay).await;
                }
            }
        }

        info!(
            chain = ?chain,
            to = ?to,
            token = ?token,
            total_amount = ?result.amount,
            "Finished amount calculation"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemioscanConfigBuilder;
    use alloy_primitives::address;
    use std::time::Duration;

    #[test]
    fn test_amount_result_initialization() {
        let chain = NamedChain::Arbitrum;
        let to = address!("1111111111111111111111111111111111111111");
        let token = address!("2222222222222222222222222222222222222222");

        let result = AmountResult {
            chain,
            to,
            token,
            amount: U256::ZERO,
        };

        assert_eq!(result.chain, chain);
        assert_eq!(result.to, to);
        assert_eq!(result.token, token);
        assert_eq!(result.amount, U256::ZERO);
    }

    #[test]
    fn test_rate_limit_applied_for_sonic() {
        let config = SemioscanConfig::default();

        // Sonic should have 250ms delay by default
        let sonic_delay = config.get_rate_limit_delay(NamedChain::Sonic);
        assert_eq!(sonic_delay, Some(Duration::from_millis(250)));
    }

    #[test]
    fn test_rate_limit_applied_for_base() {
        let config = SemioscanConfig::default();

        // Base should have 250ms delay by default
        let base_delay = config.get_rate_limit_delay(NamedChain::Base);
        assert_eq!(base_delay, Some(Duration::from_millis(250)));
    }

    #[test]
    fn test_no_rate_limit_for_arbitrum_by_default() {
        let config = SemioscanConfig::default();

        // Arbitrum should have no delay by default
        let arb_delay = config.get_rate_limit_delay(NamedChain::Arbitrum);
        assert_eq!(arb_delay, None);
    }

    #[test]
    fn test_custom_rate_limit_overrides_default() {
        let config = SemioscanConfigBuilder::with_defaults()
            .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
            .build();

        // Custom delay should override default (no delay)
        let arb_delay = config.get_rate_limit_delay(NamedChain::Arbitrum);
        assert_eq!(arb_delay, Some(Duration::from_millis(100)));

        // Base should still have default delay
        let base_delay = config.get_rate_limit_delay(NamedChain::Base);
        assert_eq!(base_delay, Some(Duration::from_millis(250)));
    }

    #[test]
    fn test_minimal_config_has_no_delays() {
        let config = SemioscanConfig::minimal();

        // Premium RPC: no delays for any chain
        assert_eq!(config.get_rate_limit_delay(NamedChain::Sonic), None);
        assert_eq!(config.get_rate_limit_delay(NamedChain::Base), None);
        assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
    }

    #[test]
    fn test_amount_accumulation() {
        let chain = NamedChain::Mainnet;
        let to = address!("1111111111111111111111111111111111111111");
        let token = address!("2222222222222222222222222222222222222222");

        let mut result = AmountResult {
            chain,
            to,
            token,
            amount: U256::ZERO,
        };

        // Add amounts using saturating_add (as done in calculate_transfer_amount_between_blocks)
        result.amount = result.amount.saturating_add(U256::from(1_000_000u64)); // 1 USDC (6 decimals)
        result.amount = result.amount.saturating_add(U256::from(2_500_000u64)); // 2.5 USDC

        assert_eq!(result.amount, U256::from(3_500_000u64)); // 3.5 USDC total
    }

    #[test]
    fn test_amount_overflow_protection() {
        let chain = NamedChain::Mainnet;
        let to = address!("1111111111111111111111111111111111111111");
        let token = address!("2222222222222222222222222222222222222222");

        let mut result = AmountResult {
            chain,
            to,
            token,
            amount: U256::MAX - U256::from(100u64),
        };

        // Add amount that would overflow - should saturate at U256::MAX
        result.amount = result.amount.saturating_add(U256::from(200u64));

        assert_eq!(result.amount, U256::MAX);
    }

    #[test]
    fn test_large_token_amounts() {
        let chain = NamedChain::Mainnet;
        let to = address!("1111111111111111111111111111111111111111");
        let token = address!("2222222222222222222222222222222222222222");

        let mut result = AmountResult {
            chain,
            to,
            token,
            amount: U256::ZERO,
        };

        // Test with 18-decimal token (like WETH): 1 ETH = 1e18 wei
        let one_eth = U256::from(1_000_000_000_000_000_000u64);
        result.amount = result.amount.saturating_add(one_eth);
        result.amount = result.amount.saturating_add(one_eth);

        assert_eq!(result.amount, U256::from(2_000_000_000_000_000_000u64)); // 2 ETH
    }
}
