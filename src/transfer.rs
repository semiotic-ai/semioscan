//! Transfer amount calculation from ERC-20 token events
//!
//! This module provides tools for calculating the total amount of ERC-20 tokens
//! transferred from one address to another over a given block range.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::AmountCalculator;
//! use alloy_provider::ProviderBuilder;
//!
//! let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
//! let calculator = AmountCalculator::new(provider);
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

use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

use crate::{Transfer, TRANSFER_EVENT_SIGNATURE};

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
    pub chain_id: u64,
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
/// The calculator automatically adds delays for certain chains (e.g., Sonic)
/// to avoid hitting RPC rate limits.
pub struct AmountCalculator {
    provider: RootProvider,
}

impl AmountCalculator {
    /// Creates a new `AmountCalculator` with the given provider
    pub fn new(provider: RootProvider) -> Self {
        Self { provider }
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
    /// Adds automatic delays for chains with strict rate limits (e.g., Sonic chain ID 146).
    pub async fn calculate_transfer_amount_between_blocks(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<AmountResult> {
        let mut result = AmountResult {
            chain_id,
            to,
            token,
            amount: U256::ZERO,
        };

        let contract_address = token;

        let transfer_topic = B256::from_slice(&*keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes()));

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
                            chain_id = chain_id,
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

            // Add a small delay to avoid hitting rate limits on Sonic Alchemy endpoint
            if chain_id.eq(&146) && current_block <= to_block {
                sleep(Duration::from_millis(250)).await;
            }
        }

        info!(
            chain_id = chain_id,
            to = ?to,
            token = ?token,
            total_amount = ?result.amount,
            "Finished amount calculation"
        );

        Ok(result)
    }
}
