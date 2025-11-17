//! Configuration for semioscan operations
//!
//! This module provides a flexible configuration system for controlling
//! semioscan's RPC behavior, rate limiting, and block range limits.
//!
//! # Example: Using defaults
//!
//! ```rust
//! use semioscan::SemioscanConfig;
//!
//! // Uses common defaults optimized for Alchemy/Infura
//! let config = SemioscanConfig::default();
//! ```
//!
//! # Example: Custom configuration
//!
//! ```rust
//! use semioscan::{SemioscanConfig, SemioscanConfigBuilder};
//! use std::time::Duration;
//! use alloy_chains::NamedChain;
//!
//! let config = SemioscanConfigBuilder::with_defaults()
//!     .max_block_range(1000)  // Query up to 1000 blocks at once
//!     .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
//!     .build();
//! ```
//!
//! # Example: Premium RPC (no delays)
//!
//! ```rust
//! use semioscan::SemioscanConfig;
//!
//! // For premium RPC providers with higher rate limits
//! let config = SemioscanConfig::minimal();
//! ```

use std::collections::HashMap;
use std::time::Duration;

use alloy_chains::NamedChain;

use crate::types::config::MaxBlockRange;

pub mod constants;

/// Configuration for semioscan operations
///
/// Controls RPC behavior including block range limits, rate limiting, and timeouts.
/// Use [`SemioscanConfigBuilder`] for a fluent API to construct instances.
#[derive(Debug, Clone)]
pub struct SemioscanConfig {
    /// Maximum number of blocks to query in a single RPC call
    /// Default: 500 (safe for most RPC providers)
    pub max_block_range: MaxBlockRange,

    /// Delay between RPC requests to avoid rate limiting
    /// Default: None (no delay)
    pub rate_limit_delay: Option<Duration>,

    /// Timeout for RPC requests
    /// Default: 30 seconds (prevents hanging on unresponsive providers)
    pub rpc_timeout: Duration,

    /// Chain-specific overrides
    pub chain_overrides: HashMap<NamedChain, ChainConfig>,
}

/// Chain-specific configuration overrides
///
/// Allows per-chain customization of block ranges, rate limits, and timeouts.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    /// Override max block range for this chain
    pub max_block_range: Option<MaxBlockRange>,

    /// Override rate limit delay for this chain
    pub rate_limit_delay: Option<Duration>,

    /// Override RPC timeout for this chain
    pub rpc_timeout: Option<Duration>,
}

impl Default for SemioscanConfig {
    fn default() -> Self {
        Self::with_common_defaults()
    }
}

impl SemioscanConfig {
    /// Create config with defaults optimized for common RPC providers
    ///
    /// This configuration includes sensible defaults for Alchemy, Infura, and QuickNode,
    /// with chain-specific overrides for Base and Sonic which tend to have stricter rate limits.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfig;
    ///
    /// let config = SemioscanConfig::with_common_defaults();
    /// // Base and Sonic will have 250ms delay between requests
    /// // All chains use 500 block range by default
    /// ```
    pub fn with_common_defaults() -> Self {
        let mut config = Self {
            max_block_range: MaxBlockRange::new(500),
            rate_limit_delay: None,
            rpc_timeout: Duration::from_secs(30), // 30 second default timeout
            chain_overrides: HashMap::new(),
        };

        // Base: Alchemy tends to be stricter, add delay
        config.set_chain_override(
            NamedChain::Base,
            ChainConfig {
                max_block_range: None, // Use default 500
                rate_limit_delay: Some(Duration::from_millis(250)),
                rpc_timeout: None, // Use default timeout
            },
        );

        // Sonic: Known to have strict rate limits
        config.set_chain_override(
            NamedChain::Sonic,
            ChainConfig {
                max_block_range: None,
                rate_limit_delay: Some(Duration::from_millis(250)),
                rpc_timeout: None, // Use default timeout
            },
        );

        config
    }

