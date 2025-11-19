//! Error types for block window calculations.
//!
//! This module provides error types for operations in the `blocks` module,
//! particularly for calculating daily block windows.

use alloy_primitives::BlockNumber;
use chrono::NaiveDate;

use crate::UnixTimestamp;

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
///     let calculator = BlockWindowCalculator::with_disk_cache(provider, "cache.json")?;
///     let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
///
///     match calculator.get_daily_window(NamedChain::Arbitrum, date).await {
///         Ok(window) => println!("Success: {:?}", window),
///         Err(BlockWindowError::InvalidRange { start_block, end_block }) => {
///             eprintln!("Invalid block range: start={}, end={}", start_block, end_block);
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
    /// Invalid block range where end block is less than start block.
    ///
    /// This error occurs when you attempt to create a block window where the
    /// ending block number comes before the starting block number. You can use
    /// the `start_block` and `end_block` fields to understand which blocks
    /// caused the validation to fail.
    #[error("Invalid block range: end_block ({end_block}) < start_block ({start_block})")]
    InvalidRange {
        /// The starting block number of the invalid range
        start_block: BlockNumber,
        /// The ending block number of the invalid range
        end_block: BlockNumber,
    },

    /// Invalid timestamp range where end timestamp is not after start timestamp.
    ///
    /// This error occurs when validating a time window where the end timestamp
    /// is less than or equal to the start timestamp. You can access the specific
    /// timestamp values to understand the validation failure.
    #[error("Invalid timestamp range: end_ts ({end_ts}) <= start_ts ({start_ts})")]
    InvalidTimestampRange {
        /// The starting Unix timestamp
        start_ts: UnixTimestamp,
        /// The ending Unix timestamp (should be after start_ts)
        end_ts: UnixTimestamp,
    },

    /// Date cannot be converted to a valid UTC timestamp.
    ///
    /// This error occurs when attempting to convert a date to UTC time fails,
    /// typically when the date represents an invalid or ambiguous time.
    #[error("Cannot convert date to UTC timestamp: {date}")]
    InvalidDateConversion {
        /// The date that could not be converted to UTC
        date: NaiveDate,
    },

    /// Arithmetic overflow when performing date calculations.
    ///
    /// This error occurs when date arithmetic (such as adding days) would
    /// overflow beyond the valid range of representable dates.
    #[error("Date arithmetic overflow when adding to: {date}")]
    DateArithmeticOverflow {
        /// The date that caused the overflow
        date: NaiveDate,
    },

    /// Error reading from or writing to the block window cache.
    ///
    /// This error occurs when filesystem operations fail while accessing the
    /// cache file. You can access the specific path and underlying I/O error
    /// to understand what went wrong.
    #[error("Cache I/O error at {path}: {source}")]
    CacheIoError {
        /// Path to the cache file that caused the error
        path: String,
        /// The underlying I/O error
        #[source]
        source: std::io::Error,
    },

    /// Error serializing or deserializing block window data.
    ///
    /// This error occurs when JSON serialization/deserialization fails while
    /// reading or writing cached block windows. You can access the underlying
    /// serde_json error for details.
    #[error("Serialization error: {source}")]
    SerializationError {
        /// The underlying serde_json error
        #[source]
        source: serde_json::Error,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// block window calculations (e.g., fetching block numbers, block details).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl BlockWindowError {
    /// Create an `InvalidRange` error with start and end block numbers.
    pub fn invalid_range(start_block: BlockNumber, end_block: BlockNumber) -> Self {
        BlockWindowError::InvalidRange {
            start_block,
            end_block,
        }
    }

    /// Create an `InvalidTimestampRange` error with start and end timestamps.
    pub fn invalid_timestamp_range(start_ts: UnixTimestamp, end_ts: UnixTimestamp) -> Self {
        BlockWindowError::InvalidTimestampRange { start_ts, end_ts }
    }

    /// Create an `InvalidDateConversion` error for a date that cannot be converted to UTC.
    pub fn invalid_date_conversion(date: NaiveDate) -> Self {
        BlockWindowError::InvalidDateConversion { date }
    }

    /// Create a `DateArithmeticOverflow` error when date arithmetic overflows.
    pub fn date_arithmetic_overflow(date: NaiveDate) -> Self {
        BlockWindowError::DateArithmeticOverflow { date }
    }

    /// Create a `CacheIoError` from a path and I/O error.
    pub fn cache_io_error(path: impl Into<String>, source: std::io::Error) -> Self {
        BlockWindowError::CacheIoError {
            path: path.into(),
            source,
        }
    }

    /// Create a `SerializationError` from a serde_json error.
    pub fn serialization_error(source: serde_json::Error) -> Self {
        BlockWindowError::SerializationError { source }
    }
}
