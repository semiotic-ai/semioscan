//! USD value type for financial calculations

use serde::{Deserialize, Serialize};
use std::ops::Add;
use thiserror::Error;

/// Errors that can occur when creating a USD value
#[derive(Debug, Error, Clone, Copy)]
pub enum UsdValueError {
    #[error("USD value cannot be negative: {0}")]
    Negative(f64),
    #[error("USD value cannot be NaN")]
    NaN,
    #[error("USD value cannot be infinite: {0}")]
    Infinite(f64),
}

/// Represents a USD-denominated value for amounts, balances, and prices
///
/// This type provides type safety for financial calculations involving USD values,
/// preventing confusion with other f64 values like percentages or raw token amounts.
///
/// # Validation Rules
///
/// USD values must be:
/// - Non-negative (with tolerance for floating point rounding errors)
/// - Finite (not infinite)
/// - Not NaN
///
/// Values within $0.000001 (one microdollar) of zero (including tiny negative values
/// from floating point errors) are automatically clamped to zero. Larger negative values
/// are rejected. This tolerance is well below any practical financial threshold.
///
/// # Design Note
///
/// This type is designed for values that are semantically always positive:
/// - Token balances and swap amounts
/// - Prices per token
/// - Total value locked (TVL)
/// - Liquidation amounts
///
/// For values that can legitimately be negative (PnL, debt, price deltas), consider
/// using a signed type or `f64` directly with appropriate documentation.
///
/// # Examples
///
/// ```
/// use semioscan::UsdValue;
///
/// let price = UsdValue::new(1800.50);
/// let formatted = price.format(2);
/// assert_eq!(formatted, "$1800.50");
///
/// // Fallible construction for external data
/// let result = UsdValue::try_new(100.0);
/// assert!(result.is_ok());
///
/// let invalid = UsdValue::try_new(-100.0);
/// assert!(invalid.is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct UsdValue(f64);

impl std::hash::Hash for UsdValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl UsdValue {
    /// Zero USD value
    pub const ZERO: Self = Self(0.0);

    /// Create a new USD value with validation
    ///
    /// # Panics
    ///
    /// Panics if the value is negative, NaN, or infinite.
    /// For fallible construction, use [`UsdValue::try_new`].
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// let value = UsdValue::new(100.50);
    /// assert_eq!(value.as_f64(), 100.50);
    /// ```
    ///
    /// ```should_panic
    /// use semioscan::UsdValue;
    ///
    /// let invalid = UsdValue::new(-100.0); // Panics
    /// ```
    pub fn new(value: f64) -> Self {
        Self::try_new(value).unwrap_or_else(|e| panic!("Invalid USD value: {}", e))
    }

    /// Try to create a new USD value with validation
    ///
    /// Returns an error if the value is negative, NaN, or infinite.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// let valid = UsdValue::try_new(100.50);
    /// assert!(valid.is_ok());
    ///
    /// let negative = UsdValue::try_new(-100.0);
    /// assert!(negative.is_err());
    ///
    /// let nan = UsdValue::try_new(f64::NAN);
    /// assert!(nan.is_err());
    /// ```
    pub fn try_new(value: f64) -> Result<Self, UsdValueError> {
        if value.is_nan() {
            return Err(UsdValueError::NaN);
        }
        if value.is_infinite() {
            return Err(UsdValueError::Infinite(value));
        }
        // Allow tiny negative values due to accumulated floating point errors
        // Use a practical threshold: one microdollar ($0.000001)
        // This is well below any meaningful financial threshold
        const TOLERANCE: f64 = 1e-6;
        if value < -TOLERANCE {
            return Err(UsdValueError::Negative(value));
        }
        // Clamp tiny negative values to zero
        Ok(Self(value.max(0.0)))
    }

