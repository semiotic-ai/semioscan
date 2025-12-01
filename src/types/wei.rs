// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strong types for native currency amounts
//!
//! This module provides newtype wrappers for native currency (ETH, MATIC, etc.)
//! in wei to prevent confusion with ERC-20 token amounts.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::ops::Add;

/// Represents an amount of native currency (ETH, MATIC, etc.) in wei
///
/// This type is distinct from [`TokenAmount`](crate::TokenAmount) to prevent
/// mixing native currency amounts with ERC-20 token amounts in calculations.
///
/// # Examples
///
/// ```
/// use alloy_primitives::U256;
/// use semioscan::WeiAmount;
///
/// let gas_cost = WeiAmount::new(U256::from(1_000_000_000_000_000u64)); // 0.001 ETH
/// let eth = gas_cost.to_ether();
/// assert!((eth - 0.001).abs() < 0.0000001);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct WeiAmount(U256);

impl WeiAmount {
    /// Zero wei amount
    pub const ZERO: Self = Self(U256::ZERO);

    /// Create a new wei amount
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::WeiAmount;
    ///
    /// let amount = WeiAmount::new(U256::from(1000));
    /// assert_eq!(amount.as_u256(), U256::from(1000));
    /// ```
    pub const fn new(wei: U256) -> Self {
        Self(wei)
    }

    /// Get the inner U256 value (in wei)
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Convert to u64 if it fits, otherwise None
    pub fn as_u64(&self) -> Option<u64> {
        self.0.try_into().ok()
    }

    /// Check if the amount is zero
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Convert to gwei (1 gwei = 10^9 wei)
    ///
    /// Returns f64 for display purposes. This is a lossy conversion.
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::WeiAmount;
    ///
    /// let amount = WeiAmount::new(U256::from(5_000_000_000u64)); // 5 gwei
    /// let gwei = amount.to_gwei();
    /// assert!((gwei - 5.0).abs() < 0.0001);
    /// ```
    pub fn to_gwei(&self) -> f64 {
        let gwei_divisor = U256::from(1_000_000_000u64);
        self.0.to_string().parse::<f64>().unwrap_or(0.0)
            / gwei_divisor.to_string().parse::<f64>().unwrap_or(1.0)
    }

    /// Convert to ether (1 ETH = 10^18 wei)
    ///
    /// Returns f64 for display purposes. This is a lossy conversion.
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::WeiAmount;
    ///
    /// let amount = WeiAmount::new(U256::from(1_500_000_000_000_000_000u128)); // 1.5 ETH
    /// let eth = amount.to_ether();
    /// assert!((eth - 1.5).abs() < 0.0001);
    /// ```
    pub fn to_ether(&self) -> f64 {
        let eth_divisor = U256::from(1_000_000_000_000_000_000u128);
        self.0.to_string().parse::<f64>().unwrap_or(0.0)
            / eth_divisor.to_string().parse::<f64>().unwrap_or(1.0)
    }
}

impl From<u64> for WeiAmount {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl From<U256> for WeiAmount {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl Add for WeiAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::fmt::Display for WeiAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let eth = self.to_ether();
        if eth < 0.000001 {
            write!(f, "{} wei", self.0)
        } else {
            write!(f, "{:.6} ETH", eth)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wei_amount_creation() {
        let amount = WeiAmount::new(U256::from(1000));
        assert_eq!(amount.as_u256(), U256::from(1000));
    }

    #[test]
    fn test_wei_amount_zero() {
        assert!(WeiAmount::ZERO.is_zero());
        assert_eq!(WeiAmount::ZERO.as_u256(), U256::ZERO);
    }

    #[test]
    fn test_wei_amount_addition() {
        let a = WeiAmount::new(U256::from(500));
        let b = WeiAmount::new(U256::from(300));
        let sum = a + b;
        assert_eq!(sum.as_u256(), U256::from(800));
    }

    #[test]
    fn test_saturating_addition() {
        let max_amount = WeiAmount::new(U256::MAX);
        let small_amount = WeiAmount::new(U256::from(1u64));
        let result = max_amount + small_amount;
        assert_eq!(result.as_u256(), U256::MAX);
    }

    #[test]
    fn test_to_gwei() {
        let amount = WeiAmount::new(U256::from(5_000_000_000u64)); // 5 gwei
        let gwei = amount.to_gwei();
        assert!((gwei - 5.0).abs() < 0.0001);
    }

    #[test]
    fn test_to_ether() {
        let amount = WeiAmount::new(U256::from(1_500_000_000_000_000_000u128)); // 1.5 ETH
        let eth = amount.to_ether();
        assert!((eth - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_as_u64() {
        let small_amount = WeiAmount::new(U256::from(12345u64));
        assert_eq!(small_amount.as_u64(), Some(12345u64));

        let large_amount = WeiAmount::new(U256::MAX);
        assert_eq!(large_amount.as_u64(), None);
    }

    #[test]
    fn test_display_small_amount() {
        let amount = WeiAmount::new(U256::from(100u64));
        let display = format!("{}", amount);
        assert!(display.contains("100 wei"));
    }

    #[test]
    fn test_display_large_amount() {
        // 0.01 ETH in wei
        let amount = WeiAmount::new(U256::from(10_000_000_000_000_000u64));
        let display = format!("{}", amount);
        assert!(display.contains("0.01"));
        assert!(display.contains("ETH"));
    }

    #[test]
    fn test_serialization() {
        let amount = WeiAmount::new(U256::from(1000));
        let json = serde_json::to_string(&amount).unwrap();
        let deserialized: WeiAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(amount, deserialized);
    }

    #[test]
    fn test_conversions() {
        let u256_val = U256::from(12345u64);
        let amount: WeiAmount = u256_val.into();
        let back: U256 = amount.as_u256();
        assert_eq!(u256_val, back);

        let u64_val = 12345u64;
        let amount: WeiAmount = u64_val.into();
        assert_eq!(amount.as_u256(), U256::from(u64_val));
    }

    #[test]
    fn test_ordering() {
        let small = WeiAmount::new(U256::from(100u64));
        let medium = WeiAmount::new(U256::from(500u64));
        let large = WeiAmount::new(U256::from(1000u64));

        assert!(small < medium);
        assert!(medium < large);
        assert!(small < large);
    }

    #[test]
    fn test_gas_cost_scenario() {
        // 100,000 gas at 50 gwei = 0.005 ETH
        let gas_units = 100_000u64;
        let gas_price_gwei = 50u64;
        let gas_price_wei = gas_price_gwei * 1_000_000_000u64;
        let total_cost = WeiAmount::new(U256::from(gas_units * gas_price_wei));

        let eth = total_cost.to_ether();
        assert!((eth - 0.005).abs() < 0.000001);
    }
}
