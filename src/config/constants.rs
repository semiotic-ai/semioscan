//! Well-known addresses and constants
//!
//! This module centralizes magic constants and well-known blockchain addresses
//! used throughout the semioscan crate, improving discoverability and maintainability.

use alloy_primitives::{address, Address};

/// Well-known stablecoin addresses
pub mod stablecoins {
    use super::*;

    /// Binance-Peg USDC on BSC (BNB Smart Chain)
    ///
    /// This is a bridged version of USDC issued by Binance on BSC.
    /// Note: This is different from native USDC on other chains.
    ///
    /// Contract: 0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d
    pub const BSC_BINANCE_PEG_USDC: Address = address!("8ac76a51cc950d9822d68b83fe1ad97b32cd580d");

    /// Native USDC on Ethereum Mainnet
    ///
    /// Contract: 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
    pub const ETH_USDC: Address = address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");

    /// USDT on Ethereum Mainnet
    ///
    /// Contract: 0xdAC17F958D2ee523a2206206994597C13D831ec7
    pub const ETH_USDT: Address = address!("dac17f958d2ee523a2206206994597c13d831ec7");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bsc_binance_peg_usdc() {
        // Verify the address is correct
        assert_eq!(
            stablecoins::BSC_BINANCE_PEG_USDC,
            address!("8ac76a51cc950d9822d68b83fe1ad97b32cd580d")
        );
    }

    #[test]
    fn test_eth_usdc() {
        assert_eq!(
            stablecoins::ETH_USDC,
            address!("a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
        );
    }

    #[test]
    fn test_eth_usdt() {
        assert_eq!(
            stablecoins::ETH_USDT,
            address!("dac17f958d2ee523a2206206994597c13d831ec7")
        );
    }
}
