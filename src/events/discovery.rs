//! Token discovery by scanning Transfer events
//!
//! This module provides utilities for discovering which tokens have been transferred
//! to a specific address (typically a router contract) by scanning blockchain Transfer events.
//!
//! # Use Cases
//!
//! - **Token inventory**: Discover which tokens a contract has received
//! - **Balance checking**: Identify tokens that may have non-zero balances
//! - **Historical analysis**: Track token flows over time
//!
//! # Example: Discover tokens transferred to a router
//!
//! ```rust,ignore
//! use semioscan::extract_transferred_to_tokens;
//! use alloy_chains::NamedChain;
//! use alloy_primitives::Address;
//! use alloy_provider::ProviderBuilder;
//!
//! let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
//! let router = Address::from_str("0x1234...")?;
//!
//! // Scan blocks 1000-2000 for Transfer events to this router
//! let tokens = extract_transferred_to_tokens(
//!     &provider,
//!     NamedChain::Arbitrum,
//!     router,
//!     1000,
//!     2000,
//! ).await?;
//!
//! println!("Found {} unique tokens", tokens.len());
//! ```
//!
//! # Performance
//!
//! The scanner automatically chunks requests based on chain-specific rate limits
//! (configured via [`SemioscanConfig`](crate::SemioscanConfig)) to avoid RPC throttling.

use alloy_chains::NamedChain;
use alloy_primitives::{keccak256, Address, BlockNumber, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::SemioscanConfig;
use crate::events::definitions::Transfer;
use crate::types::tokens::TokenSet;

/// Extract tokens transferred to a router contract using default configuration
///
/// Scans Transfer events over the specified block range to find all unique tokens
/// that have been transferred to the router address. Uses default rate limiting
/// and block range settings optimized for common RPC providers.
///
/// # Arguments
///
/// * `provider` - RPC provider for blockchain queries
/// * `chain` - The blockchain to scan
/// * `router` - Address to find transfers to (typically a router contract)
/// * `start_block` - First block in range (inclusive)
/// * `end_block` - Last block in range (inclusive)
///
/// # Returns
///
/// A [`TokenSet`] of unique token addresses that have been transferred to the router.
/// Using [`TokenSet`] ensures:
/// - Automatic deduplication
/// - Deterministic ordering
/// - Clear semantic meaning (this is a set of tokens, not arbitrary addresses)
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::extract_transferred_to_tokens;
/// use alloy_chains::NamedChain;
/// use alloy_primitives::address;
/// use alloy_provider::ProviderBuilder;
///
/// let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
/// let router = address!("0x1234567890abcdef1234567890abcdef12345678");
///
/// let tokens = extract_transferred_to_tokens(
///     &provider,
///     NamedChain::Base,
///     router,
///     1_000_000,
///     1_010_000,
/// ).await?;
///
/// for token in tokens.iter() {
///     println!("Token: {}", token);
/// }
/// ```
pub async fn extract_transferred_to_tokens<T: Provider>(
    provider: &T,
    chain: NamedChain,
    router: Address,
    start_block: BlockNumber,
    end_block: BlockNumber,
) -> anyhow::Result<TokenSet> {
    extract_transferred_to_tokens_with_config(
        provider,
        chain,
        router,
        start_block,
        end_block,
        &SemioscanConfig::default(),
    )
    .await
}

/// Extract tokens transferred to a router contract with custom configuration
///
/// Like [`extract_transferred_to_tokens`], but allows customizing RPC behavior
/// through a [`SemioscanConfig`](crate::SemioscanConfig). Use this when you need
/// to control rate limiting or block range sizes.
///
/// # Arguments
///
/// * `provider` - RPC provider for blockchain queries
/// * `chain` - The blockchain to scan
/// * `router` - Address to find transfers to (typically a router contract)
/// * `start_block` - First block in range (inclusive)
/// * `end_block` - Last block in range (inclusive)
/// * `config` - Custom configuration for RPC behavior
///
/// # Returns
///
/// A [`TokenSet`] of unique token addresses that have been transferred to the router.
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::{extract_transferred_to_tokens_with_config, SemioscanConfigBuilder};
/// use alloy_chains::NamedChain;
/// use alloy_primitives::address;
/// use alloy_provider::ProviderBuilder;
/// use std::time::Duration;
///
/// let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
/// let router = address!("0x1234567890abcdef1234567890abcdef12345678");
///
/// // Custom config with slower rate limiting
/// let config = SemioscanConfigBuilder::new()
///     .max_block_range(1000)
///     .rate_limit_delay(Duration::from_millis(500))
///     .build();
///
/// let tokens = extract_transferred_to_tokens_with_config(
///     &provider,
///     NamedChain::Polygon,
///     router,
///     40_000_000,
///     40_100_000,
///     &config,
/// ).await?;
///
/// println!("Found {} tokens", tokens.len());
/// ```
pub async fn extract_transferred_to_tokens_with_config<T: Provider>(
    provider: &T,
    chain: NamedChain,
    router: Address,
    start_block: BlockNumber,
    end_block: BlockNumber,
    config: &SemioscanConfig,
) -> anyhow::Result<TokenSet> {
    info!(
        chain = %chain,
        router = %router,
        start_block = start_block,
        end_block = end_block,
        "Fetching Transfer logs"
    );

    let max_block_range = config.get_max_block_range(chain);
    let rate_limit = config.get_rate_limit_delay(chain);

    let mut current_block = start_block;

    // TokenSet automatically deduplicates tokens and preserves deterministic order
    let mut transferred_to_tokens = TokenSet::new();

    while current_block <= end_block {
        let to_block = current_block
            .saturating_add(max_block_range.as_u64())
            .saturating_sub(1)
            .min(end_block);

        let filter = Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .event_signature(*keccak256(b"Transfer(address,address,uint256)"))
            .topic2(U256::from_be_bytes(router.into_word().into()));

        match provider.get_logs(&filter).await {
            Ok(logs) => {
                for log in logs {
                    let token_address = log.address();
                    match Transfer::decode_log(&log.inner) {
                        Ok(event) if event.to == router => {
                            debug!(extracted_token = ?token_address);
                            transferred_to_tokens.insert(token_address);
                        }
                        Err(e) => {
                            // This happens more for some chains than others, so we don't want to error out.
                            warn!(error = ?e, "Failed to decode Transfer log");
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                error!(?e, %current_block, %to_block, "Error fetching logs in range");
            }
        }

        current_block = to_block + 1;

        // Apply rate limiting if configured for this chain
        if let Some(delay) = rate_limit {
            if current_block <= end_block {
                sleep(delay).await;
            }
        }
    }

    Ok(transferred_to_tokens)
}
