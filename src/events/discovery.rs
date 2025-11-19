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
//! # Typical Workflow
//!
//! 1. Scan router for tokens: [`extract_transferred_to_tokens()`]
//! 2. Check balances for each discovered token
//! 3. Liquidate tokens with non-zero balances above threshold
//!
//! # Performance
//!
//! - Automatically chunks large block ranges to avoid RPC limits
//! - Rate-limited by default for chains like Base and Sonic (250ms delay between chunks)
//! - Returns deduplicated token addresses in deterministic order
//! - Handles 100k+ block ranges efficiently with progress logging
//!
//! Configure behavior via [`SemioscanConfig`](crate::SemioscanConfig).
//!
//! # Advanced Usage
//!
//! For custom scanning patterns beyond "transfers to a specific address", use
//! [`EventScanner`](crate::events::scanner::EventScanner) and
//! [`TransferFilterBuilder`](crate::events::filter::TransferFilterBuilder):
//!
//! ```rust,ignore
//! use semioscan::{EventScanner, TransferFilterBuilder, SemioscanConfigBuilder};
//! use alloy_chains::NamedChain;
//! use alloy_primitives::address;
//! use std::time::Duration;
//!
//! // Custom config for premium RPC endpoints
//! let config = SemioscanConfigBuilder::new()
//!     .minimal()  // No rate limiting for dedicated endpoints
//!     .max_block_range(10_000)
//!     .build();
//!
//! let scanner = EventScanner::new(&provider, config);
//!
//! // Filter by both sender AND recipient
//! let filter = TransferFilterBuilder::new()
//!     .with_sender(sender_address)
//!     .with_recipient(recipient_address)
//!     .build();
//!
//! let logs = scanner.scan(NamedChain::Arbitrum, filter, 1000, 2000).await?;
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::{Address, BlockNumber};
use alloy_provider::Provider;
use alloy_sol_types::SolEvent;
use tracing::{debug, info, warn};

use crate::config::SemioscanConfig;
use crate::errors::EventProcessingError;
use crate::events::definitions::Transfer;
use crate::events::filter::TransferFilterBuilder;
use crate::events::scanner::EventScanner;
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
) -> Result<TokenSet, EventProcessingError> {
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
) -> Result<TokenSet, EventProcessingError> {
    info!(
        chain = %chain,
        router = %router,
        start_block = start_block,
        end_block = end_block,
        "Fetching Transfer logs"
    );

    // Create a scanner with the provider and config
    let scanner = EventScanner::new(provider, config.clone());

    // Build a filter for transfers to the router
    // No need for obscure U256::from_be_bytes conversion - filter builder handles it
    let filter = TransferFilterBuilder::new().with_recipient(router).build();

    // Scan for all Transfer events to this router
    let logs = scanner.scan(chain, filter, start_block, end_block).await?;

    // TokenSet automatically deduplicates tokens and preserves deterministic order
    let mut transferred_to_tokens = TokenSet::new();

    // Process the logs to extract unique token addresses
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

    Ok(transferred_to_tokens)
}
