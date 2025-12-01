// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strong types for gas-related values
//!
//! This module provides newtype wrappers around U256 to add type safety
//! for gas calculations and prevent mixing incompatible units.

use alloy_eips::eip4844::{
    DATA_GAS_PER_BLOB, MAX_BLOBS_PER_BLOCK_DENCUN, TARGET_BLOBS_PER_BLOCK_DENCUN,
};
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

impl std::fmt::Display for GasPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let gwei = self.as_gwei_f64();
        if gwei >= 1.0 {
            write!(f, "{:.2} gwei", gwei)
        } else {
            write!(f, "{} wei", self.0)
        }
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
        let max_gas = GasAmount::from(U256::MAX);
        let price = GasPrice::from_gwei(1);

        // Should saturate, not panic
        let cost = max_gas.cost(price);
        assert_eq!(cost, U256::MAX);
    }

    #[test]
    fn test_display() {
        let gas = GasAmount::new(21000);
        assert_eq!(format!("{}", gas), "21000");

        let price = GasPrice::new(50_000_000_000); // 50 gwei
        assert_eq!(format!("{}", price), "50.00 gwei");

        let small_price = GasPrice::new(100); // < 1 gwei
        assert_eq!(format!("{}", small_price), "100 wei");
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
        let back: U256 = gas.as_u256();
        assert_eq!(value, back);

        let price: GasPrice = value.into();
        let back: U256 = price.as_u256();
        assert_eq!(value, back);
    }
}

/// Represents the number of EIP-4844 blobs in a transaction
///
/// EIP-4844 introduced blob-carrying transactions that can include
/// "blob versioned hashes" referencing data blobs stored separately.
/// Each blob is 128KB and costs a fixed amount of blob gas.
///
/// # Examples
///
/// ```
/// use semioscan::BlobCount;
///
/// let count = BlobCount::new(2);
/// assert_eq!(count.as_usize(), 2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobCount(usize);

impl BlobCount {
    /// Zero blobs
    pub const ZERO: Self = Self(0);

    /// Target number of blobs per block (EIP-4844 Dencun target = 3)
    pub const TARGET: Self = Self(TARGET_BLOBS_PER_BLOCK_DENCUN as usize);

    /// Maximum number of blobs per block (EIP-4844 Dencun max = 6)
    pub const MAX: Self = Self(MAX_BLOBS_PER_BLOCK_DENCUN);

    /// Create a new blob count (unchecked)
    ///
    /// For validation against EIP-4844 limits, use [`BlobCount::new_checked`].
    pub const fn new(count: usize) -> Self {
        Self(count)
    }

    /// Create a new blob count with validation
    ///
    /// Returns `None` if count exceeds `MAX_BLOBS_PER_BLOCK_DENCUN` (6 blobs).
    /// Per EIP-4844 Dencun upgrade, transactions can include 1-6 blobs.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::BlobCount;
    ///
    /// assert!(BlobCount::new_checked(3).is_some()); // Valid
    /// assert!(BlobCount::new_checked(6).is_some()); // Valid (max)
    /// assert!(BlobCount::new_checked(7).is_none()); // Invalid (exceeds max)
    /// ```
    pub fn new_checked(count: usize) -> Option<Self> {
        if count <= MAX_BLOBS_PER_BLOCK_DENCUN {
            Some(Self(count))
        } else {
            None
        }
    }

    /// Get the inner usize value
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// Calculate the blob gas amount for this blob count
    ///
    /// Uses the EIP-4844 constant `DATA_GAS_PER_BLOB` (131,072 gas per blob).
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy_primitives::U256;
    /// use semioscan::BlobCount;
    ///
    /// let count = BlobCount::new(2);
    /// let gas = count.to_blob_gas_amount();
    /// assert_eq!(gas.as_u256(), U256::from(262_144)); // 2 * 131_072
    /// ```
    pub fn to_blob_gas_amount(&self) -> BlobGasAmount {
        BlobGasAmount::new(U256::from(self.0 * DATA_GAS_PER_BLOB as usize))
    }

    /// Check if this blob count is within EIP-4844 Dencun limits
    ///
    /// Returns `true` if count is between 0 and `MAX_BLOBS_PER_BLOCK_DENCUN` (6).
    pub fn is_valid(&self) -> bool {
        self.0 <= MAX_BLOBS_PER_BLOCK_DENCUN
    }
}

impl From<usize> for BlobCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for BlobCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 1 {
            write!(f, "1 blob")
        } else {
            write!(f, "{} blobs", self.0)
        }
    }
}

/// Represents the amount of blob gas consumed by EIP-4844 transactions
///
/// Blob gas is separate from regular gas and has its own pricing mechanism.
/// Each blob consumes a fixed amount of blob gas (DATA_GAS_PER_BLOB = 131,072).
///
/// # Examples
///
/// ```
/// use alloy_primitives::U256;
/// use semioscan::BlobGasAmount;
///
/// let blob_gas = BlobGasAmount::new(U256::from(131_072));
/// assert_eq!(blob_gas.as_u256(), U256::from(131_072));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobGasAmount(U256);

impl BlobGasAmount {
    /// Zero blob gas
    pub const ZERO: Self = Self(U256::ZERO);

    /// Create a new blob gas amount
    pub const fn new(gas: U256) -> Self {
        Self(gas)
    }

