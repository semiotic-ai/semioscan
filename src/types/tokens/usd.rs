//! USD value type for financial calculations

use serde::{Deserialize, Serialize};
use std::ops::Add;

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
    fn test_usd_value_creation() {
        let value = UsdValue::new(100.50);
        assert_eq!(value.as_f64(), 100.50);
    }

    #[test]
    fn test_usd_value_zero() {
        assert!(UsdValue::ZERO.is_zero());
        assert!(UsdValue::new(0.0).is_zero());
        assert!(!UsdValue::new(0.1).is_zero());
    }

    #[test]
    fn test_usd_value_format() {
        let value = UsdValue::new(1234.567);
        assert_eq!(value.format(2), "$1234.57");
        assert_eq!(value.format(0), "$1235");
        assert_eq!(value.format(3), "$1234.567");
    }

    #[test]
    fn test_usd_value_arithmetic() {
        let val1 = UsdValue::new(100.0);
        let val2 = UsdValue::new(50.0);

        let sum = val1 + val2;
        assert_eq!(sum.as_f64(), 150.0);

        let diff = val1 - val2;
        assert_eq!(diff.as_f64(), 50.0);
    }

    #[test]
    fn test_usd_value_abs() {
        let negative = UsdValue::new(-100.0);
        assert_eq!(negative.abs().as_f64(), 100.0);

        let positive = UsdValue::new(100.0);
        assert_eq!(positive.abs().as_f64(), 100.0);
    }

    #[test]
    fn test_display_formatting() {
        let value = UsdValue::new(1234.567);
        assert_eq!(format!("{}", value), "$1234.57");
    }

    #[test]
    fn test_serialization() {
        let value = UsdValue::new(100.50);
        let json = serde_json::to_string(&value).unwrap();
        let deserialized: UsdValue = serde_json::from_str(&json).unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn test_conversions() {
        let f64_val = 100.50;
        let usd: UsdValue = f64_val.into();
        let back: f64 = usd.as_f64();
        assert_eq!(f64_val, back);
    }
}
