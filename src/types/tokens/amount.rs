// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Raw token amount type

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::ops::Add;

use super::decimals::TokenDecimals;
use super::normalized::NormalizedAmount;

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
    fn test_display_formatting() {
        let amount = TokenAmount::new(U256::from(12345u64));
        assert_eq!(format!("{}", amount), "12345");
    }

    #[test]
    fn test_serialization() {
        let amount = TokenAmount::new(U256::from(12345u64));
        let json = serde_json::to_string(&amount).unwrap();
        let deserialized: TokenAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(amount, deserialized);
    }

    #[test]
    fn test_conversions() {
        let u256_val = U256::from(12345u64);
        let amount: TokenAmount = u256_val.into();
        let back: U256 = amount.as_u256();
        assert_eq!(u256_val, back);
    }
}
