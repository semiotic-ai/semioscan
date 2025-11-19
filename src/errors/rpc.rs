//! Shared RPC error types for blockchain provider operations.
//!
//! This module provides error types for common RPC failures that can occur
//! when interacting with blockchain providers.
//!
//! # When RPC Errors Occur
//!
//! RPC errors typically occur due to:
//! - **Network issues**: Connectivity problems, timeouts, or DNS failures
//! - **Rate limiting**: Provider has throttled your requests
//! - **Invalid parameters**: Block number out of range, invalid transaction hash
//! - **Provider issues**: Node is down, syncing, or experiencing problems
//! - **Chain reorganizations**: Requested block was reorged
//!
//! # Handling RPC Errors
//!
//! RPC errors include context about what operation was being performed to help
//! with debugging and retry logic:
//!
//! ```rust,ignore
//! use semioscan::{GasCostCalculator, GasCalculationError, RpcError};
//!
//! match calculator.calculate_gas_cost(...).await {
//!     Ok(result) => println!("Success: {:?}", result),
//!     Err(GasCalculationError::Rpc(RpcError::BlockNotFound { block_number })) => {
//!         eprintln!("Block {block_number} not found - may be beyond chain tip");
//!     }
//!     Err(GasCalculationError::Rpc(RpcError::ChainConnectionFailed { operation, .. })) => {
//!         eprintln!("Network error during {operation} - retrying...");
//!     }
//!     Err(e) => eprintln!("Other error: {e}"),
//! }
//! ```
//!
//! # Accessing Underlying Provider Errors
//!
//! Several variants preserve the underlying provider error in their `source` field.
//! Access it using the standard `Error::source()` method:
//!
//! ```rust,ignore
//! use std::error::Error;
//!
//! if let Err(e) = calculator.calculate_gas_cost(...).await {
//!     eprintln!("Error: {e}");
//!
//!     // Walk the error chain
//!     let mut source = e.source();
//!     while let Some(err) = source {
//!         eprintln!("  Caused by: {err}");
//!         source = err.source();
//!     }
//! }
//! ```

use std::borrow::Cow;

use alloy_primitives::{BlockNumber, TxHash};
use alloy_transport::TransportError;

/// Errors that can occur during blockchain RPC operations.
///
/// This error type captures common failure modes when interacting with
/// blockchain providers (e.g., via Alloy). It includes context about what
/// operation was being performed to aid in debugging.
///
/// # Examples
///
/// ```rust
/// use semioscan::RpcError;
/// use alloy_primitives::TxHash;
///
/// // Example of creating an RPC error with context
/// let error = RpcError::TransactionNotFound {
///     tx_hash: TxHash::ZERO,
/// };
/// println!("Error: {}", error);
/// ```
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// Failed to fetch logs from the blockchain.
    ///
    /// This can occur due to rate limiting, invalid block ranges, network
    /// connectivity issues, or provider-side errors.
    #[error("Failed to fetch logs for {operation}")]
    GetLogsFailed {
        /// Description of the operation that failed (e.g., "Transfer events 100-200")
        operation: Cow<'static, str>,
        /// The underlying transport error from alloy
        #[source]
        source: TransportError,
    },

    /// Transaction was not found on the blockchain.
    ///
    /// This typically means the transaction hash is invalid or the transaction
    /// hasn't been indexed by the provider yet.
    #[error("Transaction not found: {tx_hash}")]
    TransactionNotFound {
        /// The transaction hash that wasn't found
        tx_hash: TxHash,
    },

    /// Receipt was not found for a transaction.
    ///
    /// This can occur if the transaction hasn't been mined yet, or if the
    /// provider hasn't indexed the receipt.
    #[error("Receipt not found for transaction: {tx_hash}")]
    ReceiptNotFound {
        /// The transaction hash whose receipt wasn't found
        tx_hash: TxHash,
    },

    /// Block was not found at the specified block number.
    ///
    /// This can occur if the block number is beyond the chain tip, if there
    /// was a chain reorganization, or if the provider hasn't synced that block.
    #[error("Block not found: {block_number}")]
    BlockNotFound {
        /// The block number that wasn't found
        block_number: BlockNumber,
    },

    /// Failed to connect to the blockchain or execute an RPC call.
    ///
    /// This is a catch-all for RPC failures that don't fit other categories,
    /// such as network errors or provider downtime.
    #[error("Chain connection failed during {operation}")]
    ChainConnectionFailed {
        /// Description of the operation that failed
        operation: Cow<'static, str>,
        /// The underlying transport error from alloy
        #[source]
        source: TransportError,
    },

    /// RPC request timed out.
    ///
    /// This occurs when an RPC provider doesn't respond within the configured
    /// timeout period. Consider increasing the timeout or checking provider health.
    #[error("RPC request timed out after {timeout_secs}s during {operation}")]
    Timeout {
        /// Description of the operation that timed out
        operation: Cow<'static, str>,
        /// Timeout duration in seconds
        timeout_secs: u64,
    },

    /// Failed to fetch block number from the blockchain.
    ///
    /// This typically indicates a connectivity issue or provider problem.
    #[error("Failed to get current block number")]
    GetBlockNumberFailed {
        /// The underlying transport error from alloy
        #[source]
        source: TransportError,
    },

    /// Failed to fetch block details by number.
    ///
    /// This is different from `BlockNotFound` - it indicates the RPC call itself
    /// failed, not that the block doesn't exist.
    #[error("Failed to fetch block {block_number} details")]
    GetBlockFailed {
        /// The block number we tried to fetch
        block_number: BlockNumber,
        /// The underlying transport error from alloy
        #[source]
        source: TransportError,
    },
}