    /// Get the inner U256 value
    pub const fn as_u256(&self) -> U256 {
        self.0
    }
}

impl From<U256> for BlobGasAmount {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl Add for BlobGasAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::fmt::Display for BlobGasAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} blob gas", self.0)
    }
}

#[cfg(test)]
mod blob_tests {
    use super::*;

    #[test]
    fn test_blob_count_constants() {
        assert_eq!(BlobCount::ZERO.as_usize(), 0);
        assert_eq!(BlobCount::TARGET.as_usize(), 3);
        assert_eq!(BlobCount::MAX.as_usize(), 6);
    }

    #[test]
    fn test_blob_count_new() {
        let count = BlobCount::new(3);
        assert_eq!(count.as_usize(), 3);
    }

    #[test]
    fn test_blob_count_new_checked_valid() {
        assert!(BlobCount::new_checked(0).is_some());
        assert!(BlobCount::new_checked(1).is_some());
        assert!(BlobCount::new_checked(3).is_some());
        assert!(BlobCount::new_checked(6).is_some());
    }

    #[test]
    fn test_blob_count_new_checked_invalid() {
        assert!(BlobCount::new_checked(7).is_none());
        assert!(BlobCount::new_checked(10).is_none());
        assert!(BlobCount::new_checked(100).is_none());
    }

    #[test]
    fn test_blob_count_is_valid() {
        assert!(BlobCount::new(0).is_valid());
        assert!(BlobCount::new(3).is_valid());
        assert!(BlobCount::new(6).is_valid());
        assert!(!BlobCount::new(7).is_valid());
        assert!(!BlobCount::new(100).is_valid());
    }

    #[test]
    fn test_blob_count_to_blob_gas_amount() {
        let count = BlobCount::new(0);
        let gas = count.to_blob_gas_amount();
        assert_eq!(gas.as_u256(), U256::ZERO);

        let count = BlobCount::new(1);
        let gas = count.to_blob_gas_amount();
        assert_eq!(gas.as_u256(), U256::from(131_072)); // DATA_GAS_PER_BLOB

        let count = BlobCount::new(2);
        let gas = count.to_blob_gas_amount();
        assert_eq!(gas.as_u256(), U256::from(262_144)); // 2 * 131_072

        let count = BlobCount::new(6);
        let gas = count.to_blob_gas_amount();
        assert_eq!(gas.as_u256(), U256::from(786_432)); // 6 * 131_072
    }

    #[test]
    fn test_blob_count_display() {
        assert_eq!(format!("{}", BlobCount::new(1)), "1 blob");
        assert_eq!(format!("{}", BlobCount::new(2)), "2 blobs");
        assert_eq!(format!("{}", BlobCount::new(6)), "6 blobs");
    }

    #[test]
    fn test_blob_count_from() {
        let count: BlobCount = 3usize.into();
        assert_eq!(count.as_usize(), 3);
    }

    #[test]
    fn test_blob_gas_amount_creation() {
        let gas = BlobGasAmount::new(U256::from(131_072));
        assert_eq!(gas.as_u256(), U256::from(131_072));
    }

    #[test]
    fn test_blob_gas_amount_zero() {
        assert_eq!(BlobGasAmount::ZERO.as_u256(), U256::ZERO);
    }

    #[test]
    fn test_blob_gas_amount_addition() {
        let gas1 = BlobGasAmount::new(U256::from(131_072));
        let gas2 = BlobGasAmount::new(U256::from(131_072));
        let total = gas1 + gas2;
        assert_eq!(total.as_u256(), U256::from(262_144));
    }

    #[test]
    fn test_blob_gas_amount_saturating_add() {
        let max_gas = BlobGasAmount::from(U256::MAX);
        let gas = BlobGasAmount::new(U256::from(1));
        let result = max_gas + gas;
        assert_eq!(result.as_u256(), U256::MAX);
    }

    #[test]
    fn test_blob_gas_amount_display() {
        let gas = BlobGasAmount::new(U256::from(131_072));
        assert_eq!(format!("{}", gas), "131072 blob gas");
    }

    #[test]
    fn test_blob_gas_amount_from() {
        let gas: BlobGasAmount = U256::from(262_144).into();
        assert_eq!(gas.as_u256(), U256::from(262_144));
    }

    #[test]
    fn test_eip4844_constants() {
        // Verify we're using the correct EIP-4844 Dencun constants
        assert_eq!(DATA_GAS_PER_BLOB, 131_072);
        assert_eq!(MAX_BLOBS_PER_BLOCK_DENCUN, 6);
        assert_eq!(TARGET_BLOBS_PER_BLOCK_DENCUN, 3);
    }

    #[test]
    fn test_blob_count_serialization() {
        let count = BlobCount::new(3);
        let json = serde_json::to_string(&count).unwrap();
        let deserialized: BlobCount = serde_json::from_str(&json).unwrap();
        assert_eq!(count, deserialized);
    }

    #[test]
    fn test_blob_gas_amount_serialization() {
        let gas = BlobGasAmount::new(U256::from(131_072));
        let json = serde_json::to_string(&gas).unwrap();
        let deserialized: BlobGasAmount = serde_json::from_str(&json).unwrap();
        assert_eq!(gas, deserialized);
    }
}
