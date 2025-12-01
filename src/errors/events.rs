// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Error types for event processing.
//!
//! This module provides error types for operations in the `events` module,
//! particularly for scanning and processing Transfer events and token discovery.

use super::RpcError;

/// Errors that can occur during event processing.
///
/// This error type covers event decoding failures, configuration issues,
/// and RPC failures that can occur when scanning for and processing
/// blockchain events.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::{extract_transferred_to_tokens, EventProcessingError};
/// use alloy_chains::NamedChain;
///
/// async fn example() -> Result<(), EventProcessingError> {
///     match extract_transferred_to_tokens(&provider, NamedChain::Arbitrum, router_address, start_block, end_block).await {
///         Ok(tokens) => println!("Found {} tokens", tokens.len()),
///         Err(EventProcessingError::ConfigurationMissing { field }) => {
///             eprintln!("Missing configuration: {}", field);
///         }
///         Err(EventProcessingError::Rpc(e)) => {
///             eprintln!("RPC error: {}", e);
///         }
///         Err(e) => eprintln!("Other error: {}", e),
///     }
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum EventProcessingError {
    /// Failed to decode an event from log data.
    ///
    /// This occurs when a log entry doesn't match the expected event signature
    /// (e.g., Transfer event) or contains invalid data. Note that in some
    /// contexts, decode failures are logged as warnings and processing continues.
    #[error("Failed to decode event: {details}")]
    DecodeFailed {
        /// Details about why the decode failed
        details: String,
    },

    /// Required configuration is missing.
    ///
    /// This typically occurs when environment variables (like RPC URLs)
    /// are not set, preventing event processing from proceeding.
    #[error("Missing configuration: {field}")]
    ConfigurationMissing {
        /// Name of the missing configuration field
        field: String,
    },

    /// RPC error when communicating with blockchain provider.
    ///
    /// This wraps [`RpcError`] for blockchain provider failures during
    /// event processing (e.g., fetching logs, scanning for events).
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),
}

impl EventProcessingError {
    /// Create a `DecodeFailed` error with details.
    pub fn decode_failed(details: impl Into<String>) -> Self {
        EventProcessingError::DecodeFailed {
            details: details.into(),
        }
    }

    /// Create a `ConfigurationMissing` error for a specific field.
    pub fn configuration_missing(field: impl Into<String>) -> Self {
        EventProcessingError::ConfigurationMissing {
            field: field.into(),
        }
    }

    /// Helper to create a missing RPC URL configuration error.
    pub fn missing_rpc_url(chain: impl std::fmt::Display) -> Self {
        Self::configuration_missing(format!("RPC_URL for chain {}", chain))
    }
}
