//! Error types for block window calculations.
//!
//! This module provides error types for operations in the `blocks` module,
//! particularly for calculating daily block windows.

use super::RpcError;

/// Errors that can occur during block window calculations.
///
/// This error type covers validation errors, RPC failures, cache I/O errors,
/// and serialization errors that can occur when calculating block windows
/// for specific dates on various chains.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::{BlockWindowCalculator, BlockWindowError};
/// use alloy_chains::NamedChain;
/// use chrono::NaiveDate;
///
/// async fn example() -> Result<(), BlockWindowError> {
///     let calculator = BlockWindowCalculator::new(provider, "cache.json");
///     let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
///
///     match calculator.get_daily_window(NamedChain::Arbitrum, date).await {
///         Ok(window) => println!("Success: {:?}", window),
///         Err(BlockWindowError::InvalidRange { reason }) => {
///             eprintln!("Invalid block range: {}", reason);
///         }
///         Err(BlockWindowError::Rpc(e)) => {
///             eprintln!("RPC error, will retry: {}", e);
///         }
///         Err(e) => eprintln!("Other error: {}", e),
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum BlockWindowError {
    /// Invalid block range provided or calculated.
    ///
    /// This can occur when end_block < start_block, when timestamps are
    /// out of order, or when the requested date range is invalid.
    #[error("Invalid block range: {reason}")]
    InvalidRange {
        /// Description of why the range is invalid
        reason: String,
    },

    /// Error related to timestamp calculations.
    ///
    /// This can occur when converting dates to timestamps, when handling
    /// overflow in timestamp arithmetic, or when block timestamps are
    /// inconsistent with expected values.
    #[error("Timestamp error: {details}")]
    TimestampError {
        /// Details about the timestamp error
        details: String,
    },

    /// Error reading from or writing to the block window cache.
    ///
    /// The cache is used to store previously calculated block windows to
    /// avoid redundant RPC calls. This error indicates a filesystem I/O
    /// problem.
    #[error("Cache I/O error at {path}: {details}")]
    CacheIoError {
        /// Path to the cache file that caused the error
        path: String,
        /// Details about the I/O error
        details: String,
        /// The underlying I/O error, if available
        #[source]
        source: Option<std::io::Error>,
    },

    /// Error serializing or deserializing block window data.
    ///
    /// This occurs when reading cached block windows or writing new ones
    /// to the cache.
    #[error("Serialization error: {details}")]
    SerializationError {
        /// Details about the serialization error
        details: String,
        /// The underlying serialization error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// block window calculations (e.g., fetching block numbers, block details).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl BlockWindowError {
    /// Create an `InvalidRange` error with a reason.
    pub fn invalid_range(reason: impl Into<String>) -> Self {
        BlockWindowError::InvalidRange {
            reason: reason.into(),
        }
    }

    /// Create a `TimestampError` with details.
    pub fn timestamp_error(details: impl Into<String>) -> Self {
        BlockWindowError::TimestampError {
            details: details.into(),
        }
    }

    /// Create a `CacheIoError` from an I/O error and path.
    pub fn cache_io_error(
        path: impl Into<String>,
        details: impl Into<String>,
        source: Option<std::io::Error>,
    ) -> Self {
        BlockWindowError::CacheIoError {
            path: path.into(),
            details: details.into(),
            source,
        }
    }

    /// Create a `SerializationError` from any serialization error.
    pub fn serialization_error(
        details: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        BlockWindowError::SerializationError {
            details: details.into(),
            source: Box::new(source),
        }
    }
}
