// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strong types for price source operations.
//!
//! This module provides type-safe error handling for price source implementations,
//! eliminating type erasure by preserving original error types rather than converting to strings.

/// Errors that can occur when extracting price data from logs.
///
/// This error type preserves the original error information without type erasure,
/// allowing callers to inspect and handle specific error cases.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::price::{PriceSource, PriceSourceError};
/// use alloy_rpc_types::Log;
/// use alloy_sol_types::SolEvent;
///
/// fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
///     let event = SwapEvent::decode_log(&log.into())
///         .map_err(PriceSourceError::from)?;  // Preserves alloy_sol_types::Error
///
///     if event.amounts.is_empty() {
///         return Err(PriceSourceError::empty_token_arrays());
///     }
///
///     // ... rest of extraction logic
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum PriceSourceError {
    /// Failed to decode an event from the log data.
    ///
    /// This preserves the original `alloy_sol_types::Error` without type erasure,
    /// allowing inspection of decode failures (wrong signature, corrupted data, etc.).
    #[error("Failed to decode event")]
    DecodeError(#[source] alloy_sol_types::Error),

    /// Event was decoded but contains empty token arrays.
    ///
    /// SwapMulti events must have at least one token in both `tokensIn` and `tokensOut`.
    #[error("Swap event has empty token arrays")]
    EmptyTokenArrays,

    /// Token and amount arrays have mismatched lengths.
    ///
    /// SwapMulti events require `tokensIn.len() == amountsIn.len()` and
    /// `tokensOut.len() == amountsOut.len()`.
    #[error("Token and amount array lengths don't match: tokens_in={tokens_in}, amounts_in={amounts_in}, tokens_out={tokens_out}, amounts_out={amounts_out}")]
    ArrayLengthMismatch {
        /// Length of tokensIn array
        tokens_in: usize,
        /// Length of amountsIn array
        amounts_in: usize,
        /// Length of tokensOut array
        tokens_out: usize,
        /// Length of amountsOut array
        amounts_out: usize,
    },

    /// Event was decoded but the swap data is invalid.
    ///
    /// This is a catch-all for validation errors beyond the specific cases above,
    /// such as zero amounts, invalid token addresses, or other business logic violations.
    ///
    /// Unlike the old string-based version, this stores the error in a `Box<dyn Error>`
    /// to preserve the source error chain and enable programmatic error handling.
    #[error("Invalid swap data: {details}")]
    InvalidSwapData {
        /// Description of what makes the swap data invalid
        details: String,
    },
}

impl PriceSourceError {
    /// Create an `EmptyTokenArrays` error.
    ///
    /// Use this when a SwapMulti event has empty `tokensIn` or `tokensOut` arrays.
    pub fn empty_token_arrays() -> Self {
        PriceSourceError::EmptyTokenArrays
    }

    /// Create an `ArrayLengthMismatch` error with specific lengths.
    ///
    /// Use this when token and amount array lengths don't match in SwapMulti events.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use semioscan::PriceSourceError;
    ///
    /// let err = PriceSourceError::array_length_mismatch(2, 3, 1, 1);
    /// assert!(err.to_string().contains("don't match"));
    /// ```
    pub fn array_length_mismatch(
        tokens_in: usize,
        amounts_in: usize,
        tokens_out: usize,
        amounts_out: usize,
    ) -> Self {
        PriceSourceError::ArrayLengthMismatch {
            tokens_in,
            amounts_in,
            tokens_out,
            amounts_out,
        }
    }

    /// Create an `InvalidSwapData` error with details.
    ///
    /// Use this for validation errors that don't fit the more specific error types.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use semioscan::PriceSourceError;
    ///
    /// let err = PriceSourceError::invalid_swap_data("Zero amount in swap");
    /// assert!(err.to_string().contains("Invalid swap data"));
    /// ```
    pub fn invalid_swap_data(details: impl Into<String>) -> Self {
        PriceSourceError::InvalidSwapData {
            details: details.into(),
        }
    }
}

impl From<alloy_sol_types::Error> for PriceSourceError {
    fn from(error: alloy_sol_types::Error) -> Self {
        PriceSourceError::DecodeError(error)
    }
}
