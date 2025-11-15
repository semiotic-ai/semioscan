//! Shared RPC error types for blockchain provider operations.
//!
//! This module provides error types for common RPC failures that can occur
//! across different modules when interacting with blockchain providers.

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
///
/// // Example of creating an RPC error with context
/// let error = RpcError::TransactionNotFound {
///     tx_hash: "0x123...".to_string(),
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
        operation: String,
        /// The underlying provider error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Transaction was not found on the blockchain.
    ///
    /// This typically means the transaction hash is invalid or the transaction
    /// hasn't been indexed by the provider yet.
    #[error("Transaction not found: {tx_hash}")]
    TransactionNotFound {
        /// The transaction hash that wasn't found
        tx_hash: String,
    },

    /// Receipt was not found for a transaction.
    ///
    /// This can occur if the transaction hasn't been mined yet, or if the
    /// provider hasn't indexed the receipt.
    #[error("Receipt not found for transaction: {tx_hash}")]
    ReceiptNotFound {
        /// The transaction hash whose receipt wasn't found
        tx_hash: String,
    },

    /// Block was not found at the specified block number.
    ///
    /// This can occur if the block number is beyond the chain tip, if there
    /// was a chain reorganization, or if the provider hasn't synced that block.
    #[error("Block not found: {block_number}")]
    BlockNotFound {
        /// The block number that wasn't found
        block_number: u64,
    },

    /// Failed to connect to the blockchain or execute an RPC call.
    ///
    /// This is a catch-all for RPC failures that don't fit other categories,
    /// such as network errors, timeouts, or provider downtime.
    #[error("Chain connection failed during {operation}")]
    ChainConnectionFailed {
        /// Description of the operation that failed
        operation: String,
        /// The underlying error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Failed to fetch block number from the blockchain.
    ///
    /// This typically indicates a connectivity issue or provider problem.
    #[error("Failed to get current block number")]
    GetBlockNumberFailed {
        /// The underlying provider error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Failed to fetch block details by number.
    ///
    /// This is different from `BlockNotFound` - it indicates the RPC call itself
    /// failed, not that the block doesn't exist.
    #[error("Failed to fetch block {block_number} details")]
    GetBlockFailed {
        /// The block number we tried to fetch
        block_number: u64,
        /// The underlying provider error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl RpcError {
    /// Helper to create a `GetLogsFailed` error from any error type.
    pub fn get_logs_failed(
        operation: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        RpcError::GetLogsFailed {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Helper to create a `ChainConnectionFailed` error from any error type.
    pub fn chain_connection_failed(
        operation: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        RpcError::ChainConnectionFailed {
            operation: operation.into(),
            source: Box::new(source),
        }
    }

    /// Helper to create a `GetBlockNumberFailed` error from any error type.
    pub fn get_block_number_failed(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        RpcError::GetBlockNumberFailed {
            source: Box::new(source),
        }
    }

    /// Helper to create a `GetBlockFailed` error from any error type.
    pub fn get_block_failed(
        block_number: u64,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        RpcError::GetBlockFailed {
            block_number,
            source: Box::new(source),
        }
    }
}
