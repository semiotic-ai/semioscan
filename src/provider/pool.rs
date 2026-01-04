// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Provider connection pooling for high-throughput scenarios
//!
//! This module provides thread-safe provider pooling that allows reusing
//! provider connections across multiple concurrent operations.
//!
//! # Overview
//!
//! The [`ProviderPool`] maintains a collection of providers indexed by chain,
//! enabling efficient connection reuse without creating new providers for each
//! operation. This is particularly useful for:
//!
//! - Multi-chain applications that query many chains concurrently
//! - High-throughput indexing or analytics workloads
//! - Long-running services that process blocks continuously
//!
//! # Examples
//!
//! ## Static Pool Initialization
//!
//! For applications that know their chains at startup, use a static pool:
//!
//! ```rust,ignore
//! use semioscan::provider::{ProviderPool, ProviderPoolBuilder};
//! use alloy_chains::NamedChain;
//! use std::sync::LazyLock;
//!
//! // Static pool initialized once on first access
//! static PROVIDERS: LazyLock<ProviderPool> = LazyLock::new(|| {
//!     ProviderPoolBuilder::new()
//!         .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
//!         .add_chain(NamedChain::Base, "https://mainnet.base.org")
//!         .with_rate_limit(10)
//!         .build()
//!         .expect("Failed to create provider pool")
//! });
//!
//! async fn get_block_number(chain: NamedChain) -> u64 {
//!     let provider = PROVIDERS.get(chain).expect("Chain not configured");
//!     provider.get_block_number().await.unwrap()
//! }
//! ```
//!
//! ## Dynamic Pool with Lazy Loading
//!
//! For applications that discover chains at runtime:
//!
//! ```rust,ignore
//! use semioscan::provider::ProviderPool;
//! use alloy_chains::NamedChain;
//!
//! let mut pool = ProviderPool::new();
//!
//! // Add providers as needed
//! pool.add(NamedChain::Mainnet, "https://eth.llamarpc.com", None)?;
//! pool.add(NamedChain::Base, "https://mainnet.base.org", Some(10))?;
//!
//! // Access providers
//! if let Some(provider) = pool.get(NamedChain::Mainnet) {
//!     let block = provider.get_block_number().await?;
//! }
//! ```
//!
//! ## With Preset Configurations
//!
//! ```rust,ignore
//! use semioscan::provider::{ProviderPool, ChainEndpoint};
//!
//! let endpoints = vec![
//!     ChainEndpoint::mainnet("https://eth.llamarpc.com"),
//!     ChainEndpoint::base("https://mainnet.base.org"),
//!     ChainEndpoint::optimism("https://mainnet.optimism.io"),
//! ];
//!
//! let pool = ProviderPool::from_endpoints(endpoints, Some(10))?;
//! ```

use alloy_chains::NamedChain;
use alloy_network::AnyNetwork;
use alloy_provider::{ProviderBuilder, RootProvider};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::errors::RpcError;
use crate::transport::RateLimitLayer;

/// Type alias for a pooled provider using `AnyNetwork`
pub type PooledProvider = Arc<RootProvider<AnyNetwork>>;

/// A thread-safe pool of providers indexed by chain
///
/// The pool uses read-write locks for efficient concurrent access:
/// - Multiple readers can access providers simultaneously
/// - Writers acquire exclusive access when adding new providers
///
/// # Thread Safety
///
/// The pool is safe to share across threads via `Arc<ProviderPool>` or
/// by storing it in a static variable with `LazyLock`.
#[derive(Debug, Default)]
pub struct ProviderPool {
    /// Map of chain to provider
    providers: RwLock<HashMap<NamedChain, PooledProvider>>,
    /// Default rate limit for new providers (requests per second)
    default_rate_limit: Option<u32>,
}

