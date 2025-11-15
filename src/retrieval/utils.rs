//! Utility functions for formatting and conversion

use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use bigdecimal::BigDecimal;
use std::str::FromStr;

use crate::config::constants::stablecoins::BSC_BINANCE_PEG_USDC;

use super::decimal_precision::DecimalPrecision;

/// Convert wei (U256) to ETH string with 18 decimals
pub(crate) fn format_wei_to_eth(wei: U256) -> String {
    // ETH has 18 decimals
    let eth_divisor = U256::from(1_000_000_000_000_000_000u128); // 10^18
    let eth_whole = wei / eth_divisor;
    let eth_fractional = wei % eth_divisor;

    // Format with 18 decimal places, removing trailing zeros
    let fractional_str = format!("{:018}", eth_fractional);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", eth_whole)
    } else {
        // Always use decimal notation, never scientific notation
        format!("{}.{}", eth_whole, trimmed)
    }
}

/// Convert wei (U256) to Gwei string
pub(crate) fn format_wei_to_gwei(wei: U256) -> String {
    // Gwei has 9 decimals (10^9 wei = 1 Gwei)
    let gwei_divisor = U256::from(1_000_000_000u64); // 10^9
    let gwei_whole = wei / gwei_divisor;
    let gwei_fractional = wei % gwei_divisor;

    // Format with 9 decimal places, removing trailing zeros
    let fractional_str = format!("{:09}", gwei_fractional);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", gwei_whole)
    } else {
        format!("{}.{}", gwei_whole, trimmed)
    }
}

/// Convert token raw amount (U256) to string with specified decimal precision
/// Most chains use 6 decimals for USDC, but BNB Chain uses 18 decimals
pub(crate) fn format_token_amount(raw_amount: U256, precision: DecimalPrecision) -> String {
    let decimals = precision.decimals();
    if decimals == 0 {
        return raw_amount.to_string();
    }

    // Calculate divisor: 10^decimals
    let divisor = U256::from(10u64).pow(U256::from(decimals));
    let whole = raw_amount / divisor;
    let fractional = raw_amount % divisor;

    // Format with correct decimal places, removing trailing zeros
    let fractional_str = format!("{:0width$}", fractional, width = decimals as usize);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", whole)
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

/// Get the decimal precision for a specific token on a specific chain.
/// Native tokens (Address::ZERO) use 18 decimals.
/// Most USDC tokens use 6 decimals, but BSC Binance-Peg USDC uses 18 decimals.
///
/// # Arguments
/// * `chain` - The named chain
/// * `token_address` - The token contract address (Address::ZERO for native token)
///
/// # Returns
/// The appropriate DecimalPrecision for this token
pub fn get_token_decimal_precision(chain: NamedChain, token_address: Address) -> DecimalPrecision {
    // Native token (ETH, BNB, MATIC, etc.) uses 18 decimals
    if token_address == Address::ZERO {
        return DecimalPrecision::NativeToken;
    }

    // BSC Binance-Peg USDC has 18 decimals instead of 6
    if matches!(chain, NamedChain::BinanceSmartChain) && token_address == BSC_BINANCE_PEG_USDC {
        DecimalPrecision::BinancePegUsdc // 18 decimals
    } else {
        DecimalPrecision::Usdc // 6 decimals
    }
}

/// Convert U256 to BigDecimal with decimal scaling for database storage.
/// This function properly handles large decimal places (like 18 for ETH) without overflow.
///
/// # Arguments
/// * `value` - The raw U256 value (e.g., wei for ETH, smallest unit for tokens)
/// * `precision` - The decimal precision (Usdc = 6, BinancePegUsdc = 18, NativeToken = 18)
///
/// # Returns
/// A BigDecimal representing the human-readable value
///
/// # Example
/// ```ignore
/// let wei = U256::from(1_000_000_000_000_000_000u128); // 1 ETH in wei
/// let eth = u256_to_bigdecimal(wei, DecimalPrecision::NativeToken); // Returns BigDecimal "1.0"
/// ```
pub fn u256_to_bigdecimal(value: U256, precision: DecimalPrecision) -> BigDecimal {
    // Use U256 divisor to avoid i64 overflow for large exponents
    let divisor = match precision {
        DecimalPrecision::Usdc => U256::from(1_000_000u64), // 10^6
        DecimalPrecision::BinancePegUsdc | DecimalPrecision::NativeToken => {
            U256::from(1_000_000_000_000_000_000u128) // 10^18
        }
    };

    // Perform division in U256 space to get whole and fractional parts
    let whole = value / divisor;
    let fractional = value % divisor;

    // Convert to BigDecimal
    let whole_decimal =
        BigDecimal::from_str(&whole.to_string()).unwrap_or_else(|_| BigDecimal::from(0));
    let fractional_decimal =
        BigDecimal::from_str(&fractional.to_string()).unwrap_or_else(|_| BigDecimal::from(0));
    let divisor_decimal =
        BigDecimal::from_str(&divisor.to_string()).unwrap_or_else(|_| BigDecimal::from(1));

    whole_decimal + (fractional_decimal / divisor_decimal)
}
