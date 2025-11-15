//! Token decimal precision type

use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_display_formatting() {
        let decimals = TokenDecimals::STANDARD;
        assert_eq!(format!("{}", decimals), "18 decimals");
    }

    #[test]
    fn test_serialization() {
        let decimals = TokenDecimals::STANDARD;
        let json = serde_json::to_string(&decimals).unwrap();
        let deserialized: TokenDecimals = serde_json::from_str(&json).unwrap();
        assert_eq!(decimals, deserialized);
    }

    #[test]
    fn test_conversions() {
        let u8_val: u8 = 18;
        let decimals: TokenDecimals = u8_val.into();
        let back: u8 = decimals.as_u8();
        assert_eq!(u8_val, back);
    }
}
