// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Chunked log fetching utility
//!
//! Provides a standalone function for fetching logs in chunks without
//! requiring `SemioscanConfig` or chain-specific configuration.
//!
//! # Example
//!
//! ```rust,no_run
//! use semioscan::fetch_logs_chunked;
//! use alloy_primitives::Address;
//! use alloy_provider::ProviderBuilder;
//! use alloy_rpc_types::Filter;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = ProviderBuilder::new()
//!     .connect_http("https://eth.llamarpc.com".parse()?);
//!
//! let contract_address: Address = "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse()?;
//!
//! let filter = Filter::new()
//!     .address(contract_address)
//!     .from_block(20_000_000)
//!     .to_block(20_000_100);
//!
//! // Fetch in 50-block chunks
//! let logs = fetch_logs_chunked(&provider, filter, 50).await?;
//! # Ok(())
//! # }
//! ```

use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log};
use tracing::debug;

use crate::errors::EventProcessingError;
use crate::MaxBlockRange;

/// Fetch logs in chunks to handle large block ranges
///
/// Splits the filter's block range into chunks and fetches sequentially,
/// concatenating results. This is useful when RPC providers reject queries
/// spanning too many blocks.
///
/// # Arguments
///
/// * `provider` - Any Alloy provider
/// * `filter` - Filter with `from_block` and `to_block` set
/// * `chunk_size` - Maximum blocks per RPC call (e.g., 500)
///
/// # Returns
///
/// All logs matching the filter, concatenated across all chunks.
///
/// # Errors
///
/// Returns an error if:
/// - The filter doesn't have both `from_block` and `to_block` set
/// - Any chunk fetch fails (fails fast, no partial results)
///
/// # Example
///
/// ```rust,no_run
/// use semioscan::fetch_logs_chunked;
/// use alloy_primitives::Address;
/// use alloy_provider::ProviderBuilder;
/// use alloy_rpc_types::Filter;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = ProviderBuilder::new()
///     .connect_http("https://eth.llamarpc.com".parse()?);
///
/// let swap_router: Address = "0x1111111254EEB25477B68fb85Ed929f73A960582".parse()?;
///
/// // Build filter with block range
/// let filter = Filter::new()
///     .address(swap_router)
///     .from_block(20_000_000)
///     .to_block(20_000_500);
///
/// // Fetch in 100-block chunks
/// let logs = fetch_logs_chunked(&provider, filter, 100).await?;
///
/// for log in logs {
///     println!("Log from block {:?}", log.block_number);
/// }
/// # Ok(())
/// # }
/// ```
pub async fn fetch_logs_chunked<P: Provider>(
    provider: &P,
    filter: Filter,
    chunk_size: u64,
) -> Result<Vec<Log>, EventProcessingError> {
    if chunk_size == 0 {
        return Err(EventProcessingError::invalid_input(
            "chunk_size must be greater than 0",
        ));
    }

    let start_block = filter
        .get_from_block()
        .ok_or_else(|| EventProcessingError::invalid_input("Filter must have from_block set"))?;

    let end_block = filter
        .get_to_block()
        .ok_or_else(|| EventProcessingError::invalid_input("Filter must have to_block set"))?;

    let max_block_range = MaxBlockRange::new(chunk_size);

    debug!(
        start_block = start_block,
        end_block = end_block,
        chunk_size = chunk_size,
        num_chunks = max_block_range.chunks_needed(start_block, end_block),
        "Starting chunked log fetch"
    );

    let mut all_logs = Vec::new();

    for (chunk_start, chunk_end) in max_block_range.chunk_range(start_block, end_block) {
        let chunk_filter = filter.clone().from_block(chunk_start).to_block(chunk_end);

        debug!(
            chunk_start = chunk_start,
            chunk_end = chunk_end,
            "Fetching logs for chunk"
        );

        let logs = provider.get_logs(&chunk_filter).await.map_err(|e| {
            EventProcessingError::rpc_failed(format!(
                "Failed to fetch logs for blocks {chunk_start}-{chunk_end}: {e}"
            ))
        })?;

        debug!(logs_count = logs.len(), "Fetched logs for chunk");
        all_logs.extend(logs);
    }

    debug!(total_logs = all_logs.len(), "Finished chunked log fetch");

    Ok(all_logs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_provider::ProviderBuilder;

    /// Create a provider for validation tests. The provider won't be called
    /// because validation fails before any RPC requests are made.
    fn dummy_provider() -> impl Provider {
        ProviderBuilder::new().connect_http("http://localhost:1".parse().unwrap())
    }

    #[test]
    fn test_uses_max_block_range_chunk_iterator() {
        // Verify we're using MaxBlockRange::chunk_range() correctly
        // The chunking math is tested extensively in types/config.rs
        let max_block_range = MaxBlockRange::new(30);
        let chunks: Vec<_> = max_block_range.chunk_range(0, 99).collect();

        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], (0, 29));
        assert_eq!(chunks[1], (30, 59));
        assert_eq!(chunks[2], (60, 89));
        assert_eq!(chunks[3], (90, 99));
    }

    #[tokio::test]
    async fn test_chunk_size_zero_returns_error() {
        let provider = dummy_provider();
        let filter = Filter::new().from_block(0).to_block(100);

        let result = fetch_logs_chunked(&provider, filter, 0).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, EventProcessingError::InvalidInput { .. }),
            "Expected InvalidInput error, got: {err:?}"
        );
        assert!(err.to_string().contains("chunk_size"));
    }

    #[tokio::test]
    async fn test_missing_from_block_returns_error() {
        let provider = dummy_provider();
        let filter = Filter::new().to_block(100); // No from_block

        let result = fetch_logs_chunked(&provider, filter, 500).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, EventProcessingError::InvalidInput { .. }),
            "Expected InvalidInput error, got: {err:?}"
        );
        assert!(err.to_string().contains("from_block"));
    }

    #[tokio::test]
    async fn test_missing_to_block_returns_error() {
        let provider = dummy_provider();
        let filter = Filter::new().from_block(0); // No to_block

        let result = fetch_logs_chunked(&provider, filter, 500).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, EventProcessingError::InvalidInput { .. }),
            "Expected InvalidInput error, got: {err:?}"
        );
        assert!(err.to_string().contains("to_block"));
    }
}
