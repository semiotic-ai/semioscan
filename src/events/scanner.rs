// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Generic event scanner with chunking and rate limiting
//!
//! This module provides a DRY solution for scanning blockchain events with
//! automatic block range chunking and configurable rate limiting.
//!
//! # Design Rationale
//!
//! Previously, chunking and rate limiting logic was duplicated across:
//! - `events/discovery.rs` (50+ lines)
//! - `events/transfers.rs` (50+ lines)
//! - `price/calculator.rs` (70+ lines)
//!
//! This module extracts that logic into a single, testable, reusable component.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::events::scanner::EventScanner;
//! use semioscan::events::filter::TransferFilterBuilder;
//! use alloy_chains::NamedChain;
//!
//! let scanner = EventScanner::new(provider, config);
//!
//! // Scan for token discovery
//! let filter = TransferFilterBuilder::new()
//!     .to_recipient(router)
//!     .build();
//!
//! let logs = scanner.scan(
//!     NamedChain::Arbitrum,
//!     filter,
//!     start_block,
//!     end_block,
//! ).await?;
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::BlockNumber;
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log};
use tokio::time::sleep;
use tracing::{debug, error, info};

use crate::config::SemioscanConfig;
use crate::errors::EventProcessingError;

/// Generic event scanner with chunking and rate limiting
///
/// Handles the common pattern of scanning large block ranges by:
/// 1. Chunking into smaller ranges based on chain-specific limits
/// 2. Applying rate limiting between chunks to avoid RPC throttling
/// 3. Collecting and aggregating results
///
/// # Thread Safety
///
/// The scanner is designed to be used within a single async task.
/// For concurrent scanning, create multiple scanner instances.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::events::scanner::EventScanner;
/// use semioscan::SemioscanConfig;
///
/// let scanner = EventScanner::new(provider, SemioscanConfig::default());
///
/// // The scanner handles chunking and rate limiting automatically
/// let logs = scanner.scan(
///     chain,
///     filter,
///     start_block,
///     end_block,
/// ).await?;
/// ```
pub struct EventScanner<P> {
    provider: P,
    config: SemioscanConfig,
}

impl<P: Provider> EventScanner<P> {
    /// Create a new event scanner
    ///
    /// # Arguments
    ///
    /// * `provider` - Blockchain RPC provider
    /// * `config` - Configuration for chunking and rate limiting
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::scanner::EventScanner;
    /// use semioscan::SemioscanConfig;
    /// use alloy_provider::ProviderBuilder;
    ///
    /// let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    /// let config = SemioscanConfig::default();
    /// let scanner = EventScanner::new(provider, config);
    /// ```
    pub fn new(provider: P, config: SemioscanConfig) -> Self {
        Self { provider, config }
    }

    /// Scan for events over a block range with automatic chunking and rate limiting
    ///
    /// This method handles:
    /// - Splitting large block ranges into chain-specific chunks
    /// - Applying rate limiting between RPC calls
    /// - Error handling and logging for failed chunks
    /// - Aggregating results into a single vector
    ///
    /// # Arguments
    ///
    /// * `chain` - The blockchain to scan (determines chunking and rate limits)
    /// * `filter_template` - Base filter to use (will be updated with block ranges)
    /// * `start_block` - First block to scan (inclusive)
    /// * `end_block` - Last block to scan (inclusive)
    ///
    /// # Returns
    ///
    /// A vector of all logs matching the filter across the entire block range.
    /// Failed chunks are logged but do not stop the scan.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::scanner::EventScanner;
    /// use semioscan::events::filter::transfers_to;
    /// use alloy_chains::NamedChain;
    ///
    /// let scanner = EventScanner::new(provider, config);
    ///
    /// // Create a filter without block range (scanner will add it)
    /// let filter = TransferFilterBuilder::new()
    ///     .to_recipient(router)
    ///     .build();
    ///
    /// let logs = scanner.scan(
    ///     NamedChain::Base,
    ///     filter,
    ///     1_000_000,
    ///     1_100_000,
    /// ).await?;
    ///
    /// println!("Found {} transfer events", logs.len());
    /// ```
    pub async fn scan(
        &self,
        chain: NamedChain,
        filter_template: Filter,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> Result<Vec<Log>, EventProcessingError> {
        info!(
            chain = %chain,
            start_block = start_block,
            end_block = end_block,
            "Starting event scan"
        );

        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        let mut all_logs = Vec::new();
        let mut current_block = start_block;

        while current_block <= end_block {
            let to_block = current_block
                .saturating_add(max_block_range.as_u64())
                .saturating_sub(1)
                .min(end_block);

            // Clone the filter template and add block range
            let filter = filter_template
                .clone()
                .from_block(current_block)
                .to_block(to_block);

            debug!(
                chain = %chain,
                current_block = current_block,
                to_block = to_block,
                "Fetching logs for chunk"
            );

            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    info!(
                        logs_count = logs.len(),
                        current_block = current_block,
                        to_block = to_block,
                        "Fetched logs for block range"
                    );
                    all_logs.extend(logs);
                }
                Err(e) => {
                    error!(
                        ?e,
                        %current_block,
                        %to_block,
                        "Error fetching logs in range"
                    );
                    // Continue with next chunk rather than failing completely
                }
            }

            current_block = to_block + 1;

            // Apply rate limiting if configured for this chain
            if let Some(delay) = rate_limit {
                if current_block <= end_block {
                    debug!(
                        chain = %chain,
                        delay_ms = delay.as_millis(),
                        "Applying rate limit delay"
                    );
                    sleep(delay).await;
                }
            }
        }

