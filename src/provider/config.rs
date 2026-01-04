// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Provider configuration options

use std::time::Duration;

/// Configuration for creating providers
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::provider::ProviderConfig;
///
/// let config = ProviderConfig::new("https://eth.llamarpc.com")
///     .with_rate_limit(10);
/// ```
///
/// Note: RPC request/response logging is handled natively by alloy's transport
/// layer at DEBUG/TRACE level.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// RPC endpoint URL
    pub url: String,
    /// Rate limit in requests per second (None for unlimited)
    pub rate_limit_per_second: Option<u32>,
    /// Request timeout duration
    pub timeout: Option<Duration>,
    /// Minimum delay between requests (alternative to rate limiting)
    pub min_delay: Option<Duration>,
}

impl ProviderConfig {
    /// Create a new provider configuration with the specified URL
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            rate_limit_per_second: None,
            timeout: None,
            min_delay: None,
        }
    }

    /// Set rate limiting (requests per second)
    ///
    /// When set, the provider will automatically throttle requests to stay
    /// within the specified limit. This is useful for public RPC endpoints.
    #[must_use]
    pub fn with_rate_limit(mut self, requests_per_second: u32) -> Self {
        self.rate_limit_per_second = Some(requests_per_second);
        self
    }

    /// Set rate limiting from an optional value
    #[must_use]
    pub fn with_rate_limit_opt(mut self, requests_per_second: Option<u32>) -> Self {
        self.rate_limit_per_second = requests_per_second;
        self
    }

    /// Set request timeout
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set minimum delay between requests
    ///
    /// This is an alternative to rate limiting that ensures a minimum
    /// time gap between consecutive requests.
    #[must_use]
    pub fn with_min_delay(mut self, delay: Duration) -> Self {
        self.min_delay = Some(delay);
        self
    }

    /// Check if this configuration includes rate limiting
    #[must_use]
    pub fn has_rate_limiting(&self) -> bool {
        self.rate_limit_per_second.is_some() || self.min_delay.is_some()
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self::new("http://localhost:8545")
    }
}

/// Preset configurations for common RPC providers
impl ProviderConfig {
    /// Configuration preset for public endpoints (conservative rate limiting)
    #[must_use]
    pub fn public_endpoint(url: impl Into<String>) -> Self {
        Self::new(url)
            .with_rate_limit(5) // Conservative default for public endpoints
            .with_timeout(Duration::from_secs(30))
    }

    /// Configuration preset for private/paid endpoints (higher limits)
    #[must_use]
    pub fn private_endpoint(url: impl Into<String>) -> Self {
        Self::new(url)
            .with_rate_limit(50)
            .with_timeout(Duration::from_secs(60))
    }

    /// Configuration preset for local nodes (no rate limiting)
    #[must_use]
    pub fn local_node(url: impl Into<String>) -> Self {
        Self::new(url).with_timeout(Duration::from_secs(120))
    }

    /// Configuration preset for Infura
    #[must_use]
    pub fn infura(project_id: &str, network: &str) -> Self {
        Self::private_endpoint(format!("https://{network}.infura.io/v3/{project_id}"))
    }

    /// Configuration preset for Alchemy
    #[must_use]
    pub fn alchemy(api_key: &str, network: &str) -> Self {
        Self::private_endpoint(format!("https://{network}.g.alchemy.com/v2/{api_key}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_new() {
        let config = ProviderConfig::new("https://eth.llamarpc.com");
        assert_eq!(config.url, "https://eth.llamarpc.com");
        assert!(config.rate_limit_per_second.is_none());
    }

    #[test]
    fn test_provider_config_with_rate_limit() {
        let config = ProviderConfig::new("https://eth.llamarpc.com").with_rate_limit(10);
        assert_eq!(config.rate_limit_per_second, Some(10));
        assert!(config.has_rate_limiting());
    }

    #[test]
    fn test_provider_config_public_endpoint() {
        let config = ProviderConfig::public_endpoint("https://eth.llamarpc.com");
        assert_eq!(config.rate_limit_per_second, Some(5));
        assert!(config.timeout.is_some());
    }

    #[test]
    fn test_provider_config_private_endpoint() {
        let config = ProviderConfig::private_endpoint("https://my-node.com");
        assert_eq!(config.rate_limit_per_second, Some(50));
    }

    #[test]
    fn test_provider_config_local_node() {
        let config = ProviderConfig::local_node("http://localhost:8545");
        assert!(config.rate_limit_per_second.is_none());
        assert!(!config.has_rate_limiting());
    }

    #[test]
    fn test_provider_config_infura() {
        let config = ProviderConfig::infura("my-project-id", "mainnet");
        assert!(config.url.contains("infura.io"));
        assert!(config.url.contains("my-project-id"));
        assert!(config.url.contains("mainnet"));
    }

    #[test]
    fn test_provider_config_alchemy() {
        let config = ProviderConfig::alchemy("my-api-key", "eth-mainnet");
        assert!(config.url.contains("alchemy.com"));
        assert!(config.url.contains("my-api-key"));
    }
}