    /// Create minimal config with no delays
    ///
    /// Suitable for testing or premium RPC endpoints with generous rate limits.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfig;
    ///
    /// let config = SemioscanConfig::minimal();
    /// // No rate limiting, 500 block range, 30s timeout
    /// ```
    pub fn minimal() -> Self {
        Self {
            max_block_range: MaxBlockRange::new(500),
            rate_limit_delay: None,
            rpc_timeout: Duration::from_secs(30), // Still include timeout for safety
            chain_overrides: HashMap::new(),
        }
    }

    /// Get effective max block range for a specific chain
    ///
    /// Returns chain-specific override if set, otherwise returns global default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{SemioscanConfig, SemioscanConfigBuilder, ChainConfig, MaxBlockRange};
    /// use alloy_chains::NamedChain;
    ///
    /// let mut config = SemioscanConfig::minimal();
    /// config.set_chain_override(
    ///     NamedChain::Arbitrum,
    ///     ChainConfig {
    ///         max_block_range: Some(MaxBlockRange::new(1000)),
    ///         rate_limit_delay: None,
    ///         rpc_timeout: None,
    ///     },
    ///     );
    ///
    /// assert_eq!(config.get_max_block_range(NamedChain::Arbitrum), MaxBlockRange::new(1000));
    /// assert_eq!(config.get_max_block_range(NamedChain::Base), MaxBlockRange::new(500)); // Default
    /// ```
    pub fn get_max_block_range(&self, chain: NamedChain) -> MaxBlockRange {
        self.chain_overrides
            .get(&chain)
            .and_then(|c| c.max_block_range)
            .unwrap_or(self.max_block_range)
    }

    /// Get effective rate limit delay for a specific chain
    ///
    /// Returns chain-specific override if set, otherwise returns global default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfig;
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfig::default();
    ///
    /// // Base has chain-specific delay
    /// assert_eq!(
    ///     config.get_rate_limit_delay(NamedChain::Base),
    ///     Some(Duration::from_millis(250))
    /// );
    ///
    /// // Arbitrum uses global default (none)
    /// assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
    /// ```
    pub fn get_rate_limit_delay(&self, chain: NamedChain) -> Option<Duration> {
        self.chain_overrides
            .get(&chain)
            .and_then(|c| c.rate_limit_delay)
            .or(self.rate_limit_delay)
    }

    /// Get effective RPC timeout for a specific chain
    ///
    /// Returns chain-specific override if set, otherwise returns global default.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfig;
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfig::default();
    ///
    /// // All chains use default 30s timeout
    /// assert_eq!(
    ///     config.get_rpc_timeout(NamedChain::Arbitrum),
    ///     Duration::from_secs(30)
    /// );
    /// ```
    pub fn get_rpc_timeout(&self, chain: NamedChain) -> Duration {
        self.chain_overrides
            .get(&chain)
            .and_then(|c| c.rpc_timeout)
            .unwrap_or(self.rpc_timeout)
    }

    /// Set chain-specific override
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{SemioscanConfig, ChainConfig, MaxBlockRange};
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let mut config = SemioscanConfig::minimal();
    /// config.set_chain_override(
    ///     NamedChain::Polygon,
    ///     ChainConfig {
    ///         max_block_range: Some(MaxBlockRange::new(2000)),
    ///         rate_limit_delay: Some(Duration::from_millis(500)),
    ///         rpc_timeout: None,
    ///     },
    /// );
    /// ```
    pub fn set_chain_override(&mut self, chain: NamedChain, config: ChainConfig) {
        self.chain_overrides.insert(chain, config);
    }
}

/// Builder for [`SemioscanConfig`]
///
/// Provides a fluent API for constructing semioscan configurations.
///
/// # Example
///
/// ```rust
/// use semioscan::SemioscanConfigBuilder;
/// use alloy_chains::NamedChain;
/// use std::time::Duration;
///
/// let config = SemioscanConfigBuilder::new()
///     .max_block_range(1000)
///     .rate_limit_delay(Duration::from_millis(500))
///     .chain_rate_limit(NamedChain::Base, Duration::from_millis(250))
///     .build();
/// ```
pub struct SemioscanConfigBuilder {
    config: SemioscanConfig,
}