impl ProviderPool {
    /// Create a new empty provider pool
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            default_rate_limit: None,
        }
    }

    /// Create a pool with default rate limit for new providers
    #[must_use]
    pub fn with_defaults(rate_limit: Option<u32>) -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            default_rate_limit: rate_limit,
        }
    }

    /// Create a pool from a list of chain endpoints
    ///
    /// # Errors
    ///
    /// Returns an error if any endpoint URL is invalid
    pub fn from_endpoints(
        endpoints: Vec<ChainEndpoint>,
        rate_limit: Option<u32>,
    ) -> Result<Self, RpcError> {
        let pool = Self::with_defaults(rate_limit);
        for endpoint in endpoints {
            pool.add(
                endpoint.chain,
                &endpoint.url,
                endpoint.rate_limit.or(rate_limit),
            )?;
        }
        Ok(pool)
    }

    /// Add a provider for a specific chain
    ///
    /// If a provider already exists for this chain, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `chain` - The chain to add the provider for
    /// * `url` - The RPC endpoint URL
    /// * `rate_limit` - Optional rate limit in requests per second
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid
    pub fn add(
        &self,
        chain: NamedChain,
        url: &str,
        rate_limit: Option<u32>,
    ) -> Result<(), RpcError> {
        let provider = create_pooled_provider(url, rate_limit.or(self.default_rate_limit))?;

        let mut providers = self.providers.write().map_err(|_| {
            RpcError::ProviderConnectionFailed("Provider pool lock poisoned".to_string())
        })?;

        if providers.contains_key(&chain) {
            debug!(chain = ?chain, "Replacing existing provider");
        } else {
            info!(chain = ?chain, url = url, "Added provider to pool");
        }

        providers.insert(chain, Arc::new(provider));
        Ok(())
    }

    /// Get a provider for a specific chain
    ///
    /// Returns `None` if no provider is configured for the chain.
    #[must_use]
    pub fn get(&self, chain: NamedChain) -> Option<PooledProvider> {
        self.providers
            .read()
            .ok()
            .and_then(|providers| providers.get(&chain).cloned())
    }

    /// Get a provider for a chain, or add it if not present
    ///
    /// This is useful for lazy initialization of providers.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid and the provider needs to be created
    pub fn get_or_add(
        &self,
        chain: NamedChain,
        url: &str,
        rate_limit: Option<u32>,
    ) -> Result<PooledProvider, RpcError> {
        // Try read lock first for better concurrency
        if let Some(provider) = self.get(chain) {
            return Ok(provider);
        }

        // Provider not found, need to add it
        self.add(chain, url, rate_limit)?;
        self.get(chain).ok_or_else(|| {
            RpcError::ProviderConnectionFailed("Failed to retrieve newly added provider".into())
        })
    }

    /// Remove a provider from the pool
    ///
    /// Returns the removed provider if it existed.
    pub fn remove(&self, chain: NamedChain) -> Option<PooledProvider> {
        self.providers
            .write()
            .ok()
            .and_then(|mut providers| providers.remove(&chain))
    }

    /// Check if a provider exists for a chain
    #[must_use]
    pub fn contains(&self, chain: NamedChain) -> bool {
        self.providers
            .read()
            .ok()
            .is_some_and(|providers| providers.contains_key(&chain))
    }

    /// Get the number of providers in the pool
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers
            .read()
            .map(|providers| providers.len())
            .unwrap_or(0)
    }

    /// Check if the pool is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all configured chains
    #[must_use]
    pub fn chains(&self) -> Vec<NamedChain> {
        self.providers
            .read()
            .map(|providers| providers.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Clear all providers from the pool
    pub fn clear(&self) {
        if let Ok(mut providers) = self.providers.write() {
            providers.clear();
            info!("Cleared all providers from pool");
        }
    }
}

/// Builder for creating provider pools with common configurations
#[derive(Default)]
pub struct ProviderPoolBuilder {
    endpoints: Vec<ChainEndpoint>,
    default_rate_limit: Option<u32>,
}

impl ProviderPoolBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a chain endpoint to the pool
    #[must_use]
    pub fn add_chain(mut self, chain: NamedChain, url: &str) -> Self {
        self.endpoints.push(ChainEndpoint {
            chain,
            url: url.to_string(),
            rate_limit: None,
        });
        self
    }

    /// Add a chain endpoint with a specific rate limit
    #[must_use]
    pub fn add_chain_with_rate_limit(
        mut self,
        chain: NamedChain,
        url: &str,
        rate_limit: u32,
    ) -> Self {
        self.endpoints.push(ChainEndpoint {
            chain,
            url: url.to_string(),
            rate_limit: Some(rate_limit),
        });
        self
    }

    /// Set the default rate limit for all providers
    #[must_use]
    pub fn with_rate_limit(mut self, requests_per_second: u32) -> Self {
        self.default_rate_limit = Some(requests_per_second);
        self
    }

    /// Build the provider pool
    ///
    /// # Errors
    ///
    /// Returns an error if any endpoint URL is invalid
    pub fn build(self) -> Result<ProviderPool, RpcError> {
        let pool = ProviderPool::with_defaults(self.default_rate_limit);

        for endpoint in self.endpoints {
            pool.add(
                endpoint.chain,
                &endpoint.url,
                endpoint.rate_limit.or(self.default_rate_limit),
            )?;
        }

        Ok(pool)
    }
}

/// Configuration for a chain endpoint
#[derive(Debug, Clone)]
pub struct ChainEndpoint {
    /// The chain this endpoint serves
    pub chain: NamedChain,
    /// The RPC endpoint URL
    pub url: String,
    /// Optional rate limit override for this specific chain
    pub rate_limit: Option<u32>,
}

impl ChainEndpoint {
    /// Create a new chain endpoint
    #[must_use]
    pub fn new(chain: NamedChain, url: impl Into<String>) -> Self {
        Self {
            chain,
            url: url.into(),
            rate_limit: None,
        }
    }

