// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strong types for fee-related values
//!
//! Separates different kinds of fees for type safety and clarity.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::ops::Add;

/// L1 data fee for L2 transactions
///
/// L2 chains (Arbitrum, Optimism, Base, etc.) post transaction data to L1
/// for data availability. This fee represents the cost of posting that data
/// to L1 (in wei).
///
/// This is conceptually separate from L2 gas costs and can be a significant
/// portion of total transaction cost, especially for transactions with large
/// calldata.
///
/// # L2 Fee Structure
///
/// Total L2 transaction cost = L2 execution cost + L1 data fee
///
/// - **L2 execution cost**: Gas consumed on L2 × L2 gas price
/// - **L1 data fee**: Cost to post tx data to L1 (this type)
///
/// # Examples
///
/// ```
/// use alloy_primitives::U256;
/// use semioscan::L1DataFee;
///
/// let l1_fee = L1DataFee::new(U256::from(50_000_000_000_000u64)); // 0.00005 ETH
/// let l2_cost = U256::from(10_000_000_000_000u64); // 0.00001 ETH
///
/// let total = l1_fee.total_with_l2_cost(l2_cost);
/// assert_eq!(total, U256::from(60_000_000_000_000u64)); // 0.00006 ETH total
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct L1DataFee(U256);

impl L1DataFee {
    /// Zero L1 data fee (for L1 transactions or L2s without L1 fees)
    pub const ZERO: Self = Self(U256::ZERO);

    /// Create a new L1 data fee from wei
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::L1DataFee;
    ///
    /// let fee = L1DataFee::new(U256::from(1_000_000u64));
    /// assert_eq!(fee.as_u256(), U256::from(1_000_000u64));
    /// ```
    pub const fn new(fee_wei: U256) -> Self {
        Self(fee_wei)
    }

    /// Get the inner U256 value (in wei)
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Convert to u64 if it fits, otherwise None
    pub fn as_u64(&self) -> Option<u64> {
        self.0.try_into().ok()
    }

    /// Add L1 data fee to L2 execution cost for total transaction cost
    ///
    /// Uses saturating addition to prevent overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::L1DataFee;
    ///
    /// let l1_fee = L1DataFee::new(U256::from(50_000u64));
    /// let l2_cost = U256::from(10_000u64);
    /// let total = l1_fee.total_with_l2_cost(l2_cost);
    /// assert_eq!(total, U256::from(60_000u64));
    /// ```
    pub fn total_with_l2_cost(&self, l2_cost: U256) -> U256 {
        self.0.saturating_add(l2_cost)
    }

    /// Calculate percentage of total cost that is L1 data fee
    ///
    /// Returns a Percentage value (0.0 = 0%, 1.0 = 100%)
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::{L1DataFee, Percentage};
    ///
    /// let l1_fee = L1DataFee::new(U256::from(75_000u64));
    /// let total_cost = U256::from(100_000u64);
    /// let percentage = l1_fee.percentage_of_total(total_cost);
    /// assert_eq!(percentage, Percentage::new(0.75)); // 75%
    /// ```
    pub fn percentage_of_total(&self, total_cost: U256) -> Percentage {
        if total_cost.is_zero() {
            return Percentage::ZERO;
        }

        let l1_f64 = self.0.to_string().parse::<f64>().unwrap_or(0.0);
        let total_f64 = total_cost.to_string().parse::<f64>().unwrap_or(1.0);

        Percentage::new(l1_f64 / total_f64)
    }

    /// Check if L1 data fee is zero
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Convert to ETH as f64 (lossy, for display purposes)
    pub fn as_eth_f64(&self) -> f64 {
        let wei_per_eth = 1_000_000_000_000_000_000u128;
        let wei_str = self.0.to_string();
        let wei_f64 = wei_str.parse::<f64>().unwrap_or(0.0);
        wei_f64 / wei_per_eth as f64
    }
}

