//! Error types for price calculations.
//!
//! This module provides error types for operations in the `price` module,
//! particularly for calculating token prices from swap events.

use alloy_primitives::Address;

use super::RpcError;

/// Errors that can occur during price calculations.
///
/// This error type wraps [`crate::price::PriceSourceError`] (which handles
/// event decoding and validation) and adds additional error cases for
/// metadata fetching, processing failures, and RPC errors.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::{PriceCalculator, PriceCalculationError};
/// use alloy_chains::NamedChain;
///
/// async fn example() -> Result<(), PriceCalculationError> {
///     let calculator = PriceCalculator::new(provider, NamedChain::Arbitrum, usdc_address, price_source);
///
///     match calculator.calculate_price_between_blocks(token_address, start_block, end_block).await {
///         Ok(result) => println!("Price data: {:?}", result),
///         Err(PriceCalculationError::MetadataFetchFailed { token, .. }) => {
///             eprintln!("Failed to fetch metadata for token {}", token);
///         }
///         Err(PriceCalculationError::Rpc(e)) => {
///             eprintln!("RPC error, will retry: {}", e);
///         }
///         Err(e) => eprintln!("Other error: {}", e),
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum PriceCalculationError {
    /// Error from the price source (event decoding/validation).
    ///
    /// This wraps errors from [`crate::price::PriceSourceError`], which occur
    /// when decoding swap events or validating swap data.
    #[error("Price source error: {0}")]
    Source(#[from] crate::price::PriceSourceError),

    /// Failed to fetch token metadata from the blockchain.
    ///
    /// This typically occurs when fetching token decimals from the token
    /// contract. It may indicate the contract doesn't implement the standard
    /// ERC20 interface, or there's an RPC issue.
    #[error("Failed to fetch token metadata for {token}")]
    MetadataFetchFailed {
        /// Address of the token
        token: Address,
        /// The underlying error from the contract call
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Swap data processing failed.
    ///
    /// This is a catch-all for processing errors that don't fit other categories,
    /// such as unexpected data formats or calculation errors.
    #[error("Processing failed: {details}")]
    ProcessingFailed {
        /// Details about the processing failure
        details: String,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// price calculations (e.g., fetching swap events, token metadata).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl PriceCalculationError {
    /// Create a `MetadataFetchFailed` error for a specific token.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use semioscan::PriceCalculationError;
    /// use alloy_primitives::Address;
    ///
    /// let addr = Address::ZERO;
    /// // Pass the typed error directly - no formatting!
    /// let err = PriceCalculationError::metadata_fetch_failed(addr, contract_error);
    /// ```
    pub fn metadata_fetch_failed(
        token: Address,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        PriceCalculationError::MetadataFetchFailed {
            token,
            source: Box::new(source),
        }
    }

    /// Create a `ProcessingFailed` error with details.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use semioscan::PriceCalculationError;
    ///
    /// let err = PriceCalculationError::processing_failed(
    ///     format!("Invalid swap data: {}", "missing price"),
    /// );
    /// ```
    pub fn processing_failed(details: impl Into<String>) -> Self {
        PriceCalculationError::ProcessingFailed {
            details: details.into(),
        }
    }
}
