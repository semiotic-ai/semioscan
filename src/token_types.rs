//! Strong types for token-related values
//!
//! This module provides newtype wrappers for token operations
//! to add type safety and prevent mixing incompatible units.
//!
//! # Type Relationships
//!
//! ```text
//! TokenAmount (U256, raw)
//!     |
//!     | normalize(TokenDecimals)
//!     ↓
//! NormalizedAmount (f64, human-readable)
//!     |
//!     | × price_per_token
//!     ↓
//! USD value (f64)
//! ```

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::ops::Add;

/// Raw token amount (not normalized for decimals)
///
/// This represents the raw token amount as stored on-chain in the smallest
/// unit (e.g., wei for ETH, satoshis for WBTC). To convert to human-readable
/// amounts, use [`normalize`](Self::normalize) with the token's [`TokenDecimals`].
///
/// # Examples
///
/// ```
/// use alloy_primitives::U256;
/// use semioscan::{TokenAmount, TokenDecimals};
///
/// // 1.5 ETH in wei (18 decimals)
/// let amount = TokenAmount::new(U256::from(1_500_000_000_000_000_000u64));
/// let normalized = amount.normalize(TokenDecimals::STANDARD);
/// assert!((normalized.as_f64() - 1.5).abs() < 0.0001);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenAmount(U256);

impl TokenAmount {
    /// Zero token amount
    pub const ZERO: Self = Self(U256::ZERO);

    /// Create a new token amount from U256
    pub const fn new(amount: U256) -> Self {
        Self(amount)
    }

    /// Get the inner U256 value
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Normalize by token decimals: amount / 10^decimals
    ///
    /// Converts raw token amount to human-readable decimal form.
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::{TokenAmount, TokenDecimals};
    ///
    /// // 100 USDC (6 decimals)
    /// let raw = TokenAmount::new(U256::from(100_000_000u64));
    /// let normalized = raw.normalize(TokenDecimals::USDC);
    /// assert_eq!(normalized.as_f64(), 100.0);
    /// ```
    pub fn normalize(&self, decimals: TokenDecimals) -> NormalizedAmount {
        // Convert U256 to f64 via string to handle large numbers
        let amount_str = self.0.to_string();
        let amount_f64 = amount_str.parse::<f64>().unwrap_or_else(|e| {
            tracing::warn!(
                amount = %self.0,
                error = %e,
                "Failed to parse token amount to f64, using 0.0"
            );
            0.0
        });

        // Calculate divisor: 10^decimals
        let divisor = 10_f64.powi(decimals.as_u8() as i32);

        NormalizedAmount::new(amount_f64 / divisor)
    }
}

impl From<u64> for TokenAmount {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl From<U256> for TokenAmount {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl Add for TokenAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::fmt::Display for TokenAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// ERC-20 token decimal precision
///
/// Represents the number of decimal places for a token. Most ERC-20 tokens
/// use 18 decimals (like ETH), but some use different values:
/// - USDC: 6 decimals
/// - WBTC: 8 decimals
/// - Standard: 18 decimals
///
/// Note: This is distinct from the `DecimalPrecision` enum in combined_retriever.rs,
/// which provides chain-specific decimal rules for specific tokens. This type is
/// a general-purpose wrapper for any token's decimal count.
///
/// # Examples
///
/// ```
/// use semioscan::TokenDecimals;
///
/// let eth_decimals = TokenDecimals::STANDARD;
/// assert_eq!(eth_decimals.as_u8(), 18);
///
/// let usdc_decimals = TokenDecimals::USDC;
/// assert_eq!(usdc_decimals.as_u8(), 6);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenDecimals(u8);

impl TokenDecimals {
    /// Maximum reasonable decimals (following ERC-20 convention)
    pub const MAX_REASONABLE: u8 = 18;

    /// Standard decimals for ETH-like tokens (18)
    pub const STANDARD: Self = Self(18);

