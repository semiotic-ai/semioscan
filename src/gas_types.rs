//! Strong types for gas-related values
//!
//! This module provides newtype wrappers around U256 to add type safety
//! for gas calculations and prevent mixing incompatible units.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul};

/// Amount of gas consumed by a transaction
///
/// This represents the total gas units consumed, not the cost.
/// To calculate cost, multiply by [`GasPrice`].
///
/// # Example
/// ```
/// use alloy_primitives::U256;
/// use semioscan::GasAmount;
///
/// let gas = GasAmount::new(21000);
/// assert_eq!(gas.as_u256(), U256::from(21000));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GasAmount(U256);

impl GasAmount {
    /// Create a new gas amount
    pub const fn new(amount: u64) -> Self {
        Self(U256::from_limbs([amount, 0, 0, 0]))
    }

    /// Create from U256
    pub const fn from_u256(amount: U256) -> Self {
        Self(amount)
    }

    /// Get the inner U256 value
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Convert to u64 if it fits, otherwise None
    pub fn as_u64(&self) -> Option<u64> {
        self.0.try_into().ok()
    }

    /// Multiply gas amount by gas price to get total cost in wei
    ///
    /// Uses saturating multiplication to prevent overflow.
    pub fn cost(&self, price: GasPrice) -> U256 {
        self.0.saturating_mul(price.0)
    }
}

impl From<u64> for GasAmount {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl From<U256> for GasAmount {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl From<GasAmount> for U256 {
    fn from(value: GasAmount) -> Self {
        value.0
    }
}

impl Add for GasAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::fmt::Display for GasAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Gas price in wei per unit of gas
///
/// This represents the price paid per gas unit, not the total cost.
/// To calculate total cost, multiply by [`GasAmount`].
///
/// # Example
/// ```
/// use alloy_primitives::U256;
/// use semioscan::{GasAmount, GasPrice};
///
/// let price = GasPrice::from_gwei(50); // 50 gwei
/// let gas = GasAmount::new(21000);
/// let cost = gas.cost(price);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GasPrice(U256);

impl GasPrice {
    /// Create a new gas price from wei
    pub const fn new(price_wei: u64) -> Self {
        Self(U256::from_limbs([price_wei, 0, 0, 0]))
    }

    /// Create from U256
    pub const fn from_u256(price: U256) -> Self {
        Self(price)
    }

    /// Create from gwei (convenience constructor)
    pub fn from_gwei(gwei: u64) -> Self {
        Self(U256::from(gwei).saturating_mul(U256::from(1_000_000_000u64)))
    }

    /// Get the inner U256 value (in wei)
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Convert to gwei as f64 (lossy, for display purposes)
    pub fn as_gwei_f64(&self) -> f64 {
        let gwei_divisor = 1_000_000_000u64;
        let whole_gwei = self.0 / U256::from(gwei_divisor);
        whole_gwei.to_string().parse::<f64>().unwrap_or(0.0)
    }

    /// Multiply by gas amount to get total cost in wei
    ///
    /// Uses saturating multiplication to prevent overflow.
    pub fn total_cost(&self, amount: GasAmount) -> U256 {
        self.0.saturating_mul(amount.0)
    }
}

impl From<u64> for GasPrice {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl From<U256> for GasPrice {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl From<GasPrice> for U256 {
    fn from(value: GasPrice) -> Self {
        value.0
    }
}

impl std::fmt::Display for GasPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} wei", self.0)
    }
}

/// Type-safe multiplication: GasAmount × GasPrice = Wei cost
impl Mul<GasPrice> for GasAmount {
    type Output = U256;

    fn mul(self, rhs: GasPrice) -> Self::Output {
        self.cost(rhs)
    }
}

/// Type-safe multiplication: GasPrice × GasAmount = Wei cost
impl Mul<GasAmount> for GasPrice {
    type Output = U256;

    fn mul(self, rhs: GasAmount) -> Self::Output {
        self.total_cost(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_amount_creation() {
        let gas = GasAmount::new(21000);
        assert_eq!(gas.as_u256(), U256::from(21000));
        assert_eq!(gas.as_u64(), Some(21000));
    }

    #[test]
    fn test_gas_price_creation() {
        let price = GasPrice::new(50_000_000_000); // 50 gwei in wei
        assert_eq!(price.as_u256(), U256::from(50_000_000_000u64));
    }

    #[test]
    fn test_gas_price_from_gwei() {
        let price = GasPrice::from_gwei(50);
        assert_eq!(price.as_u256(), U256::from(50_000_000_000u64));
    }

    #[test]
    fn test_gas_cost_calculation() {
        let gas = GasAmount::new(21000);
        let price = GasPrice::from_gwei(50);

        // 21000 gas × 50 gwei = 1,050,000 gwei = 1,050,000,000,000,000 wei
        let cost = gas.cost(price);
        assert_eq!(cost, U256::from(1_050_000_000_000_000u64));
    }

    #[test]
    fn test_type_safe_multiplication() {
        let gas = GasAmount::new(100000);
        let price = GasPrice::from_gwei(10);

        // Both orders should work
        let cost1 = gas * price;
        let cost2 = price * gas;
        assert_eq!(cost1, cost2);
        assert_eq!(cost1, U256::from(1_000_000_000_000_000u64));
    }

    #[test]
    fn test_gas_amount_addition() {
        let gas1 = GasAmount::new(21000);
        let gas2 = GasAmount::new(50000);
        let total = gas1 + gas2;
        assert_eq!(total.as_u256(), U256::from(71000));
    }

    #[test]
    fn test_saturating_arithmetic() {
        let max_gas = GasAmount::from_u256(U256::MAX);
        let price = GasPrice::from_gwei(1);

        // Should saturate, not panic
        let cost = max_gas.cost(price);
        assert_eq!(cost, U256::MAX);
    }

    #[test]
    fn test_display() {
        let gas = GasAmount::new(21000);
        assert_eq!(format!("{}", gas), "21000");

        let price = GasPrice::new(50_000_000_000);
        assert!(format!("{}", price).contains("50000000000"));
    }

    #[test]
    fn test_serialization() {
        let gas = GasAmount::new(21000);
        let json = serde_json::to_string(&gas).unwrap();
        let deserialized: GasAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(gas, deserialized);
    }

    #[test]
    fn test_conversions() {
        let value = U256::from(12345u64);

        let gas: GasAmount = value.into();
        let back: U256 = gas.into();
        assert_eq!(value, back);

        let price: GasPrice = value.into();
        let back: U256 = price.into();
        assert_eq!(value, back);
    }
}
