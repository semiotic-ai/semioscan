// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Token price type (USDC per token)

use serde::{Deserialize, Serialize};

use super::normalized::NormalizedAmount;
use super::usd::UsdValue;

/// Price of one token in USDC (or other stablecoin)
///
/// This type represents the exchange rate between a token and USDC/USD,
/// providing type safety to distinguish prices from amounts or raw values.
///
/// # Examples
///
/// ```
/// use semioscan::{TokenPrice, NormalizedAmount, UsdValue};
///
/// // ETH trading at $2,000 per token
/// let eth_price = TokenPrice::new(2000.0);
///
/// // Calculate value of 1.5 ETH
/// let amount = NormalizedAmount::new(1.5);
/// let value = eth_price.value_of(amount);
/// assert_eq!(value, UsdValue::new(3000.0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenPrice(f64);

impl TokenPrice {
    /// Zero price (no value)
    pub const ZERO: Self = Self(0.0);

    /// Create a new token price
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::TokenPrice;
    ///
    /// let price = TokenPrice::new(1800.50);
    /// assert_eq!(price.as_f64(), 1800.50);
    /// ```
    pub const fn new(price_per_token: f64) -> Self {
        Self(price_per_token)
    }

    /// Get the inner f64 value
    pub const fn as_f64(&self) -> f64 {
        self.0
    }

    /// Check if price is effectively zero (within epsilon)
    pub fn is_zero(&self) -> bool {
        self.0.abs() < f64::EPSILON
    }

    /// Calculate USD value for a given amount of tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::{TokenPrice, NormalizedAmount, UsdValue};
    ///
    /// let price = TokenPrice::new(2000.0); // $2000 per token
    /// let amount = NormalizedAmount::new(2.5); // 2.5 tokens
    /// let value = price.value_of(amount);
    /// assert_eq!(value, UsdValue::new(5000.0)); // $5000 total
    /// ```
    pub fn value_of(&self, amount: NormalizedAmount) -> UsdValue {
        UsdValue::new(amount.as_f64() * self.0)
    }

    /// Calculate how many tokens can be bought with a given USD amount
    ///
    /// Returns None if price is zero to avoid division by zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::{TokenPrice, UsdValue, NormalizedAmount};
    ///
    /// let price = TokenPrice::new(2000.0); // $2000 per token
    /// let budget = UsdValue::new(5000.0); // $5000 to spend
    /// let amount = price.tokens_for(budget).unwrap();
    /// assert_eq!(amount, NormalizedAmount::new(2.5)); // Can buy 2.5 tokens
    /// ```
    pub fn tokens_for(&self, usd_value: UsdValue) -> Option<NormalizedAmount> {
        if self.is_zero() {
            None
        } else {
            Some(NormalizedAmount::new(usd_value.as_f64() / self.0))
        }
    }

    /// Format as price string with specified precision
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::TokenPrice;
    ///
    /// let price = TokenPrice::new(1234.567890);
    /// assert_eq!(price.format(2), "$1234.57");
    /// assert_eq!(price.format(6), "$1234.567890");
    /// ```
    pub fn format(&self, precision: usize) -> String {
        format!("${:.precision$}", self.0, precision = precision)
    }
}

impl From<f64> for TokenPrice {
    fn from(value: f64) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for TokenPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${:.6}", self.0) // 6 decimal places for crypto prices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_price_creation() {
        let price = TokenPrice::new(1800.50);
        assert_eq!(price.as_f64(), 1800.50);
    }

    #[test]
    fn test_token_price_zero() {
        assert!(TokenPrice::ZERO.is_zero());
        assert!(TokenPrice::new(0.0).is_zero());
        assert!(!TokenPrice::new(0.1).is_zero());
    }

    #[test]
    fn test_value_of() {
        let price = TokenPrice::new(2000.0); // $2000 per token
        let amount = NormalizedAmount::new(2.5); // 2.5 tokens
        let value = price.value_of(amount);
        assert_eq!(value, UsdValue::new(5000.0)); // $5000 total
    }

    #[test]
    fn test_value_of_fractional() {
        let price = TokenPrice::new(1800.75);
        let amount = NormalizedAmount::new(1.5);
        let value = price.value_of(amount);
        assert!((value.as_f64() - 2701.125).abs() < 0.001);
    }

    #[test]
    fn test_tokens_for() {
        let price = TokenPrice::new(2000.0); // $2000 per token
        let budget = UsdValue::new(5000.0); // $5000
        let amount = price.tokens_for(budget).unwrap();
        assert_eq!(amount, NormalizedAmount::new(2.5));
    }

    #[test]
    fn test_tokens_for_zero_price() {
        let price = TokenPrice::ZERO;
        let budget = UsdValue::new(1000.0);
        assert!(price.tokens_for(budget).is_none());
    }

    #[test]
    fn test_token_price_format() {
        let price = TokenPrice::new(1234.567890);
        assert_eq!(price.format(2), "$1234.57");
        assert_eq!(price.format(6), "$1234.567890");
        assert_eq!(price.format(0), "$1235");
    }

    #[test]
    fn test_display_formatting() {
        let price = TokenPrice::new(1234.567890);
        assert_eq!(format!("{}", price), "$1234.567890");
    }

    #[test]
    fn test_serialization() {
        let price = TokenPrice::new(1800.50);
        let json = serde_json::to_string(&price).unwrap();
        let deserialized: TokenPrice = serde_json::from_str(&json).unwrap();
        assert_eq!(price, deserialized);
    }

    #[test]
    fn test_conversions() {
        let f64_val = 1800.50;
        let price: TokenPrice = f64_val.into();
        let back: f64 = price.as_f64();
        assert_eq!(f64_val, back);
    }

    #[test]
    fn test_small_prices() {
        // Test with very small token prices (like some memecoins)
        let price = TokenPrice::new(0.00000123);
        let amount = NormalizedAmount::new(1_000_000.0); // 1 million tokens
        let value = price.value_of(amount);
        assert!((value.as_f64() - 1.23).abs() < 0.001);
    }

    #[test]
    fn test_large_prices() {
        // Test with very large token prices (like WBTC)
        let price = TokenPrice::new(45_000.0);
        let amount = NormalizedAmount::new(0.5);
        let value = price.value_of(amount);
        assert_eq!(value, UsdValue::new(22_500.0));
    }
}