impl From<u64> for L1DataFee {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl From<U256> for L1DataFee {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl Add for L1DataFee {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::fmt::Display for L1DataFee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let eth = self.as_eth_f64();
        if eth < 0.000001 {
            write!(f, "{} wei (L1 data fee)", self.0)
        } else {
            write!(f, "{:.6} ETH (L1 data fee)", eth)
        }
    }
}

/// Represents a percentage value in the range [0.0, 1.0]
///
/// This type provides type safety for percentage calculations, preventing confusion
/// with other f64 values like USD amounts or raw token amounts. The value is stored
/// as a fraction (0.0 = 0%, 1.0 = 100%).
///
/// # Examples
///
/// ```
/// use semioscan::Percentage;
///
/// let percent = Percentage::new(0.75);
/// assert_eq!(percent.format(), "75.00%");
/// assert_eq!(percent.as_basis_points(), 7500);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Percentage(f64);

impl Percentage {
    /// Zero percent (0%)
    pub const ZERO: Self = Self(0.0);

    /// One hundred percent (100%)
    pub const ONE_HUNDRED: Self = Self(1.0);

    /// Create a percentage, clamping to [0.0, 1.0]
    ///
    /// Values outside the range are automatically clamped.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::Percentage;
    ///
    /// let valid = Percentage::new(0.5);
    /// assert_eq!(valid.as_f64(), 0.5);
    ///
    /// let clamped = Percentage::new(1.5); // > 1.0
    /// assert_eq!(clamped.as_f64(), 1.0);
    /// ```
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Create a percentage from basis points (10000 = 100%)
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::Percentage;
    ///
    /// let percent = Percentage::from_basis_points(7500);
    /// assert_eq!(percent.as_f64(), 0.75);
    /// ```
    pub fn from_basis_points(bps: u64) -> Self {
        Self::new(bps as f64 / 10000.0)
    }

    /// Get the inner f64 value (0.0 to 1.0)
    pub const fn as_f64(&self) -> f64 {
        self.0
    }

    /// Convert to basis points (10000 = 100%)
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::Percentage;
    ///
    /// let percent = Percentage::new(0.75);
    /// assert_eq!(percent.as_basis_points(), 7500);
    /// ```
    pub fn as_basis_points(&self) -> u64 {
        (self.0 * 10000.0).round() as u64
    }

    /// Format as percentage string (e.g., "25.50%")
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::Percentage;
    ///
    /// let percent = Percentage::new(0.7534);
    /// assert_eq!(percent.format(), "75.34%");
    /// ```
    pub fn format(&self) -> String {
        format!("{:.2}%", self.0 * 100.0)
    }

