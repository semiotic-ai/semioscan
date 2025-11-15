//! Error types for combined data retrieval operations.
//!
//! This module provides error types for operations in the `retrieval` module,
//! particularly for retrieving combined blockchain data (transactions, receipts,
//! events, gas costs) for analysis.

use super::RpcError;

/// Errors that can occur during data retrieval operations.
///
/// This error type covers missing blockchain data, event decoding failures,
/// conversion errors, and RPC failures that can occur when retrieving and
/// processing combined blockchain data.
///
/// # Examples
///
/// ```rust,no_run
/// use semioscan::{CombinedCalculator, RetrievalError};
///
/// # async fn example() -> Result<(), RetrievalError> {
/// let calculator = CombinedCalculator::new(/* provider */);
///
/// match calculator.calculate_combined_data_ethereum(/* params */).await {
///     Ok(result) => println!("Retrieved data: {:?}", result),
///     Err(RetrievalError::MissingBlockchainData { field }) => {
///         eprintln!("Missing required field: {}", field);
///     }
///     Err(RetrievalError::ConversionFailed { details }) => {
///         eprintln!("Conversion failed: {}", details);
///     }
///     Err(e) => eprintln!("Other error: {}", e),
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    /// Required blockchain data is missing.
    ///
    /// This occurs when expected data is not present in logs, transactions,
    /// or receipts (e.g., transaction hash not in log, transaction not found,
    /// receipt not found, block number missing).
    #[error("Missing blockchain data: {field}")]
    MissingBlockchainData {
        /// Name/description of the missing data
        field: String,
    },

    /// Failed to decode an event from log data.
    ///
    /// This occurs when a log entry doesn't match the expected event signature
    /// or contains invalid data that can't be decoded.
    #[error("Event decode failed: {details}")]
    EventDecodeFailed {
        /// Details about why the decode failed
        details: String,
    },

    /// Data conversion failed.
    ///
    /// This occurs when converting between data types (e.g., U256 to BigDecimal,
    /// string parsing). Previously these were masked with fallback values, but
    /// proper error handling surfaces these issues.
    #[error("Conversion failed: {details}")]
    ConversionFailed {
        /// Details about the conversion failure
        details: String,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// data retrieval (e.g., fetching transactions, receipts, logs).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl RetrievalError {
    /// Create a `MissingBlockchainData` error for a specific field.
    pub fn missing_blockchain_data(field: impl Into<String>) -> Self {
        RetrievalError::MissingBlockchainData {
            field: field.into(),
        }
    }

    /// Create an `EventDecodeFailed` error with details.
    pub fn event_decode_failed(details: impl Into<String>) -> Self {
        RetrievalError::EventDecodeFailed {
            details: details.into(),
        }
    }

    /// Create a `ConversionFailed` error with details.
    pub fn conversion_failed(details: impl Into<String>) -> Self {
        RetrievalError::ConversionFailed {
            details: details.into(),
        }
    }

    /// Helper to create a missing transaction hash error.
    pub fn missing_transaction_hash() -> Self {
        Self::missing_blockchain_data("transaction_hash")
    }

    /// Helper to create a missing block number error.
    pub fn missing_block_number() -> Self {
        Self::missing_blockchain_data("block_number")
    }

    /// Helper to create a missing transaction error.
    pub fn missing_transaction(tx_hash: &str) -> Self {
        Self::missing_blockchain_data(format!("transaction for hash {}", tx_hash))
    }

    /// Helper to create a missing receipt error.
    pub fn missing_receipt(tx_hash: &str) -> Self {
        Self::missing_blockchain_data(format!("receipt for transaction {}", tx_hash))
    }

    /// Helper to create a BigDecimal conversion error.
    pub fn bigdecimal_conversion_failed(value: impl std::fmt::Display) -> Self {
        Self::conversion_failed(format!("Failed to convert {} to BigDecimal", value))
    }
}