impl RpcError {
    /// Helper to create a `GetLogsFailed` error from a transport error.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::RpcError;
    ///
    /// // Pass the transport error directly - no boxing!
    /// match provider.get_logs(&filter).await {
    ///     Ok(logs) => { /* ... */ },
    ///     Err(e) => return Err(RpcError::get_logs_failed("Transfer events", e)),
    /// }
    /// ```
    pub fn get_logs_failed(
        operation: impl Into<Cow<'static, str>>,
        source: TransportError,
    ) -> Self {
        RpcError::GetLogsFailed {
            operation: operation.into(),
            source,
        }
    }

    /// Helper to create a `ChainConnectionFailed` error from a transport error.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::RpcError;
    ///
    /// // Pass the transport error directly - no boxing!
    /// match provider.get_transaction(hash).await {
    ///     Ok(tx) => { /* ... */ },
    ///     Err(e) => return Err(RpcError::chain_connection_failed("get_transaction", e)),
    /// }
    /// ```
    pub fn chain_connection_failed(
        operation: impl Into<Cow<'static, str>>,
        source: TransportError,
    ) -> Self {
        RpcError::ChainConnectionFailed {
            operation: operation.into(),
            source,
        }
    }

    /// Helper to create a `GetBlockNumberFailed` error from a transport error.
    pub fn get_block_number_failed(source: TransportError) -> Self {
        RpcError::GetBlockNumberFailed { source }
    }

    /// Helper to create a `GetBlockFailed` error from a transport error.
    pub fn get_block_failed(block_number: BlockNumber, source: TransportError) -> Self {
        RpcError::GetBlockFailed {
            block_number,
            source,
        }
    }

    /// Helper to create a `Timeout` error.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::RpcError;
    /// use std::time::Duration;
    /// use tokio::time::timeout;
    ///
    /// let timeout_duration = Duration::from_secs(30);
    /// match timeout(timeout_duration, provider.get_block(block_num)).await {
    ///     Ok(Ok(block)) => { /* ... */ },
    ///     Ok(Err(e)) => { /* RPC error */ },
    ///     Err(_elapsed) => return Err(RpcError::timeout("get_block", timeout_duration)),
    /// }
    /// ```
    pub fn timeout(operation: impl Into<Cow<'static, str>>, timeout: std::time::Duration) -> Self {
        RpcError::Timeout {
            operation: operation.into(),
            timeout_secs: timeout.as_secs(),
        }
    }
}
