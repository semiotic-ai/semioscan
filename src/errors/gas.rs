//! Error types for gas cost calculations.
//!
//! This module provides error types for operations in the `gas` module,
//! particularly for calculating gas costs for token transfers and approvals.

use super::RpcError;

/// Errors that can occur during gas cost calculations.
///
/// This error type covers event decoding failures, missing blockchain data,
/// calculation errors, and RPC failures that can occur when calculating
/// gas costs for token transfers and approvals.
///
/// # Examples
///
/// ```rust,no_run
/// use semioscan::{GasCostCalculator, GasCalculationError};
///
/// # async fn example() -> Result<(), GasCalculationError> {
/// let calculator = GasCostCalculator::new(/* provider */);
///
/// match calculator.calculate_gas_cost_for_transfers_between_blocks(
///     /* params */
/// ).await {
///     Ok(result) => println!("Gas cost: {:?}", result),
///     Err(GasCalculationError::EventDecodeFailed { log_index, .. }) => {
///         eprintln!("Failed to decode event at log {}", log_index);
///     }
///     Err(GasCalculationError::MissingData { field }) => {
///         eprintln!("Missing required field: {}", field);
///     }
///     Err(e) => eprintln!("Other error: {}", e),
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum GasCalculationError {
    /// Failed to decode an event log.
    ///
    /// This occurs when a log entry doesn't match the expected event signature
    /// or contains invalid data that can't be decoded.
    #[error("Failed to decode event at log index {log_index}: {details}")]
    EventDecodeFailed {
        /// Index of the log that failed to decode
        log_index: u64,
        /// Details about why the decode failed
        details: String,
    },

    /// Required data is missing from blockchain response.
    ///
    /// This occurs when expected fields are not present in logs, transactions,
    /// or receipts (e.g., transaction hash not in log, block number missing).
    #[error("Missing required data: {field}")]
    MissingData {
        /// Name of the missing field
        field: String,
    },

    /// Gas calculation failed.
    ///
    /// This is a catch-all for calculation errors that don't fit other categories,
    /// such as arithmetic overflow, invalid gas price data, or inconsistent
    /// transaction receipts.
    #[error("Gas calculation failed: {details}")]
    CalculationFailed {
        /// Details about the calculation failure
        details: String,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// gas calculations (e.g., fetching logs, transactions, receipts).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl GasCalculationError {
    /// Create an `EventDecodeFailed` error.
    pub fn event_decode_failed(log_index: u64, details: impl Into<String>) -> Self {
        GasCalculationError::EventDecodeFailed {
            log_index,
            details: details.into(),
        }
    }

    /// Create a `MissingData` error for a specific field.
    pub fn missing_data(field: impl Into<String>) -> Self {
        GasCalculationError::MissingData {
            field: field.into(),
        }
    }

    /// Create a `CalculationFailed` error with details.
    pub fn calculation_failed(details: impl Into<String>) -> Self {
        GasCalculationError::CalculationFailed {
            details: details.into(),
        }
    }

    /// Helper to create a `MissingData` error for a missing transaction hash.
    pub fn missing_transaction_hash() -> Self {
        Self::missing_data("transaction_hash")
    }

    /// Helper to create a `MissingData` error for a missing block number.
    pub fn missing_block_number() -> Self {
        Self::missing_data("block_number")
    }

    /// Helper to create a `MissingData` error for a missing transaction.
    pub fn missing_transaction(tx_hash: &str) -> Self {
        Self::missing_data(format!("transaction for hash {}", tx_hash))
    }

    /// Helper to create a `MissingData` error for a missing receipt.
    pub fn missing_receipt(tx_hash: &str) -> Self {
        Self::missing_data(format!("receipt for transaction {}", tx_hash))
    }
}