    /// Create from a value known to be valid at compile time
    ///
    /// Use this for constants or values you're certain are non-negative.
    /// No runtime validation is performed.
    ///
    /// # Safety
    ///
    /// The caller must ensure the value is non-negative, finite, and not NaN.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// const HUNDRED_DOLLARS: UsdValue = UsdValue::from_non_negative(100.0);
    /// ```
    pub const fn from_non_negative(value: f64) -> Self {
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

// Note: We intentionally do NOT implement From<f64> because it would bypass validation.
// Use UsdValue::new() for infallible construction or UsdValue::try_new() for fallible.

impl Add for UsdValue {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for UsdValue {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::Sub for UsdValue {
    type Output = Self;

    /// Subtract two USD values, saturating at zero
    ///
    /// Since USD values cannot be negative, subtraction is saturating.
    /// If the result would be negative, it returns zero instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::UsdValue;
    ///
    /// let a = UsdValue::new(100.0);
    /// let b = UsdValue::new(30.0);
    /// assert_eq!((a - b).as_f64(), 70.0);
    ///
    /// // Saturates at zero
    /// let c = UsdValue::new(10.0);
    /// let d = UsdValue::new(50.0);
    /// assert_eq!((c - d).as_f64(), 0.0);
    /// ```
    fn sub(self, rhs: Self) -> Self::Output {
        Self((self.0 - rhs.0).max(0.0))
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
    fn test_try_new_valid() {
        let result = UsdValue::try_new(100.50);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_f64(), 100.50);

        let zero = UsdValue::try_new(0.0);
        assert!(zero.is_ok());
    }

    #[test]
    fn test_try_new_negative() {
        let result = UsdValue::try_new(-100.0);
        assert!(result.is_err());
        match result {
            Err(UsdValueError::Negative(v)) => assert_eq!(v, -100.0),
            _ => panic!("Expected Negative error"),
        }
    }

    #[test]
    fn test_try_new_tiny_negative_clamped_to_zero() {
        // Floating point errors can produce tiny negative values
        // These should be clamped to zero, not rejected
        let tiny_negative = -0.0000000005433305882411751; // Real value from production
        let result = UsdValue::try_new(tiny_negative);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_f64(), 0.0);

        // Values within tolerance (1e-6) should be clamped
        let within_tolerance = -0.0000005; // -0.5 microdollars
        assert!(UsdValue::try_new(within_tolerance).unwrap().as_f64() == 0.0);
    }

    #[test]
    fn test_try_new_negative_beyond_tolerance_rejected() {
        // Values more negative than tolerance (1e-6) should be rejected
        let beyond_tolerance = -0.000002; // -2 microdollars
        let result = UsdValue::try_new(beyond_tolerance);
        assert!(result.is_err());
        assert!(matches!(result, Err(UsdValueError::Negative(_))));

        // A clearly negative value like -1 cent should definitely be rejected
        assert!(UsdValue::try_new(-0.01).is_err());
    }

    #[test]
    fn test_try_new_nan() {
        let result = UsdValue::try_new(f64::NAN);
        assert!(result.is_err());
        assert!(matches!(result, Err(UsdValueError::NaN)));
    }

    #[test]
    fn test_try_new_infinite() {
        let result = UsdValue::try_new(f64::INFINITY);
        assert!(result.is_err());
        assert!(matches!(result, Err(UsdValueError::Infinite(_))));
    }

    #[test]
    #[should_panic(expected = "Invalid USD value")]
    fn test_new_panics_on_negative() {
        let _value = UsdValue::new(-100.0);
    }

    #[test]
    #[should_panic(expected = "Invalid USD value")]
    fn test_new_panics_on_nan() {
        let _value = UsdValue::new(f64::NAN);
    }

    #[test]
    fn test_subtraction_saturates() {
        let a = UsdValue::new(100.0);
        let b = UsdValue::new(30.0);
        assert_eq!((a - b).as_f64(), 70.0);

        // Should saturate at zero, not go negative
        let c = UsdValue::new(10.0);
        let d = UsdValue::new(50.0);
        assert_eq!((c - d).as_f64(), 0.0);
    }

    #[test]
    fn test_from_non_negative_const() {
        const HUNDRED: UsdValue = UsdValue::from_non_negative(100.0);
        assert_eq!(HUNDRED.as_f64(), 100.0);
    }
}
