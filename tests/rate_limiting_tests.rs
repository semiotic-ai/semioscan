// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for rate limiting functionality
//!
//! These tests validate that rate limiting configuration is correctly applied
//! across the public API of semioscan.

use alloy_chains::NamedChain;
use semioscan::{SemioscanConfig, SemioscanConfigBuilder};
use std::time::Duration;

/// Test that default configuration includes rate limiting for known strict chains
#[test]
fn test_default_config_has_strict_chain_limits() {
    let config = SemioscanConfig::default();

    // Base: Known to have strict Alchemy rate limits
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(Duration::from_millis(250)),
        "Base should have 250ms delay by default"
    );

    // Sonic: Known to have strict rate limits
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Sonic),
        Some(Duration::from_millis(250)),
        "Sonic should have 250ms delay by default"
    );
}

/// Test that default configuration does not rate limit permissive chains
#[test]
fn test_default_config_no_limits_for_permissive_chains() {
    let config = SemioscanConfig::default();

    // These chains should not have rate limiting by default
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        None,
        "Arbitrum should have no delay by default"
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Optimism),
        None,
        "Optimism should have no delay by default"
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Polygon),
        None,
        "Polygon should have no delay by default"
    );
}

/// Test that minimal configuration (for premium RPC) has no rate limits
#[test]
fn test_minimal_config_no_limits() {
    let config = SemioscanConfig::minimal();

    // No chain should have rate limiting with minimal config
    assert_eq!(config.get_rate_limit_delay(NamedChain::Base), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Sonic), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Mainnet), None);
}

/// Test that global rate limiting applies to all chains
#[test]
fn test_global_rate_limit_applies_to_all_chains() {
    let global_delay = Duration::from_millis(500);
    let config = SemioscanConfigBuilder::new()
        .rate_limit_delay(global_delay)
        .build();

    // All chains should inherit global delay
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(global_delay)
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(global_delay)
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Polygon),
        Some(global_delay)
    );
}

/// Test that chain-specific overrides take precedence over global settings
#[test]
fn test_chain_override_precedence() {
    let global_delay = Duration::from_millis(500);
    let arbitrum_delay = Duration::from_millis(100);

    let config = SemioscanConfigBuilder::new()
        .rate_limit_delay(global_delay)
        .chain_rate_limit(NamedChain::Arbitrum, arbitrum_delay)
        .build();

    // Arbitrum should use chain-specific delay
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(arbitrum_delay),
        "Chain-specific delay should override global"
    );

    // Other chains should use global delay
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(global_delay),
        "Non-overridden chains should use global delay"
    );
}

/// Test that starting with defaults preserves built-in chain-specific limits
#[test]
fn test_with_defaults_preserves_chain_limits() {
    let config = SemioscanConfigBuilder::with_defaults()
        .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
        .build();

    // Built-in defaults should be preserved
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(Duration::from_millis(250)),
        "Base default should be preserved"
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Sonic),
        Some(Duration::from_millis(250)),
        "Sonic default should be preserved"
    );

    // New override should be applied
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(Duration::from_millis(100)),
        "New chain override should be applied"
    );
}

/// Test that multiple chain-specific overrides can coexist
#[test]
fn test_multiple_chain_overrides() {
    let config = SemioscanConfigBuilder::new()
        .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Polygon, Duration::from_millis(200))
        .chain_rate_limit(NamedChain::Mainnet, Duration::from_millis(300))
        .build();

    // Each chain should have its specific delay
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(Duration::from_millis(100))
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Polygon),
        Some(Duration::from_millis(200))
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Mainnet),
        Some(Duration::from_millis(300))
    );

    // Unconfigured chains should have no delay
    assert_eq!(config.get_rate_limit_delay(NamedChain::Optimism), None);
}

/// Test edge case: zero duration delay (technically valid, though unusual)
#[test]
fn test_zero_duration_delay() {
    let config = SemioscanConfigBuilder::new()
        .rate_limit_delay(Duration::from_millis(0))
        .build();

    // Zero delay is technically Some (not None), so it overrides chain defaults
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(Duration::from_millis(0))
    );
}

/// Test that very large delays are supported (e.g., for testing or extreme rate limiting)
#[test]
fn test_large_delay_values() {
    let large_delay = Duration::from_secs(60); // 1 minute delay (extreme)
    let config = SemioscanConfigBuilder::new()
        .rate_limit_delay(large_delay)
        .build();

    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(large_delay)
    );
}

