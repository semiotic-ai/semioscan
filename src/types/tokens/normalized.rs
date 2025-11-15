//! Normalized (human-readable) token amount type

use serde::{Deserialize, Serialize};
use std::ops::Add;

use super::usd::UsdValue;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        let normalized = NormalizedAmount::new(1.234567890);
        assert_eq!(format!("{}", normalized), "1.234568"); // 6 decimal places
    }

    #[test]
    fn test_serialization() {
        let normalized = NormalizedAmount::new(1.5);
        let json = serde_json::to_string(&normalized).unwrap();
        let deserialized: NormalizedAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(normalized, deserialized);
    }

    #[test]
    fn test_conversions() {
        let f64_val = 1.5;
        let normalized: NormalizedAmount = f64_val.into();
        let back: f64 = normalized.as_f64();
        assert_eq!(f64_val, back);
    }
}
