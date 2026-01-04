// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Dynamic provider utilities for runtime chain selection
//!
//! This module provides utilities for creating type-erased providers that enable
//! runtime chain selection without compile-time network type constraints.
//!
//! # Overview
//!
//! Semioscan's calculators are generic over `Provider<N: Network>`, which provides
//! excellent type safety and performance through static dispatch. However, some
//! applications need to select chains at runtime.
//!
//! This module provides:
//! - [`create_http_provider`] - Create an HTTP provider with optional rate limiting
//! - [`create_ws_provider`] - Create a WebSocket provider for real-time subscriptions (requires `ws` feature)
//! - [`ChainProvider`] - Enum-based provider for common chains
//!
//! # When to Use Dynamic Providers
//!
//! Use dynamic providers when:
//! - Chain selection happens at runtime (user input, configuration)
//! - Building multi-chain applications with shared logic
//! - Simplicity matters more than maximum performance
//!
//! Prefer generic `Provider<N>` when:
//! - Chain is known at compile time
//! - Maximum performance is critical
//! - Working with network-specific features
//!
//! # Examples
//!
//! ## HTTP Provider with Rate Limiting
//!
//! ```rust,ignore
//! use semioscan::provider::{create_http_provider, ProviderConfig};
//! use alloy_chains::NamedChain;
//!
//! // Create a rate-limited provider
//! let config = ProviderConfig::new("https://eth.llamarpc.com")
//!     .with_rate_limit(10); // 10 requests per second
//!
//! let provider = create_http_provider(config)?;
//!
//! // Use with any network-agnostic operations
//! let block_number = provider.get_block_number().await?;
//! ```
//!
//! ## Runtime Chain Selection
//!
//! ```rust,ignore
//! use semioscan::provider::{ChainProvider, create_provider_for_chain};
//! use alloy_chains::NamedChain;
//!
//! async fn get_block_for_chain(chain: NamedChain, rpc_url: &str) -> Result<u64, Error> {
//!     let provider = create_provider_for_chain(chain, rpc_url)?;
//!     let block = provider.get_block_number().await?;
//!     Ok(block)
//! }
//! ```
//!
//! # AnyNetwork vs Specific Networks
//!
//! This module primarily uses `AnyNetwork` for type erasure. This works for most
//! read operations but has limitations:
//!
//! - Network-specific receipt fields (like OP-stack L1 fees) require manual extraction
//! - Some operations may fail on incompatible networks
//!
//! For full network-specific support, use the generic calculators with explicit
//! `Ethereum` or `Optimism` network types.

mod config;
mod factory;
mod pool;

pub use config::ProviderConfig;
#[cfg(feature = "ws")]
pub use factory::create_ws_provider;
pub use factory::{
    create_http_provider, create_typed_http_provider, rate_limited_http_provider,
    simple_http_provider,
};
pub use pool::{ChainEndpoint, PooledProvider, ProviderPool, ProviderPoolBuilder};

use alloy_chains::NamedChain;
use alloy_network::{AnyNetwork, Ethereum};
use op_alloy_network::Optimism;
use std::sync::Arc;

/// Type alias for an HTTP provider using AnyNetwork
///
/// This provider can interact with any EVM chain but loses network-specific type information.
/// Use this when you need runtime flexibility over compile-time type safety.
pub type AnyHttpProvider = alloy_provider::RootProvider<AnyNetwork>;

/// Type alias for an HTTP provider using Ethereum network
pub type EthereumHttpProvider = alloy_provider::RootProvider<Ethereum>;

/// Type alias for an HTTP provider using Optimism network (OP-stack chains)
pub type OptimismHttpProvider = alloy_provider::RootProvider<Optimism>;

/// Determines the appropriate network type for a given chain
///
/// This function categorizes chains into their network types:
/// - Ethereum mainnet and testnets use `Ethereum`
/// - OP-stack chains (Optimism, Base, Mode, etc.) use `Optimism`
/// - Unknown chains default to `AnyNetwork` behavior
#[must_use]
pub fn network_type_for_chain(chain: NamedChain) -> NetworkType {
    match chain {
        // Ethereum L1 and testnets
        NamedChain::Mainnet
        | NamedChain::Sepolia
        | NamedChain::Holesky
        | NamedChain::Goerli
        | NamedChain::Polygon
        | NamedChain::PolygonAmoy
        | NamedChain::Arbitrum
        | NamedChain::ArbitrumSepolia
        | NamedChain::ArbitrumGoerli
        | NamedChain::ArbitrumNova => NetworkType::Ethereum,

        // OP-stack chains
        NamedChain::Optimism
        | NamedChain::OptimismSepolia
        | NamedChain::OptimismGoerli
        | NamedChain::Base
        | NamedChain::BaseSepolia
        | NamedChain::BaseGoerli
        | NamedChain::Mode
        | NamedChain::ModeSepolia
        | NamedChain::Fraxtal
        | NamedChain::FraxtalTestnet
        | NamedChain::Zora
        | NamedChain::ZoraSepolia => NetworkType::Optimism,

        // Default to Ethereum for other chains
        _ => NetworkType::Ethereum,
    }
}

/// Network type categorization for runtime chain selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkType {
    /// Ethereum mainnet and compatible L1/L2 chains
    Ethereum,
    /// OP-stack chains (Optimism, Base, Mode, Fraxtal, Zora)
    Optimism,
}

impl NetworkType {
    /// Returns true if this network type has L1 data fees
    #[must_use]
    pub fn has_l1_data_fees(&self) -> bool {
        matches!(self, Self::Optimism)
    }

