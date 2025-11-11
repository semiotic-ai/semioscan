//! Provider abstraction for blockchain access
//!
//! This module provides read-only provider creation for Ethereum and OP Stack chains.
//! It handles RPC URL configuration from environment variables and chain-specific features.

use alloy_chains::NamedChain;
use alloy_network::Ethereum;
use alloy_provider::RootProvider;
use alloy_rpc_client::RpcClient;
use alloy_transport_http::Http;
use dotenvy::var;
use op_alloy_network::Optimism;
use thiserror::Error;

/// Errors that can occur when creating providers
#[derive(Debug, Error)]
pub enum ProviderError {
    /// Chain is not supported by semioscan
    #[error("Unsupported chain: {0:?}")]
    UnsupportedChain(NamedChain),

    /// Required environment variable is missing
    #[error("Missing environment variable {0}: {1}")]
    MissingEnvVar(&'static str, #[source] dotenvy::Error),

    /// Invalid RPC URL format
    #[error("Invalid URL {0}: {1}")]
    InvalidUrl(String, #[source] url::ParseError),
}

/// Extension trait for chain-specific features
pub trait ChainFeatures {
    /// OP Stack chains have L1 data fees (Base, Optimism, Mode, etc.)
    fn has_l1_fees(&self) -> bool;

    /// OP Stack chains need Optimism network type
    fn is_op_stack(&self) -> bool {
        self.has_l1_fees()
    }
}

impl ChainFeatures for NamedChain {
    fn has_l1_fees(&self) -> bool {
        use NamedChain::*;
        matches!(
            self,
            Base | Optimism | Fraxtal | Mantle | Mode | Scroll | Unichain
        )
    }
}

/// Get environment variable name for a chain's RPC URL
pub fn rpc_url_env_var(chain: NamedChain) -> Result<&'static str, ProviderError> {
    use NamedChain::*;

    match chain {
        Arbitrum => Ok("ARBITRUM_RPC_URL"),
        Avalanche => Ok("AVALANCHE_RPC_URL"),
        Base => Ok("BASE_RPC_URL"),
        BinanceSmartChain => Ok("BNB_RPC_URL"),
        Fraxtal => Ok("FRAXTAL_RPC_URL"),
        Linea => Ok("LINEA_RPC_URL"),
        Mainnet => Ok("ETHEREUM_RPC_URL"),
        Mantle => Ok("MANTLE_RPC_URL"),
        Mode => Ok("MODE_RPC_URL"),
        Optimism => Ok("OPTIMISM_RPC_URL"),
        Polygon => Ok("POLYGON_RPC_URL"),
        Scroll => Ok("SCROLL_RPC_URL"),
        Sonic => Ok("SONIC_RPC_URL"),
        _ => Err(ProviderError::UnsupportedChain(chain)),
    }
}

/// Build RPC URL from environment variables
pub fn build_rpc_url(chain: NamedChain) -> Result<String, ProviderError> {
    let url_var = rpc_url_env_var(chain)?;
    let api_url = var(url_var).map_err(|e| ProviderError::MissingEnvVar(url_var, e))?;

    // Handle chain-specific API key patterns
    let api_key_var = match chain {
        NamedChain::Fraxtal => "FRAXTAL_API_KEY",
        NamedChain::Mode => "MODE_API_KEY",
        NamedChain::Sonic => "SONIC_API_KEY",
        _ => "API_KEY", // Default for most chains
    };

    let api_key = var(api_key_var).map_err(|e| ProviderError::MissingEnvVar(api_key_var, e))?;

    // Special case: Avalanche has different endpoint structure
    let provider_url = if matches!(chain, NamedChain::Avalanche) {
        format!("{api_url}{api_key}/ext/bc/C/rpc")
    } else {
        format!("{api_url}{api_key}/")
    };

    Ok(provider_url)
}

/// Create read-only provider for Ethereum-compatible chains
///
/// Reads RPC URL from environment variables. See [`build_rpc_url`] for required env vars.
///
/// # Example
///
/// ```no_run
/// use alloy_chains::NamedChain;
/// use alloy_provider::Provider;
/// use semioscan::provider::create_ethereum_provider;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = create_ethereum_provider(NamedChain::Arbitrum)?;
/// let block_number = provider.get_block_number().await?;
/// # Ok(())
/// # }
/// ```
pub fn create_ethereum_provider(
    chain: NamedChain,
) -> Result<RootProvider<Ethereum>, ProviderError> {
    let rpc_url = build_rpc_url(chain)?;
    create_ethereum_provider_from_url(rpc_url)
}

/// Create Ethereum provider from explicit RPC URL
///
/// Useful for testing or when RPC URLs are provided directly.
///
/// # Example
///
/// ```no_run
/// use semioscan::provider::create_ethereum_provider_from_url;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = create_ethereum_provider_from_url(
///     "https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY".to_string()
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn create_ethereum_provider_from_url(
    rpc_url: String,
) -> Result<RootProvider<Ethereum>, ProviderError> {
    let http = Http::new(
        rpc_url
            .parse()
            .map_err(|e| ProviderError::InvalidUrl(rpc_url.clone(), e))?,
    );
    let client = RpcClient::new(http, false);
    Ok(RootProvider::new(client))
}

/// Create read-only provider for OP Stack chains (Base, Optimism, Mode, etc.)
///
/// Reads RPC URL from environment variables. See [`build_rpc_url`] for required env vars.
///
/// # Example
///
/// ```no_run
/// use alloy_chains::NamedChain;
/// use alloy_provider::Provider;
/// use semioscan::provider::create_optimism_provider;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = create_optimism_provider(NamedChain::Base)?;
/// let block_number = provider.get_block_number().await?;
/// # Ok(())
/// # }
/// ```
pub fn create_optimism_provider(
    chain: NamedChain,
) -> Result<RootProvider<Optimism>, ProviderError> {
    let rpc_url = build_rpc_url(chain)?;
    create_optimism_provider_from_url(rpc_url)
}

/// Create Optimism provider from explicit RPC URL
///
/// Useful for testing or when RPC URLs are provided directly.
///
/// # Example
///
/// ```no_run
/// use semioscan::provider::create_optimism_provider_from_url;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let provider = create_optimism_provider_from_url(
///     "https://base-mainnet.g.alchemy.com/v2/YOUR_KEY".to_string()
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn create_optimism_provider_from_url(
    rpc_url: String,
) -> Result<RootProvider<Optimism>, ProviderError> {
    let http = Http::new(
        rpc_url
            .parse()
            .map_err(|e| ProviderError::InvalidUrl(rpc_url.clone(), e))?,
    );
    let client = RpcClient::new(http, false);
    Ok(RootProvider::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_features_op_stack() {
        use NamedChain::*;

        // OP Stack chains should have L1 fees
        assert!(Base.has_l1_fees());
        assert!(Optimism.has_l1_fees());
        assert!(Mode.has_l1_fees());
        assert!(Fraxtal.has_l1_fees());
        assert!(Mantle.has_l1_fees());
        assert!(Scroll.has_l1_fees());
        assert!(Unichain.has_l1_fees());
    }

    #[test]
    fn test_chain_features_non_op_stack() {
        use NamedChain::*;

        // Non-OP Stack chains should not have L1 fees
        assert!(!Arbitrum.has_l1_fees());
        assert!(!Polygon.has_l1_fees());
        assert!(!Mainnet.has_l1_fees());
        assert!(!Avalanche.has_l1_fees());
        assert!(!BinanceSmartChain.has_l1_fees());
    }

    #[test]
    fn test_is_op_stack_matches_has_l1_fees() {
        use NamedChain::*;

        for chain in &[Base, Optimism, Mode, Fraxtal, Arbitrum, Polygon, Mainnet] {
            assert_eq!(
                chain.has_l1_fees(),
                chain.is_op_stack(),
                "is_op_stack should match has_l1_fees for {chain:?}"
            );
        }
    }

    #[test]
    fn test_rpc_url_env_var() {
        use NamedChain::*;

        assert_eq!(rpc_url_env_var(Base).unwrap(), "BASE_RPC_URL");
        assert_eq!(rpc_url_env_var(Arbitrum).unwrap(), "ARBITRUM_RPC_URL");
        assert_eq!(rpc_url_env_var(Mainnet).unwrap(), "ETHEREUM_RPC_URL");
        assert_eq!(rpc_url_env_var(Polygon).unwrap(), "POLYGON_RPC_URL");
        assert_eq!(rpc_url_env_var(Optimism).unwrap(), "OPTIMISM_RPC_URL");
    }

    #[test]
    fn test_unsupported_chain() {
        use NamedChain::*;

        // Example of unsupported chain
        let result = rpc_url_env_var(Celo);
        assert!(matches!(result, Err(ProviderError::UnsupportedChain(_))));
    }
}
