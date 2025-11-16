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
/// ```rust,ignore
/// use semioscan::{GasCostCalculator, GasCalculationError};
/// use alloy_chains::NamedChain;
///
/// async fn example() -> Result<(), GasCalculationError> {
///     let calculator = GasCostCalculator::new(provider);
///
///     match calculator.calculate_gas_cost_for_transfers_between_blocks(
///         NamedChain::Arbitrum, from_address, to_address, token_address, start_block, end_block
///     ).await {
///         Ok(result) => println!("Gas cost: {:?}", result),
///         Err(GasCalculationError::EventDecodeFailed { log_index, .. }) => {
///             eprintln!("Failed to decode event at log {}", log_index);
///         }
///         Err(GasCalculationError::MissingData { field }) => {
///             eprintln!("Missing required field: {}", field);
///         }
///         Err(e) => eprintln!("Other error: {}", e),
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum GasCalculationError {
    /// Failed to decode an event log.
    ///
    /// This occurs when a log entry doesn't match the expected event signature
    /// or contains invalid data that can't be decoded.
    #[error("Failed to decode event at log index {log_index}")]
    EventDecodeFailed {
        /// Index of the log that failed to decode
        log_index: u64,
        /// The underlying decode error from alloy
        #[source]
        source: alloy_sol_types::Error,
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
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::GasCalculationError;
    ///
    /// // Pass the typed decode error directly - no formatting!
    /// match event.decode_log(&log) {
    ///     Ok(decoded) => { /* ... */ },
    ///     Err(e) => return Err(GasCalculationError::event_decode_failed(log_index, e)),
    /// }
    /// ```
    pub fn event_decode_failed(log_index: u64, source: alloy_sol_types::Error) -> Self {
        GasCalculationError::EventDecodeFailed { log_index, source }
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