    /// Check if percentage is zero
    pub fn is_zero(&self) -> bool {
        self.0.abs() < f64::EPSILON
    }
}

impl From<f64> for Percentage {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for Percentage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2}%", self.0 * 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_data_fee_creation() {
        let fee = L1DataFee::new(U256::from(1_000_000u64));
        assert_eq!(fee.as_u256(), U256::from(1_000_000u64));
    }

    #[test]
    fn test_l1_data_fee_zero() {
        assert!(L1DataFee::ZERO.is_zero());
        assert_eq!(L1DataFee::ZERO.as_u256(), U256::ZERO);
    }

    #[test]
    fn test_total_with_l2_cost() {
        let l1_fee = L1DataFee::new(U256::from(50_000u64));
        let l2_cost = U256::from(10_000u64);
        let total = l1_fee.total_with_l2_cost(l2_cost);
        assert_eq!(total, U256::from(60_000u64));
    }

    #[test]
    fn test_percentage_of_total() {
        let l1_fee = L1DataFee::new(U256::from(75_000u64));
        let total_cost = U256::from(100_000u64);
        let percentage = l1_fee.percentage_of_total(total_cost);
        assert!((percentage.as_f64() - 0.75).abs() < 0.0001);
    }

    #[test]
    fn test_percentage_of_total_zero_total() {
        let l1_fee = L1DataFee::new(U256::from(1_000u64));
        let percentage = l1_fee.percentage_of_total(U256::ZERO);
        assert_eq!(percentage, Percentage::ZERO);
    }

    #[test]
    fn test_percentage_of_total_zero_fee() {
        let l1_fee = L1DataFee::ZERO;
        let total_cost = U256::from(100_000u64);
        let percentage = l1_fee.percentage_of_total(total_cost);
        assert_eq!(percentage, Percentage::ZERO);
    }

    #[test]
    fn test_addition() {
        let fee1 = L1DataFee::new(U256::from(1_000u64));
        let fee2 = L1DataFee::new(U256::from(2_000u64));
        let total = fee1 + fee2;
        assert_eq!(total.as_u256(), U256::from(3_000u64));
    }

    #[test]
    fn test_saturating_addition() {
        let max_fee = L1DataFee::new(U256::MAX);
        let small_fee = L1DataFee::new(U256::from(1u64));
        let result = max_fee + small_fee;
        assert_eq!(result.as_u256(), U256::MAX);
    }

    #[test]
    fn test_as_u64() {
        let small_fee = L1DataFee::new(U256::from(12345u64));
        assert_eq!(small_fee.as_u64(), Some(12345u64));

        let large_fee = L1DataFee::new(U256::MAX);
        assert_eq!(large_fee.as_u64(), None);
    }

    #[test]
    fn test_as_eth_f64() {
        // 0.001 ETH in wei
        let fee = L1DataFee::new(U256::from(1_000_000_000_000_000u64));
        let eth = fee.as_eth_f64();
        assert!((eth - 0.001).abs() < 0.0000001);
    }

    #[test]
    fn test_display_small_fee() {
        let fee = L1DataFee::new(U256::from(100u64));
        let display = format!("{}", fee);
        assert!(display.contains("100 wei"));
        assert!(display.contains("L1 data fee"));
    }

    #[test]
    fn test_display_large_fee() {
        // 0.01 ETH in wei
        let fee = L1DataFee::new(U256::from(10_000_000_000_000_000u64));
        let display = format!("{}", fee);
        assert!(display.contains("0.01"));
        assert!(display.contains("ETH"));
        assert!(display.contains("L1 data fee"));
    }

    #[test]
    fn test_conversions() {
        let u256_val = U256::from(12345u64);
        let fee: L1DataFee = u256_val.into();
        let back: U256 = fee.as_u256();
        assert_eq!(u256_val, back);

        let u64_val = 12345u64;
        let fee: L1DataFee = u64_val.into();
        assert_eq!(fee.as_u256(), U256::from(u64_val));
    }

    #[test]
    fn test_serialization() {
        let fee = L1DataFee::new(U256::from(12345u64));
        let json = serde_json::to_string(&fee).unwrap();
        let deserialized: L1DataFee = serde_json::from_str(&json).unwrap();
        assert_eq!(fee, deserialized);
    }

    #[test]
    fn test_ordering() {
        let small = L1DataFee::new(U256::from(100u64));
        let medium = L1DataFee::new(U256::from(500u64));
        let large = L1DataFee::new(U256::from(1000u64));

        assert!(small < medium);
        assert!(medium < large);
        assert!(small < large);
    }

    #[test]
    fn test_real_world_scenario() {
        // Simulate a typical Arbitrum transaction
        // L2 execution: 100,000 gas × 0.1 gwei = 10,000 gwei = 0.00001 ETH
        let l2_execution_cost = U256::from(10_000_000_000_000u64);

        // L1 data fee: ~0.00005 ETH (common for medium-sized tx)
        let l1_fee = L1DataFee::new(U256::from(50_000_000_000_000u64));

        // Total cost
        let total = l1_fee.total_with_l2_cost(l2_execution_cost);
        assert_eq!(total, U256::from(60_000_000_000_000u64)); // 0.00006 ETH

        // L1 fee is ~83% of total (common on L2s)
        let percentage = l1_fee.percentage_of_total(total);
        assert!((percentage.as_f64() - 0.833).abs() < 0.01);
    }

    #[test]
    fn test_l1_only_transaction() {
        // For L1 transactions, L1 data fee should be zero
        let l1_fee = L1DataFee::ZERO;
        let gas_cost = U256::from(1_000_000u64);
        let total = l1_fee.total_with_l2_cost(gas_cost);

        assert_eq!(total, gas_cost);
        assert_eq!(l1_fee.percentage_of_total(total), Percentage::ZERO);
    }
}
