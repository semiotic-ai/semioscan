//! Utility functions for formatting and conversion

use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use bigdecimal::BigDecimal;
use std::str::FromStr;

use crate::config::constants::stablecoins::BSC_BINANCE_PEG_USDC;
use crate::errors::RetrievalError;

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
/// A Result containing the BigDecimal representing the human-readable value, or a RetrievalError
/// if the conversion fails.
///
/// # Errors
/// Returns `RetrievalError::ConversionFailed` if the U256 value cannot be converted to BigDecimal.
/// This typically indicates invalid data that should not be silently masked.
///
/// # Example
/// ```ignore
/// use semioscan::u256_to_bigdecimal;
/// use semioscan::DecimalPrecision;
/// use alloy_primitives::U256;
///
/// let wei = U256::from(1_000_000_000_000_000_000u128); // 1 ETH in wei
/// let eth = u256_to_bigdecimal(wei, DecimalPrecision::NativeToken)?; // Returns Ok(BigDecimal "1.0")
/// ```
pub fn u256_to_bigdecimal(
    value: U256,
    precision: DecimalPrecision,
) -> Result<BigDecimal, RetrievalError> {
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

    // Convert to BigDecimal with proper error handling
    let whole_decimal = BigDecimal::from_str(&whole.to_string())
        .map_err(|_| RetrievalError::bigdecimal_conversion_failed(whole))?;

    let fractional_decimal = BigDecimal::from_str(&fractional.to_string())
        .map_err(|_| RetrievalError::bigdecimal_conversion_failed(fractional))?;

    let divisor_decimal = BigDecimal::from_str(&divisor.to_string())
        .map_err(|_| RetrievalError::bigdecimal_conversion_failed(divisor))?;

    Ok(whole_decimal + (fractional_decimal / divisor_decimal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use alloy_primitives::{address, U256};

    // ========== format_wei_to_eth tests ==========

    #[test]
    fn format_wei_to_eth_with_zero() {
        let wei = U256::ZERO;
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "0");
    }

    #[test]
    fn format_wei_to_eth_with_one_eth() {
        let wei = U256::from(1_000_000_000_000_000_000u128); // 1 ETH in wei
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "1");
    }

    #[test]
    fn format_wei_to_eth_with_fractional_value() {
        let wei = U256::from(1_500_000_000_000_000_000u128); // 1.5 ETH
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "1.5");
    }

    #[test]
    fn format_wei_to_eth_removes_trailing_zeros() {
        let wei = U256::from(1_200_000_000_000_000_000u128); // 1.2 ETH
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "1.2"); // Not "1.200000000000000000"
    }

    #[test]
    fn format_wei_to_eth_with_small_fractional() {
        let wei = U256::from(123_456_789_012_345_678u128); // 0.123456789012345678 ETH
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "0.123456789012345678");
    }

    #[test]
    fn format_wei_to_eth_with_large_value() {
        let wei = U256::from(100_000_000_000_000_000_000u128); // 100 ETH
        let result = format_wei_to_eth(wei);
        assert_eq!(result, "100");
    }

    // ========== format_wei_to_gwei tests ==========

    #[test]
    fn format_wei_to_gwei_with_zero() {
        let wei = U256::ZERO;
        let result = format_wei_to_gwei(wei);
        assert_eq!(result, "0");
    }

    #[test]
    fn format_wei_to_gwei_with_whole_gwei() {
        let wei = U256::from(30_000_000_000u64); // 30 Gwei
        let result = format_wei_to_gwei(wei);
        assert_eq!(result, "30");
    }

    #[test]
    fn format_wei_to_gwei_with_fractional() {
        let wei = U256::from(30_500_000_000u64); // 30.5 Gwei
        let result = format_wei_to_gwei(wei);
        assert_eq!(result, "30.5");
    }

    #[test]
    fn format_wei_to_gwei_removes_trailing_zeros() {
        let wei = U256::from(20_100_000_000u64); // 20.1 Gwei
        let result = format_wei_to_gwei(wei);
        assert_eq!(result, "20.1"); // Not "20.100000000"
    }

    #[test]
    fn format_wei_to_gwei_with_small_fractional() {
        let wei = U256::from(1_234_567_890u64); // 1.23456789 Gwei
        let result = format_wei_to_gwei(wei);
        assert_eq!(result, "1.23456789");
    }

    // ========== format_token_amount tests ==========

    #[test]
    fn format_token_amount_with_zero_decimals() {
        let amount = U256::from(1000u64);
        let result = format_token_amount(amount, DecimalPrecision::Usdc);
        // Treating as 6 decimals: 1000 / 10^6 = 0.001
        let expected = "0.001";
        assert_eq!(result, expected);
    }

    #[test]
    fn format_token_amount_with_six_decimals_usdc() {
        let amount = U256::from(1_000_000u64); // 1 USDC (6 decimals)
        let result = format_token_amount(amount, DecimalPrecision::Usdc);
        assert_eq!(result, "1");
    }

    #[test]
    fn format_token_amount_with_eighteen_decimals() {
        let amount = U256::from(1_000_000_000_000_000_000u128); // 1 token (18 decimals)
        let result = format_token_amount(amount, DecimalPrecision::NativeToken);
        assert_eq!(result, "1");
    }

    #[test]
    fn format_token_amount_removes_trailing_zeros() {
        let amount = U256::from(1_200_000u64); // 1.2 USDC (6 decimals)
        let result = format_token_amount(amount, DecimalPrecision::Usdc);
        assert_eq!(result, "1.2"); // Not "1.200000"
    }

    #[test]
    fn format_token_amount_with_bsc_binance_peg_usdc() {
        let amount = U256::from(1_500_000_000_000_000_000u128); // 1.5 tokens (18 decimals)
        let result = format_token_amount(amount, DecimalPrecision::BinancePegUsdc);
        assert_eq!(result, "1.5");
    }

    #[test]
    fn format_token_amount_with_fractional_usdc() {
        let amount = U256::from(123_456u64); // 0.123456 USDC
        let result = format_token_amount(amount, DecimalPrecision::Usdc);
        assert_eq!(result, "0.123456");
    }

    // ========== get_token_decimal_precision tests ==========

    #[test]
    fn get_token_decimal_precision_for_native_token() {
        let precision = get_token_decimal_precision(NamedChain::Arbitrum, Address::ZERO);
        assert_eq!(precision, DecimalPrecision::NativeToken);
    }

    #[test]
    fn get_token_decimal_precision_for_bsc_binance_peg_usdc() {
        let bsc_binance_peg_usdc = address!("8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d");
        let precision =
            get_token_decimal_precision(NamedChain::BinanceSmartChain, bsc_binance_peg_usdc);
        assert_eq!(precision, DecimalPrecision::BinancePegUsdc);
    }

    #[test]
    fn get_token_decimal_precision_for_standard_usdc_on_arbitrum() {
        let arbitrum_usdc = address!("af88d065e77c8cC2239327C5EDb3A432268e5831");
        let precision = get_token_decimal_precision(NamedChain::Arbitrum, arbitrum_usdc);
        assert_eq!(precision, DecimalPrecision::Usdc);
    }

    #[test]
    fn get_token_decimal_precision_for_standard_usdc_on_base() {
        let base_usdc = address!("833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
        let precision = get_token_decimal_precision(NamedChain::Base, base_usdc);
        assert_eq!(precision, DecimalPrecision::Usdc);
    }

    #[test]
    fn get_token_decimal_precision_for_non_usdc_on_bsc() {
        // Random token address on BSC (not Binance-Peg USDC)
        let other_token = address!("1111111111111111111111111111111111111111");
        let precision = get_token_decimal_precision(NamedChain::BinanceSmartChain, other_token);
        assert_eq!(precision, DecimalPrecision::Usdc); // Defaults to USDC precision
    }

    // ========== u256_to_bigdecimal tests ==========

    #[test]
    fn u256_to_bigdecimal_with_usdc_precision() {
        let value = U256::from(1_000_000u64); // 1 USDC
        let result = u256_to_bigdecimal(value, DecimalPrecision::Usdc).unwrap();
        let expected = BigDecimal::from_str("1.0").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_with_native_token_precision() {
        let value = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let result = u256_to_bigdecimal(value, DecimalPrecision::NativeToken).unwrap();
        let expected = BigDecimal::from_str("1.0").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_with_bsc_binance_peg_usdc_precision() {
        let value = U256::from(1_500_000_000_000_000_000u128); // 1.5 tokens (18 decimals)
        let result = u256_to_bigdecimal(value, DecimalPrecision::BinancePegUsdc).unwrap();
        let expected = BigDecimal::from_str("1.5").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_with_fractional_usdc() {
        let value = U256::from(123_456u64); // 0.123456 USDC
        let result = u256_to_bigdecimal(value, DecimalPrecision::Usdc).unwrap();
        let expected = BigDecimal::from_str("0.123456").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_with_zero() {
        let value = U256::ZERO;
        let result = u256_to_bigdecimal(value, DecimalPrecision::Usdc).unwrap();
        let expected = BigDecimal::from_str("0").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_with_large_value() {
        let value = U256::from(1_000_000_000_000_000_000_000u128); // 1000 ETH
        let result = u256_to_bigdecimal(value, DecimalPrecision::NativeToken).unwrap();
        let expected = BigDecimal::from_str("1000.0").unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn u256_to_bigdecimal_preserves_precision() {
        // Test that we maintain decimal precision accurately
        let value = U256::from(123_456_789_012_345_678u128); // 0.123456789012345678 ETH
        let result = u256_to_bigdecimal(value, DecimalPrecision::NativeToken).unwrap();
        let expected = BigDecimal::from_str("0.123456789012345678").unwrap();
        assert_eq!(result, expected);
    }
}