    /// Returns the human-readable name of the network type
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ethereum => "Ethereum",
            Self::Optimism => "Optimism",
        }
    }
}

/// A chain-aware provider wrapper that tracks the chain being used
///
/// This struct wraps a provider and stores chain metadata, useful for
/// applications that need to track which chain a provider is connected to.
#[derive(Clone)]
pub struct ChainAwareProvider<P> {
    provider: P,
    chain: NamedChain,
    network_type: NetworkType,
}

impl<P> ChainAwareProvider<P> {
    /// Create a new chain-aware provider
    pub fn new(provider: P, chain: NamedChain) -> Self {
        Self {
            provider,
            network_type: network_type_for_chain(chain),
            chain,
        }
    }

    /// Get the chain this provider is connected to
    #[must_use]
    pub fn chain(&self) -> NamedChain {
        self.chain
    }

    /// Get the network type for this chain
    #[must_use]
    pub fn network_type(&self) -> NetworkType {
        self.network_type
    }

    /// Get a reference to the inner provider
    #[must_use]
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Consume and return the inner provider
    pub fn into_inner(self) -> P {
        self.provider
    }

    /// Check if this chain has L1 data fees
    #[must_use]
    pub fn has_l1_data_fees(&self) -> bool {
        self.network_type.has_l1_data_fees()
    }
}

impl<P> std::ops::Deref for ChainAwareProvider<P> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        &self.provider
    }
}

/// Builder for creating providers with common configurations
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::provider::ProviderBuilder;
///
/// let provider = ProviderBuilder::new()
///     .http("https://eth.llamarpc.com")
///     .with_rate_limit(10)
///     .build()?;
/// ```
///
/// Note: RPC request/response logging is handled natively by alloy's transport
/// layer at DEBUG/TRACE level.
pub struct DynProviderBuilder {
    rate_limit_per_second: Option<u32>,
    timeout_ms: Option<u64>,
}

impl Default for DynProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DynProviderBuilder {
    /// Create a new provider builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            rate_limit_per_second: None,
            timeout_ms: None,
        }
    }

    /// Set rate limiting (requests per second)
    #[must_use]
    pub fn with_rate_limit(mut self, requests_per_second: u32) -> Self {
        self.rate_limit_per_second = Some(requests_per_second);
        self
    }

    /// Set request timeout in milliseconds
    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Build an HTTP provider for the specified chain
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid
    pub fn build_http_for_chain(
        self,
        url: &str,
        chain: NamedChain,
    ) -> Result<ChainAwareProvider<AnyHttpProvider>, crate::errors::RpcError> {
        let config = ProviderConfig::new(url).with_rate_limit_opt(self.rate_limit_per_second);

        let provider = create_http_provider(config)?;
        Ok(ChainAwareProvider::new(provider, chain))
    }

    /// Build a generic HTTP provider using AnyNetwork
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid
    pub fn build_http(self, url: &str) -> Result<AnyHttpProvider, crate::errors::RpcError> {
        let config = ProviderConfig::new(url).with_rate_limit_opt(self.rate_limit_per_second);

        create_http_provider(config)
    }
}

/// Shared provider reference for use across multiple calculators
///
/// When you need to share a provider across multiple calculator instances,
/// wrap it in `Arc` for efficient cloning without duplication.
pub type SharedProvider<P> = Arc<P>;

/// Create a shared provider that can be used across multiple calculators
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::provider::{create_http_provider, ProviderConfig, share_provider};
/// use semioscan::GasCostCalculator;
///
/// let provider = create_http_provider(ProviderConfig::new("https://eth.llamarpc.com"))?;
/// let shared = share_provider(provider);
///
/// // Use the same provider in multiple calculators
/// let gas_calc = GasCostCalculator::new(shared.clone());
/// let other_calc = GasCostCalculator::new(shared.clone());
/// ```
pub fn share_provider<P>(provider: P) -> SharedProvider<P> {
    Arc::new(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_type_for_chain_ethereum() {
        assert_eq!(
            network_type_for_chain(NamedChain::Mainnet),
            NetworkType::Ethereum
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Sepolia),
            NetworkType::Ethereum
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Arbitrum),
            NetworkType::Ethereum
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Polygon),
            NetworkType::Ethereum
        );
    }

    #[test]
    fn test_network_type_for_chain_optimism() {
        assert_eq!(
            network_type_for_chain(NamedChain::Optimism),
            NetworkType::Optimism
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Base),
            NetworkType::Optimism
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Mode),
            NetworkType::Optimism
        );
        assert_eq!(
            network_type_for_chain(NamedChain::Zora),
            NetworkType::Optimism
        );
    }

    #[test]
    fn test_network_type_l1_fees() {
        assert!(!NetworkType::Ethereum.has_l1_data_fees());
        assert!(NetworkType::Optimism.has_l1_data_fees());
    }

    #[test]
    fn test_network_type_name() {
        assert_eq!(NetworkType::Ethereum.name(), "Ethereum");
        assert_eq!(NetworkType::Optimism.name(), "Optimism");
    }

    #[test]
    fn test_dyn_provider_builder_defaults() {
        let builder = DynProviderBuilder::new();
        assert!(builder.rate_limit_per_second.is_none());
        assert!(builder.timeout_ms.is_none());
    }

    #[test]
    fn test_dyn_provider_builder_config() {
        let builder = DynProviderBuilder::new()
            .with_rate_limit(10)
            .with_timeout_ms(5000);

        assert_eq!(builder.rate_limit_per_second, Some(10));
        assert_eq!(builder.timeout_ms, Some(5000));
    }
}