impl Default for SemioscanConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SemioscanConfigBuilder {
    /// Create a new builder with minimal defaults
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .max_block_range(1000)
    ///     .build();
    /// ```
    pub fn new() -> Self {
        Self {
            config: SemioscanConfig::minimal(),
        }
    }

    /// Start with common defaults
    ///
    /// Initializes the builder with the same defaults as [`SemioscanConfig::with_common_defaults`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::with_defaults()
    ///     // Base and Sonic already have 250ms delay from defaults
    ///     .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
    ///     .build();
    /// ```
    pub fn with_defaults() -> Self {
        Self {
            config: SemioscanConfig::with_common_defaults(),
        }
    }

    /// Set global max block range
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .max_block_range(1000)  // Query up to 1000 blocks at once
    ///     .build();
    /// ```
    pub fn max_block_range(mut self, max: u64) -> Self {
        self.config.max_block_range = MaxBlockRange::new(max);
        self
    }

    /// Set global rate limit delay
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .rate_limit_delay(Duration::from_millis(500))
    ///     .build();
    /// ```
    pub fn rate_limit_delay(mut self, delay: Duration) -> Self {
        self.config.rate_limit_delay = Some(delay);
        self
    }

    /// Set global RPC timeout
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .rpc_timeout(Duration::from_secs(60))  // 60 second timeout
    ///     .build();
    /// ```
    pub fn rpc_timeout(mut self, timeout: Duration) -> Self {
        self.config.rpc_timeout = timeout;
        self
    }