    /// USDC decimals (6)
    pub const USDC: Self = Self(6);

    /// WBTC decimals (8)
    pub const WBTC: Self = Self(8);

    /// DAI decimals (18)
    pub const DAI: Self = Self(18);

    /// Create a new decimal precision value
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::TokenDecimals;
    ///
    /// let decimals = TokenDecimals::new(18);
    /// assert!(decimals.is_reasonable());
    /// ```
    pub const fn new(decimals: u8) -> Self {
        Self(decimals)
    }

    /// Get the inner u8 value
    pub const fn as_u8(&self) -> u8 {
        self.0
    }

    /// Check if decimals are in reasonable range (0-18)
    ///
    /// While the ERC-20 standard allows any u8 value, most tokens
    /// use 18 or fewer decimals. Values over 18 are unusual and
    /// may indicate data errors.
    pub const fn is_reasonable(&self) -> bool {
        self.0 <= Self::MAX_REASONABLE
    }

    /// Calculate the divisor for normalization: 10^decimals
    ///
    /// This is useful for manual normalization calculations.
    pub fn divisor(&self) -> f64 {
        10_f64.powi(self.0 as i32)
    }
}

impl From<u8> for TokenDecimals {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for TokenDecimals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} decimals", self.0)
    }
}

/// Token amount normalized by decimals (human-readable)
///
/// This represents a token amount after dividing by 10^decimals.
/// For example, 1.5 ETH (not 1.5e18 wei), or 100.25 USDC (not 100250000).
///
/// This type is used for display, calculations with USD prices, and
/// any operations requiring decimal arithmetic.
///
/// # Examples
///
/// ```
/// use semioscan::{NormalizedAmount, UsdValue};
///
/// let amount = NormalizedAmount::new(1.5);
/// let usd_value = amount.to_usd(2000.0); // 1.5 ETH × $2000/ETH
/// assert_eq!(usd_value, UsdValue::new(3000.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NormalizedAmount(f64);

impl NormalizedAmount {
    /// Zero normalized amount
    pub const ZERO: Self = Self(0.0);

    /// Create a new normalized amount
    pub const fn new(amount: f64) -> Self {
        Self(amount)
    }

    /// Get the inner f64 value
    pub const fn as_f64(&self) -> f64 {
        self.0
    }

    /// Calculate value in USD given price per token
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::{NormalizedAmount, UsdValue};
    ///
    /// let amount = NormalizedAmount::new(2.5);
    /// let price_per_token = 1800.0; // $1800 per token
    /// let usd_value = amount.to_usd(price_per_token);
    /// assert_eq!(usd_value, UsdValue::new(4500.0)); // 2.5 × $1800
    /// ```
    pub fn to_usd(&self, price_per_token: f64) -> UsdValue {
        UsdValue::new(self.0 * price_per_token)
    }

    /// Check if amount is effectively zero (within epsilon)
    pub fn is_zero(&self) -> bool {
        self.0.abs() < f64::EPSILON
    }

    /// Check if amount is negative
    pub fn is_negative(&self) -> bool {
        self.0 < 0.0
    }

    /// Get absolute value
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }
}

impl From<f64> for NormalizedAmount {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl Add for NormalizedAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for NormalizedAmount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Mul<f64> for NormalizedAmount {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self(self.0 * rhs)
    }
}

impl std::ops::Div<f64> for NormalizedAmount {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl std::fmt::Display for NormalizedAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.6}", self.0) // 6 decimal places for display
    }
}

/// Represents a USD-denominated value
///
/// This type provides type safety for financial calculations involving USD values,
/// preventing confusion with other f64 values like percentages or raw token amounts.
///
/// # Examples
///
/// ```
/// use semioscan::UsdValue;
///
/// let price = UsdValue::new(1800.50);
/// let formatted = price.format(2);
/// assert_eq!(formatted, "$1800.50");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsdValue(f64);

