// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Provider factory functions for creating type-erased providers

use alloy_network::AnyNetwork;
use alloy_provider::ProviderBuilder;
use alloy_rpc_client::ClientBuilder;

use crate::errors::RpcError;
use crate::transport::{LoggingLayer, RateLimitLayer};

use super::config::ProviderConfig;
use super::AnyHttpProvider;

/// Create an HTTP provider with the given configuration
///
/// This creates a provider using `AnyNetwork` for type erasure, enabling
/// runtime chain selection at the cost of some type safety.
///
/// # Configuration Options
///
/// - Rate limiting: Automatically throttles requests
/// - Logging: Enables request/response logging via tracing
/// - Timeout: Sets request timeout
///
/// # Examples
///
/// Basic usage:
/// ```rust,ignore
/// use semioscan::provider::{create_http_provider, ProviderConfig};
///
/// let provider = create_http_provider(
///     ProviderConfig::new("https://eth.llamarpc.com")
/// )?;
/// ```
///
/// With rate limiting:
/// ```rust,ignore
/// use semioscan::provider::{create_http_provider, ProviderConfig};
///
/// let provider = create_http_provider(
///     ProviderConfig::new("https://eth.llamarpc.com")
///         .with_rate_limit(10)
/// )?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The URL is malformed
/// - The URL cannot be parsed
pub fn create_http_provider(config: ProviderConfig) -> Result<AnyHttpProvider, RpcError> {
    let url: url::Url = config
        .url
        .parse()
        .map_err(|e| RpcError::ProviderUrlInvalid(format!("{e}")))?;

    // Build the provider based on configuration
    // We disable recommended fillers to return a RootProvider for maximum flexibility
    match (
        config.rate_limit_per_second,
        config.min_delay,
        config.logging_enabled,
    ) {
        // Rate limit + logging
        (Some(rps), None, true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .layer(RateLimitLayer::per_second(rps))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        // Rate limit only
        (Some(rps), None, false) => {
            let client = ClientBuilder::default()
                .layer(RateLimitLayer::per_second(rps))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        // Min delay + logging
        (None, Some(delay), true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .layer(RateLimitLayer::with_min_delay(delay))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        // Min delay only
        (None, Some(delay), false) => {
            let client = ClientBuilder::default()
                .layer(RateLimitLayer::with_min_delay(delay))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        // Logging only
        (None, None, true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        // No layers
        (None, None, false) => Ok(ProviderBuilder::new()
            .disable_recommended_fillers()
            .network::<AnyNetwork>()
            .connect_http(url)),

        // Both rate limit and min delay (prefer rate limit)
        (Some(rps), Some(_), logging) => {
            tracing::warn!(
                "Both rate_limit_per_second and min_delay specified, using rate_limit_per_second"
            );
            let config = ProviderConfig {
                rate_limit_per_second: Some(rps),
                min_delay: None,
                logging_enabled: logging,
                ..config
            };
            create_http_provider(config)
        }
    }
}

/// Create a WebSocket provider with the given configuration
///
/// WebSocket providers enable real-time subscriptions to blocks, logs, and
/// pending transactions. They're ideal for applications that need low-latency
/// event monitoring.
///
/// # Note
///
/// This function is async because WebSocket connections require a handshake.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::provider::{create_ws_provider, ProviderConfig};
///
/// let provider = create_ws_provider(
///     ProviderConfig::new("wss://eth.llamarpc.com/ws")
/// ).await?;
///
/// // Subscribe to new blocks
/// let mut stream = provider.subscribe_blocks().await?;
/// while let Some(block) = stream.next().await {
///     println!("New block: {}", block.number);
/// }
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The URL is malformed
/// - The WebSocket connection fails
pub async fn create_ws_provider(
    config: ProviderConfig,
) -> Result<alloy_provider::RootProvider<AnyNetwork>, RpcError> {
    use alloy_provider::WsConnect;

    let ws = WsConnect::new(&config.url);

    // Build provider with optional layers
    // We disable recommended fillers to return a RootProvider
    match (config.rate_limit_per_second, config.logging_enabled) {
        (Some(rps), true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .layer(RateLimitLayer::per_second(rps))
                .ws(ws)
                .await
                .map_err(|e| RpcError::ProviderConnectionFailed(e.to_string()))?;

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        (Some(rps), false) => {
            let client = ClientBuilder::default()
                .layer(RateLimitLayer::per_second(rps))
                .ws(ws)
                .await
                .map_err(|e| RpcError::ProviderConnectionFailed(e.to_string()))?;

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        (None, true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .ws(ws)
                .await
                .map_err(|e| RpcError::ProviderConnectionFailed(e.to_string()))?;

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<AnyNetwork>()
                .connect_client(client))
        }

        (None, false) => ProviderBuilder::new()
            .disable_recommended_fillers()
            .network::<AnyNetwork>()
            .connect_ws(ws)
            .await
            .map_err(|e| RpcError::ProviderConnectionFailed(e.to_string())),
    }
}

/// Create an HTTP provider with specific network type
///
/// For applications that know the network type at compile time, this function
/// provides better type safety by returning a provider with the specific network.
///
/// # Type Parameters
///
/// - `N`: The network type (e.g., `Ethereum`, `Optimism`, `AnyNetwork`)
///
/// # Examples
///
/// ```rust,ignore
/// use alloy_network::Ethereum;
/// use semioscan::provider::{create_typed_http_provider, ProviderConfig};
///
/// let provider = create_typed_http_provider::<Ethereum>(
///     ProviderConfig::new("https://eth.llamarpc.com")
/// )?;
/// ```
pub fn create_typed_http_provider<N>(
    config: ProviderConfig,
) -> Result<alloy_provider::RootProvider<N>, RpcError>
where
    N: alloy_network::Network,
{
    let url: url::Url = config
        .url
        .parse()
        .map_err(|e| RpcError::ProviderUrlInvalid(format!("{e}")))?;

    // Build the provider based on configuration
    // We disable recommended fillers to return a RootProvider
    match (config.rate_limit_per_second, config.logging_enabled) {
        (Some(rps), true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .layer(RateLimitLayer::per_second(rps))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<N>()
                .connect_client(client))
        }

        (Some(rps), false) => {
            let client = ClientBuilder::default()
                .layer(RateLimitLayer::per_second(rps))
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<N>()
                .connect_client(client))
        }

        (None, true) => {
            let client = ClientBuilder::default()
                .layer(LoggingLayer::new())
                .http(url);

            Ok(ProviderBuilder::new()
                .disable_recommended_fillers()
                .network::<N>()
                .connect_client(client))
        }

        (None, false) => Ok(ProviderBuilder::new()
            .disable_recommended_fillers()
            .network::<N>()
            .connect_http(url)),
    }
}

/// Quick helper to create a simple HTTP provider without configuration
///
/// This is a convenience function for simple use cases where no rate limiting
/// or logging is needed.
///
/// # Errors
///
/// Returns an error if the URL is invalid
pub fn simple_http_provider(url: &str) -> Result<AnyHttpProvider, RpcError> {
    create_http_provider(ProviderConfig::new(url))
}

/// Quick helper to create a rate-limited HTTP provider
///
/// This is a convenience function that combines URL and rate limiting.
///
/// # Errors
///
/// Returns an error if the URL is invalid
pub fn rate_limited_http_provider(
    url: &str,
    requests_per_second: u32,
) -> Result<AnyHttpProvider, RpcError> {
    create_http_provider(ProviderConfig::new(url).with_rate_limit(requests_per_second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_http_provider_invalid_url() {
        let result = create_http_provider(ProviderConfig::new("not-a-valid-url"));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_http_provider_valid_url() {
        let result = create_http_provider(ProviderConfig::new("http://localhost:8545"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_http_provider_with_rate_limit() {
        let result =
            create_http_provider(ProviderConfig::new("http://localhost:8545").with_rate_limit(10));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_http_provider_with_logging() {
        let result =
            create_http_provider(ProviderConfig::new("http://localhost:8545").with_logging(true));
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_http_provider_with_all_options() {
        let result = create_http_provider(
            ProviderConfig::new("http://localhost:8545")
                .with_rate_limit(10)
                .with_logging(true),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_simple_http_provider() {
        let result = simple_http_provider("http://localhost:8545");
        assert!(result.is_ok());
    }

    #[test]
    fn test_rate_limited_http_provider() {
        let result = rate_limited_http_provider("http://localhost:8545", 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_typed_http_provider() {
        use alloy_network::Ethereum;

        let result =
            create_typed_http_provider::<Ethereum>(ProviderConfig::new("http://localhost:8545"));
        assert!(result.is_ok());
    }
}