    /// Create a new chain endpoint with rate limiting
    #[must_use]
    pub fn with_rate_limit(mut self, rate_limit: u32) -> Self {
        self.rate_limit = Some(rate_limit);
        self
    }

    /// Create an Ethereum mainnet endpoint
    #[must_use]
    pub fn mainnet(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Mainnet, url)
    }

    /// Create a Base mainnet endpoint
    #[must_use]
    pub fn base(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Base, url)
    }

    /// Create an Optimism mainnet endpoint
    #[must_use]
    pub fn optimism(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Optimism, url)
    }

    /// Create an Arbitrum One endpoint
    #[must_use]
    pub fn arbitrum(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Arbitrum, url)
    }

    /// Create a Polygon mainnet endpoint
    #[must_use]
    pub fn polygon(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Polygon, url)
    }

    /// Create a Sepolia testnet endpoint
    #[must_use]
    pub fn sepolia(url: impl Into<String>) -> Self {
        Self::new(NamedChain::Sepolia, url)
    }
}

/// Create a pooled provider with optional rate limiting
///
/// Returns a bare `RootProvider` without fillers, as fillers are typically
/// application-specific and should be added by the consumer if needed.
///
/// Note: RPC request/response logging is handled natively by alloy's transport
/// layer at DEBUG/TRACE level.
fn create_pooled_provider(
    url: &str,
    rate_limit: Option<u32>,
) -> Result<RootProvider<AnyNetwork>, RpcError> {
    let parsed_url: url::Url = url.parse().map_err(|e| {
        warn!(url = url, error = ?e, "Invalid provider URL");
        RpcError::ProviderUrlInvalid(url.to_string())
    })?;

    // Build the RPC client with optional rate limiting
    let client = match rate_limit {
        Some(limit) => alloy_rpc_client::ClientBuilder::default()
            .layer(RateLimitLayer::per_second(limit))
            .http(parsed_url),
        None => alloy_rpc_client::ClientBuilder::default().http(parsed_url),
    };

    // Create a bare provider without fillers - fillers are application-specific
    // and should be added by consumers if needed
    let provider = ProviderBuilder::new()
        .disable_recommended_fillers()
        .network::<AnyNetwork>()
        .connect_client(client);

    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_new() {
        let pool = ProviderPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_pool_with_defaults() {
        let pool = ProviderPool::with_defaults(Some(10));
        assert_eq!(pool.default_rate_limit, Some(10));
    }

    #[test]
    fn test_chain_endpoint_constructors() {
        let endpoint = ChainEndpoint::mainnet("https://eth.llamarpc.com");
        assert_eq!(endpoint.chain, NamedChain::Mainnet);
        assert_eq!(endpoint.url, "https://eth.llamarpc.com");
        assert!(endpoint.rate_limit.is_none());

        let endpoint = ChainEndpoint::base("https://mainnet.base.org").with_rate_limit(5);
        assert_eq!(endpoint.chain, NamedChain::Base);
        assert_eq!(endpoint.rate_limit, Some(5));
    }

    #[test]
    fn test_pool_builder() {
        let builder = ProviderPoolBuilder::new()
            .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
            .add_chain_with_rate_limit(NamedChain::Base, "https://mainnet.base.org", 5)
            .with_rate_limit(10);

        assert_eq!(builder.endpoints.len(), 2);
        assert_eq!(builder.default_rate_limit, Some(10));
    }

    #[test]
    fn test_pool_contains_and_chains() {
        let pool = ProviderPool::new();

        // Add a provider (using a valid URL format)
        let result = pool.add(NamedChain::Mainnet, "https://eth.llamarpc.com", None);
        assert!(result.is_ok());

        assert!(pool.contains(NamedChain::Mainnet));
        assert!(!pool.contains(NamedChain::Base));

        let chains = pool.chains();
        assert_eq!(chains.len(), 1);
        assert!(chains.contains(&NamedChain::Mainnet));
    }

    #[test]
    fn test_pool_remove() {
        let pool = ProviderPool::new();
        pool.add(NamedChain::Mainnet, "https://eth.llamarpc.com", None)
            .unwrap();

        assert!(pool.contains(NamedChain::Mainnet));

        let removed = pool.remove(NamedChain::Mainnet);
        assert!(removed.is_some());
        assert!(!pool.contains(NamedChain::Mainnet));

        // Removing again should return None
        let removed_again = pool.remove(NamedChain::Mainnet);
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_pool_clear() {
        let pool = ProviderPool::new();
        pool.add(NamedChain::Mainnet, "https://eth.llamarpc.com", None)
            .unwrap();
        pool.add(NamedChain::Base, "https://mainnet.base.org", None)
            .unwrap();

        assert_eq!(pool.len(), 2);

        pool.clear();
        assert!(pool.is_empty());
    }

    #[test]
    fn test_invalid_url() {
        let pool = ProviderPool::new();
        let result = pool.add(NamedChain::Mainnet, "not a valid url", None);
        assert!(result.is_err());
    }
}