impl UsdValue {
    /// Zero USD value
    pub const ZERO: Self = Self(0.0);

    /// Create a new USD value
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// let value = UsdValue::new(100.50);
    /// assert_eq!(value.as_f64(), 100.50);
    /// ```
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    /// Get the inner f64 value
    pub const fn as_f64(&self) -> f64 {
        self.0
    }

    /// Check if the value is zero
    pub fn is_zero(&self) -> bool {
        self.0.abs() < f64::EPSILON
    }

    /// Format as USD string with specified precision
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// let value = UsdValue::new(1234.567);
    /// assert_eq!(value.format(2), "$1234.57");
    /// assert_eq!(value.format(0), "$1235");
    /// ```
    pub fn format(&self, precision: usize) -> String {
        format!("${:.precision$}", self.0, precision = precision)
    }

    /// Get absolute value
    pub fn abs(&self) -> Self {
        Self(self.0.abs())
    }
}

impl From<f64> for UsdValue {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl Add for UsdValue {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for UsdValue {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::fmt::Display for UsdValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${:.2}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_amount_creation() {
        let amount = TokenAmount::new(U256::from(1000u64));
        assert_eq!(amount.as_u256(), U256::from(1000u64));
    }

    #[test]
    fn test_token_amount_normalization_eth() {
        // 1.5 ETH in wei (18 decimals)
        let raw = TokenAmount::new(U256::from(1_500_000_000_000_000_000u64));
        let normalized = raw.normalize(TokenDecimals::STANDARD);
        assert!((normalized.as_f64() - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_token_amount_normalization_usdc() {
        // 100 USDC in smallest units (6 decimals)
        let raw = TokenAmount::new(U256::from(100_000_000u64));
        let normalized = raw.normalize(TokenDecimals::USDC);
        assert_eq!(normalized.as_f64(), 100.0);
    }

    #[test]
    fn test_token_amount_normalization_wbtc() {
        // 0.5 WBTC (8 decimals)
        let raw = TokenAmount::new(U256::from(50_000_000u64));
        let normalized = raw.normalize(TokenDecimals::WBTC);
        assert_eq!(normalized.as_f64(), 0.5);
    }

    #[test]
    fn test_token_amount_addition() {
        let amt1 = TokenAmount::new(U256::from(1000u64));
        let amt2 = TokenAmount::new(U256::from(2000u64));
        let total = amt1 + amt2;
        assert_eq!(total.as_u256(), U256::from(3000u64));
    }

    #[test]
    fn test_token_amount_zero() {
        assert_eq!(TokenAmount::ZERO.as_u256(), U256::ZERO);
    }

    #[test]
    fn test_token_decimals_constants() {
        assert_eq!(TokenDecimals::STANDARD.as_u8(), 18);
        assert_eq!(TokenDecimals::USDC.as_u8(), 6);
        assert_eq!(TokenDecimals::WBTC.as_u8(), 8);
        assert_eq!(TokenDecimals::DAI.as_u8(), 18);
    }

    #[test]
    fn test_token_decimals_reasonable() {
        assert!(TokenDecimals::new(0).is_reasonable());
        assert!(TokenDecimals::new(18).is_reasonable());
        assert!(!TokenDecimals::new(19).is_reasonable());
        assert!(!TokenDecimals::new(255).is_reasonable());
    }

    #[test]
    fn test_token_decimals_divisor() {
        assert_eq!(TokenDecimals::USDC.divisor(), 1_000_000.0);
        assert_eq!(TokenDecimals::WBTC.divisor(), 100_000_000.0);
        assert_eq!(
            TokenDecimals::STANDARD.divisor(),
            1_000_000_000_000_000_000.0
        );
    }

    #[test]
    fn test_normalized_amount_creation() {
        let amount = NormalizedAmount::new(1.5);
        assert_eq!(amount.as_f64(), 1.5);
    }

    #[test]
    fn test_normalized_amount_arithmetic() {
        let amt1 = NormalizedAmount::new(1.5);
        let amt2 = NormalizedAmount::new(2.5);

        let sum = amt1 + amt2;
        assert_eq!(sum.as_f64(), 4.0);

        let diff = amt2 - amt1;
        assert_eq!(diff.as_f64(), 1.0);

        let product = amt1 * 2.0;
        assert_eq!(product.as_f64(), 3.0);

        let quotient = amt2 / 2.0;
        assert_eq!(quotient.as_f64(), 1.25);
    }

    #[test]
    fn test_normalized_amount_to_usd() {
        let amount = NormalizedAmount::new(2.5);
        let price = 1800.0; // $1800 per token
        let usd = amount.to_usd(price);
        assert_eq!(usd, UsdValue::new(4500.0));
    }

    #[test]
    fn test_normalized_amount_zero() {
        assert!(NormalizedAmount::ZERO.is_zero());
        assert!(NormalizedAmount::new(0.0).is_zero());
        assert!(!NormalizedAmount::new(0.1).is_zero());
    }

    #[test]
    fn test_normalized_amount_negative() {
        assert!(NormalizedAmount::new(-1.0).is_negative());
        assert!(!NormalizedAmount::new(1.0).is_negative());
        assert!(!NormalizedAmount::new(0.0).is_negative());
    }

    #[test]
    fn test_normalized_amount_abs() {
        let negative = NormalizedAmount::new(-5.5);
        let positive = negative.abs();
        assert_eq!(positive.as_f64(), 5.5);
    }

    #[test]
    fn test_display_formatting() {
        let amount = TokenAmount::new(U256::from(12345u64));
        assert_eq!(format!("{}", amount), "12345");

        let decimals = TokenDecimals::STANDARD;
        assert_eq!(format!("{}", decimals), "18 decimals");

        let normalized = NormalizedAmount::new(1.234567890);
        assert_eq!(format!("{}", normalized), "1.234568"); // 6 decimal places
    }

    #[test]
    fn test_serialization() {
        let amount = TokenAmount::new(U256::from(12345u64));
        let json = serde_json::to_string(&amount).unwrap();
        let deserialized: TokenAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(amount, deserialized);

        let decimals = TokenDecimals::STANDARD;
        let json = serde_json::to_string(&decimals).unwrap();
        let deserialized: TokenDecimals = serde_json::from_str(&json).unwrap();
        assert_eq!(decimals, deserialized);

        let normalized = NormalizedAmount::new(1.5);
        let json = serde_json::to_string(&normalized).unwrap();
        let deserialized: NormalizedAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(normalized, deserialized);
    }

    #[test]
    fn test_conversions() {
        // TokenAmount conversions
        let u256_val = U256::from(12345u64);
        let amount: TokenAmount = u256_val.into();
        let back: U256 = amount.as_u256();
        assert_eq!(u256_val, back);

        // TokenDecimals conversions
        let u8_val: u8 = 18;
        let decimals: TokenDecimals = u8_val.into();
        let back: u8 = decimals.as_u8();
        assert_eq!(u8_val, back);

        // NormalizedAmount conversions
        let f64_val = 1.5;
        let normalized: NormalizedAmount = f64_val.into();
        let back: f64 = normalized.as_f64();
        assert_eq!(f64_val, back);
    }

    #[test]
    fn test_full_workflow() {
        // Simulate receiving a token transfer and calculating USD value
        let raw_amount = TokenAmount::new(U256::from(250_000_000u64)); // 250 USDC
        let decimals = TokenDecimals::USDC;
        let price_per_usdc = 1.0; // $1 per USDC

        let normalized = raw_amount.normalize(decimals);
        assert_eq!(normalized.as_f64(), 250.0);

        let usd_value = normalized.to_usd(price_per_usdc);
        assert_eq!(usd_value, UsdValue::new(250.0));
    }
}