        info!(
            chain = %chain,
            total_logs = all_logs.len(),
            "Finished event scan"
        );

        Ok(all_logs)
    }

    /// Scan for events and process them with a custom handler
    ///
    /// This is a more flexible version of `scan()` that allows processing logs
    /// as they're fetched rather than collecting them all in memory.
    ///
    /// # Arguments
    ///
    /// * `chain` - The blockchain to scan
    /// * `filter_template` - Base filter to use
    /// * `start_block` - First block to scan (inclusive)
    /// * `end_block` - Last block to scan (inclusive)
    /// * `handler` - Async function to process each chunk of logs
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::events::scanner::EventScanner;
    /// use alloy_rpc_types::Log;
    ///
    /// let scanner = EventScanner::new(provider, config);
    ///
    /// scanner.scan_with_handler(
    ///     chain,
    ///     filter,
    ///     start_block,
    ///     end_block,
    ///     |logs: Vec<Log>| async move {
    ///         for log in logs {
    ///             // Process each log
    ///             process_log(log).await?;
    ///         }
    ///         Ok::<(), EventProcessingError>(())
    ///     },
    /// ).await?;
    /// ```
    #[allow(dead_code)]
    pub async fn scan_with_handler<F, Fut>(
        &self,
        chain: NamedChain,
        filter_template: Filter,
        start_block: BlockNumber,
        end_block: BlockNumber,
        mut handler: F,
    ) -> Result<(), EventProcessingError>
    where
        F: FnMut(Vec<Log>) -> Fut,
        Fut: std::future::Future<Output = Result<(), EventProcessingError>>,
    {
        info!(
            chain = %chain,
            start_block = start_block,
            end_block = end_block,
            "Starting event scan with handler"
        );

        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        let mut current_block = start_block;

        while current_block <= end_block {
            let to_block = current_block
                .saturating_add(max_block_range.as_u64())
                .saturating_sub(1)
                .min(end_block);

            let filter = filter_template
                .clone()
                .from_block(current_block)
                .to_block(to_block);

            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    info!(
                        logs_count = logs.len(),
                        current_block = current_block,
                        to_block = to_block,
                        "Processing logs for block range"
                    );

                    // Call the handler with this chunk of logs
                    handler(logs).await?;
                }
                Err(e) => {
                    error!(
                        ?e,
                        %current_block,
                        %to_block,
                        "Error fetching logs in range"
                    );
                    // Continue with next chunk
                }
            }

            current_block = to_block + 1;

            // Apply rate limiting if configured
            if let Some(delay) = rate_limit {
                if current_block <= end_block {
                    sleep(delay).await;
                }
            }
        }

        info!(chain = %chain, "Finished event scan with handler");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SemioscanConfigBuilder;
    use alloy_chains::NamedChain;
    use std::time::Duration;

    #[test]
    fn test_config_provides_correct_defaults() {
        let config = SemioscanConfig::default();

        // Verify default rate limits
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Base),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Sonic),
            Some(Duration::from_millis(250))
        );
        assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
    }

    #[test]
    fn test_custom_config_overrides() {
        let config = SemioscanConfigBuilder::with_defaults()
            .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
            .build();

        // Custom delay should override default
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Arbitrum),
            Some(Duration::from_millis(100))
        );
    }

    #[test]
    fn test_minimal_config_has_no_delays() {
        let config = SemioscanConfig::minimal();

        // Premium RPC: no delays
        assert_eq!(config.get_rate_limit_delay(NamedChain::Base), None);
        assert_eq!(config.get_rate_limit_delay(NamedChain::Sonic), None);
        assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
    }
}