    /// Add chain-specific configuration
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{SemioscanConfigBuilder, ChainConfig, MaxBlockRange};
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .chain_config(
    ///         NamedChain::Polygon,
    ///         ChainConfig {
    ///             max_block_range: Some(MaxBlockRange::new(2000)),
    ///             rate_limit_delay: Some(Duration::from_millis(500)),
    ///             rpc_timeout: None,
    ///         },
    ///     )
    ///     .build();
    /// ```
    pub fn chain_config(mut self, chain: NamedChain, config: ChainConfig) -> Self {
        self.config.set_chain_override(chain, config);
        self
    }

    /// Convenience: set rate limit delay for a specific chain
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::with_defaults()
    ///     .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
    ///     .build();
    /// ```
    pub fn chain_rate_limit(mut self, chain: NamedChain, delay: Duration) -> Self {
        let existing = self.config.chain_overrides.remove(&chain);
        let chain_config = ChainConfig {
            max_block_range: existing.as_ref().and_then(|c| c.max_block_range),
            rate_limit_delay: Some(delay),
            rpc_timeout: existing.and_then(|c| c.rpc_timeout),
        };
        self.config.set_chain_override(chain, chain_config);
        self
    }

    /// Convenience: set max block range for a specific chain
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use alloy_chains::NamedChain;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .chain_max_blocks(NamedChain::Polygon, 1000)
    ///     .build();
    /// ```
    pub fn chain_max_blocks(mut self, chain: NamedChain, max: u64) -> Self {
        let existing = self.config.chain_overrides.remove(&chain);
        let chain_config = ChainConfig {
            max_block_range: Some(MaxBlockRange::new(max)),
            rate_limit_delay: existing.as_ref().and_then(|c| c.rate_limit_delay),
            rpc_timeout: existing.and_then(|c| c.rpc_timeout),
        };
        self.config.set_chain_override(chain, chain_config);
        self
    }

    /// Convenience: set RPC timeout for a specific chain
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    /// use alloy_chains::NamedChain;
    /// use std::time::Duration;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .chain_timeout(NamedChain::Polygon, Duration::from_secs(60))
    ///     .build();
    /// ```
    pub fn chain_timeout(mut self, chain: NamedChain, timeout: Duration) -> Self {
        let existing = self.config.chain_overrides.remove(&chain);
        let chain_config = ChainConfig {
            max_block_range: existing.as_ref().and_then(|c| c.max_block_range),
            rate_limit_delay: existing.as_ref().and_then(|c| c.rate_limit_delay),
            rpc_timeout: Some(timeout),
        };
        self.config.set_chain_override(chain, chain_config);
        self
    }

    /// Build the final configuration
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::SemioscanConfigBuilder;
    ///
    /// let config = SemioscanConfigBuilder::new()
    ///     .max_block_range(1000)
    ///     .build();
    /// ```
    pub fn build(self) -> SemioscanConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SemioscanConfig::default();

        // Base has custom delay
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Base),
            Some(Duration::from_millis(250))
        );

        // Sonic has custom delay
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Sonic),
            Some(Duration::from_millis(250))
        );

        // Arbitrum uses default (no delay)
        assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);

        // All chains use default max block range
        assert_eq!(
            config.get_max_block_range(NamedChain::Base),
            MaxBlockRange::new(500)
        );
        assert_eq!(
            config.get_max_block_range(NamedChain::Arbitrum),
            MaxBlockRange::new(500)
        );
    }

    #[test]
    fn test_minimal_config() {
        let config = SemioscanConfig::minimal();

        // No chain-specific delays
        assert_eq!(config.get_rate_limit_delay(NamedChain::Base), None);
        assert_eq!(config.get_rate_limit_delay(NamedChain::Sonic), None);

        // Default max block range
        assert_eq!(
            config.get_max_block_range(NamedChain::Base),
            MaxBlockRange::new(500)
        );
    }

    #[test]
    fn test_builder_pattern() {
        let config = SemioscanConfigBuilder::new()
            .max_block_range(1000)
            .chain_rate_limit(NamedChain::Polygon, Duration::from_secs(1))
            .build();

        assert_eq!(
            config.get_max_block_range(NamedChain::Polygon),
            MaxBlockRange::new(1000)
        );
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Polygon),
            Some(Duration::from_secs(1))
        );
    }

    #[test]
    fn test_chain_override() {
        let mut config = SemioscanConfig::minimal();

        config.set_chain_override(
            NamedChain::Arbitrum,
            ChainConfig {
                max_block_range: Some(MaxBlockRange::new(2000)),
                rate_limit_delay: Some(Duration::from_millis(100)),
                rpc_timeout: None, // Use default timeout
            },
        );

        assert_eq!(
            config.get_max_block_range(NamedChain::Arbitrum),
            MaxBlockRange::new(2000)
        );
        assert_eq!(
            config.get_max_block_range(NamedChain::Base),
            MaxBlockRange::new(500)
        ); // Default
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Arbitrum),
            Some(Duration::from_millis(100))
        );
        assert_eq!(
            config.get_rpc_timeout(NamedChain::Arbitrum),
            Duration::from_secs(30)
        ); // Uses default
    }

    #[test]
    fn test_builder_with_defaults() {
        let config = SemioscanConfigBuilder::with_defaults()
            .chain_max_blocks(NamedChain::Polygon, 1000)
            .build();

        // Base and Sonic should still have default delays
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Base),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Sonic),
            Some(Duration::from_millis(250))
        );

        // Polygon should have custom max blocks
        assert_eq!(
            config.get_max_block_range(NamedChain::Polygon),
            MaxBlockRange::new(1000)
        );
    }

    #[test]
    fn test_chain_config_preserves_existing() {
        let config = SemioscanConfigBuilder::new()
            .chain_max_blocks(NamedChain::Arbitrum, 1000)
            .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
            .build();

        // Both settings should be present
        assert_eq!(
            config.get_max_block_range(NamedChain::Arbitrum),
            MaxBlockRange::new(1000)
        );
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Arbitrum),
            Some(Duration::from_millis(100))
        );
    }

    #[test]
    fn test_global_rate_limit() {
        let config = SemioscanConfigBuilder::new()
            .rate_limit_delay(Duration::from_millis(500))
            .build();

        // All chains should use global delay
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Arbitrum),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Base),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn test_chain_override_global_rate_limit() {
        let config = SemioscanConfigBuilder::new()
            .rate_limit_delay(Duration::from_millis(500))
            .chain_rate_limit(NamedChain::Base, Duration::from_millis(250))
            .build();

        // Base should use chain-specific delay
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Base),
            Some(Duration::from_millis(250))
        );

        // Other chains use global delay
        assert_eq!(
            config.get_rate_limit_delay(NamedChain::Arbitrum),
            Some(Duration::from_millis(500))
        );
    }
}