/// Test that rate limiting configuration is independent across instances
#[test]
fn test_config_independence() {
    let config1 = SemioscanConfigBuilder::new()
        .rate_limit_delay(Duration::from_millis(100))
        .build();

    let config2 = SemioscanConfigBuilder::new()
        .rate_limit_delay(Duration::from_millis(500))
        .build();

    // Each config should maintain its own settings
    assert_eq!(
        config1.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(Duration::from_millis(100))
    );
    assert_eq!(
        config2.get_rate_limit_delay(NamedChain::Arbitrum),
        Some(Duration::from_millis(500))
    );
}

/// Test rate limiting configuration for all supported major chains
#[test]
fn test_rate_limiting_for_major_chains() {
    let config = SemioscanConfigBuilder::new()
        .chain_rate_limit(NamedChain::Mainnet, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Base, Duration::from_millis(250))
        .chain_rate_limit(NamedChain::Optimism, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Polygon, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Avalanche, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::BinanceSmartChain, Duration::from_millis(100))
        .build();

    // Verify all chains are configured
    assert!(config.get_rate_limit_delay(NamedChain::Mainnet).is_some());
    assert!(config.get_rate_limit_delay(NamedChain::Arbitrum).is_some());
    assert!(config.get_rate_limit_delay(NamedChain::Base).is_some());
    assert!(config.get_rate_limit_delay(NamedChain::Optimism).is_some());
    assert!(config.get_rate_limit_delay(NamedChain::Polygon).is_some());
    assert!(config.get_rate_limit_delay(NamedChain::Avalanche).is_some());
    assert!(config
        .get_rate_limit_delay(NamedChain::BinanceSmartChain)
        .is_some());
}

/// Test that cloning config preserves rate limiting settings
#[test]
fn test_config_clone_preserves_rate_limits() {
    let original = SemioscanConfigBuilder::new()
        .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
        .build();

    let cloned = original.clone();

    // Cloned config should have same rate limits
    assert_eq!(
        original.get_rate_limit_delay(NamedChain::Arbitrum),
        cloned.get_rate_limit_delay(NamedChain::Arbitrum)
    );
}

/// Test realistic production scenarios
#[test]
fn test_production_alchemy_config() {
    // Typical config for Alchemy free tier
    let config = SemioscanConfigBuilder::with_defaults()
        .chain_rate_limit(NamedChain::Mainnet, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Arbitrum, Duration::from_millis(100))
        .chain_rate_limit(NamedChain::Optimism, Duration::from_millis(100))
        .build();

    // Base and Sonic should have built-in defaults (250ms)
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(Duration::from_millis(250))
    );
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Sonic),
        Some(Duration::from_millis(250))
    );

    // Custom overrides should be applied
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Mainnet),
        Some(Duration::from_millis(100))
    );
}

#[test]
fn test_production_premium_rpc_config() {
    // Premium RPC provider (Infura paid, QuickNode, etc.)
    let config = SemioscanConfig::minimal();

    // No rate limiting for any chain
    assert_eq!(config.get_rate_limit_delay(NamedChain::Base), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Sonic), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Mainnet), None);
    assert_eq!(config.get_rate_limit_delay(NamedChain::Arbitrum), None);
}

/// Test that overriding a default chain limit works correctly
#[test]
fn test_override_default_chain_limit() {
    // Start with defaults (Base has 250ms)
    let config = SemioscanConfigBuilder::with_defaults()
        .chain_rate_limit(NamedChain::Base, Duration::from_millis(500)) // Override to 500ms
        .build();

    // Base should use overridden value
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Base),
        Some(Duration::from_millis(500)),
        "Should use overridden delay for Base"
    );

    // Sonic should still have default
    assert_eq!(
        config.get_rate_limit_delay(NamedChain::Sonic),
        Some(Duration::from_millis(250)),
        "Sonic default should be unchanged"
    );
}

/// Test that removing a chain-specific limit by setting global to None works
#[test]
fn test_clearing_chain_limits_with_minimal() {
    // Start with defaults (has chain-specific limits)
    let _ = SemioscanConfig::with_common_defaults();

    // Create minimal config (no limits)
    let minimal = SemioscanConfig::minimal();

    // All limits should be cleared
    assert_eq!(minimal.get_rate_limit_delay(NamedChain::Base), None);
    assert_eq!(minimal.get_rate_limit_delay(NamedChain::Sonic), None);
}
