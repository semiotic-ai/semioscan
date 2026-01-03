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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
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

/// Blob gas price in wei per unit of blob gas (EIP-4844)
///
/// This represents the price paid per unit of blob gas, which is separate
/// from regular execution gas pricing. The blob base fee follows its own
/// EIP-4844 pricing mechanism based on blob supply and demand.
///
/// # Example
/// ```
/// use alloy_primitives::U256;
/// use semioscan::BlobGasPrice;
///
/// let price = BlobGasPrice::new(U256::from(1_000_000)); // ~1 gwei
/// let blob_gas = U256::from(131_072); // 1 blob
/// let cost = price.cost_for_gas(blob_gas);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct BlobGasPrice(U256);

impl BlobGasPrice {
    /// Zero blob gas price
    pub const ZERO: Self = Self(U256::ZERO);

    /// Create a new blob gas price from wei
    pub const fn new(price_wei: U256) -> Self {
        Self(price_wei)
    }

    /// Create from gwei (convenience constructor)
    pub fn from_gwei(gwei: u64) -> Self {
        Self(U256::from(gwei).saturating_mul(U256::from(1_000_000_000u64)))
    }

    /// Get the inner U256 value (in wei)
    pub const fn as_u256(&self) -> U256 {
        self.0
    }

    /// Calculate cost for a given amount of blob gas
    ///
    /// Uses saturating multiplication to prevent overflow.
    pub fn cost_for_gas(&self, blob_gas: U256) -> U256 {
        self.0.saturating_mul(blob_gas)
    }

    /// Calculate cost for a specific blob count
    pub fn cost_for_blobs(&self, count: BlobCount) -> U256 {
        self.cost_for_gas(count.to_blob_gas_amount().as_u256())
    }

    /// Convert to gwei as f64 (lossy, for display purposes)
    pub fn as_gwei_f64(&self) -> f64 {
        let gwei_divisor = 1_000_000_000u64;
        let whole_gwei = self.0 / U256::from(gwei_divisor);
        whole_gwei.to_string().parse::<f64>().unwrap_or(0.0)
    }
}

impl From<u64> for BlobGasPrice {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl From<u128> for BlobGasPrice {
    fn from(value: u128) -> Self {
        Self(U256::from(value))
    }
}

impl From<U256> for BlobGasPrice {
    fn from(value: U256) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for BlobGasPrice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let gwei = self.as_gwei_f64();
        if gwei >= 1.0 {
            write!(f, "{gwei:.4} gwei (blob)")
        } else {
            write!(f, "{} wei (blob)", self.0)
        }
    }
}

/// Detailed breakdown of gas costs for a transaction
///
/// Separates execution gas, blob gas, and L1 data fees for comprehensive
/// analytics and cost attribution.
///
/// # Example
/// ```
/// use alloy_primitives::U256;
/// use semioscan::{GasBreakdown, BlobGasPrice};
///
/// let breakdown = GasBreakdown::builder()
///     .execution_gas_cost(U256::from(1_000_000_000_000_000u64))
///     .blob_gas_cost(U256::from(500_000_000_000_000u64))
///     .build();
///
/// assert_eq!(
///     breakdown.total_cost(),
///     U256::from(1_500_000_000_000_000u64)
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GasBreakdown {
    /// Cost for regular execution gas (gas_used * effective_gas_price)
    pub execution_gas_cost: U256,
    /// Cost for blob gas (blob_gas_used * blob_gas_price) - EIP-4844 only
    pub blob_gas_cost: U256,
    /// L1 data fee for OP-stack chains (Optimism, Base, etc.)
    pub l1_data_fee: U256,
    /// Number of blobs in the transaction (0 for non-EIP-4844)
    pub blob_count: BlobCount,
    /// Blob gas price used for this transaction
    pub blob_gas_price: BlobGasPrice,
}

impl GasBreakdown {
    /// Create a new empty gas breakdown
    pub const fn new() -> Self {
        Self {
            execution_gas_cost: U256::ZERO,
            blob_gas_cost: U256::ZERO,
            l1_data_fee: U256::ZERO,
            blob_count: BlobCount::ZERO,
            blob_gas_price: BlobGasPrice::ZERO,
        }
    }

    /// Create a builder for constructing a gas breakdown
    pub fn builder() -> GasBreakdownBuilder {
        GasBreakdownBuilder::new()
    }

    /// Calculate total cost (execution + blob + L1 data fee)
    pub fn total_cost(&self) -> U256 {
        self.execution_gas_cost
            .saturating_add(self.blob_gas_cost)
            .saturating_add(self.l1_data_fee)
    }

    /// Check if this transaction used blob gas (is EIP-4844)
    pub fn has_blob_gas(&self) -> bool {
        self.blob_count.as_usize() > 0
    }

    /// Check if this transaction has L1 data fees (is on an L2)
    pub fn has_l1_data_fee(&self) -> bool {
        self.l1_data_fee > U256::ZERO
    }

    /// Merge another breakdown into this one (for aggregation)
    pub fn merge(&mut self, other: &Self) {
        self.execution_gas_cost = self
            .execution_gas_cost
            .saturating_add(other.execution_gas_cost);
        self.blob_gas_cost = self.blob_gas_cost.saturating_add(other.blob_gas_cost);
        self.l1_data_fee = self.l1_data_fee.saturating_add(other.l1_data_fee);
        // For merged results, blob_count represents total blobs across all txs
        self.blob_count = BlobCount::new(
            self.blob_count
                .as_usize()
                .saturating_add(other.blob_count.as_usize()),
        );
        // blob_gas_price doesn't aggregate meaningfully, keep the last non-zero value
        if other.blob_gas_price.as_u256() > U256::ZERO {
            self.blob_gas_price = other.blob_gas_price;
        }
    }
}

impl Add for GasBreakdown {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let mut result = self;
        result.merge(&rhs);
        result
    }
}

impl std::fmt::Display for GasBreakdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "execution: {} wei", self.execution_gas_cost)?;
        if self.has_blob_gas() {
            write!(
                f,
                ", blob: {} wei ({} @ {})",
                self.blob_gas_cost, self.blob_count, self.blob_gas_price
            )?;
        }
        if self.has_l1_data_fee() {
            write!(f, ", L1 data: {} wei", self.l1_data_fee)?;
        }
        write!(f, ", total: {} wei", self.total_cost())
    }
}

/// Builder for constructing GasBreakdown instances
#[derive(Debug, Default)]
pub struct GasBreakdownBuilder {
    execution_gas_cost: U256,
    blob_gas_cost: U256,
    l1_data_fee: U256,
    blob_count: BlobCount,
    blob_gas_price: BlobGasPrice,
}

impl GasBreakdownBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the execution gas cost
    pub fn execution_gas_cost(mut self, cost: U256) -> Self {
        self.execution_gas_cost = cost;
        self
    }

    /// Set the blob gas cost
    pub fn blob_gas_cost(mut self, cost: U256) -> Self {
        self.blob_gas_cost = cost;
        self
    }

    /// Set the L1 data fee
    pub fn l1_data_fee(mut self, fee: U256) -> Self {
        self.l1_data_fee = fee;
        self
    }

    /// Set the blob count
    pub fn blob_count(mut self, count: BlobCount) -> Self {
        self.blob_count = count;
        self
    }

    /// Set the blob gas price
    pub fn blob_gas_price(mut self, price: BlobGasPrice) -> Self {
        self.blob_gas_price = price;
        self
    }

    /// Build the GasBreakdown
    pub fn build(self) -> GasBreakdown {
        GasBreakdown {
            execution_gas_cost: self.execution_gas_cost,
            blob_gas_cost: self.blob_gas_cost,
            l1_data_fee: self.l1_data_fee,
            blob_count: self.blob_count,
            blob_gas_price: self.blob_gas_price,
        }
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

    #[test]
    fn test_blob_gas_price_creation() {
        let price = BlobGasPrice::new(U256::from(1_000_000_000)); // 1 gwei
        assert_eq!(price.as_u256(), U256::from(1_000_000_000));
    }

    #[test]
    fn test_blob_gas_price_from_gwei() {
        let price = BlobGasPrice::from_gwei(25);
        assert_eq!(price.as_u256(), U256::from(25_000_000_000u64));
    }

    #[test]
    fn test_blob_gas_price_cost_for_gas() {
        let price = BlobGasPrice::from_gwei(1); // 1 gwei
        let blob_gas = U256::from(131_072); // 1 blob worth
        let cost = price.cost_for_gas(blob_gas);
        // 1 gwei * 131072 = 131072 gwei = 131,072,000,000,000 wei
        assert_eq!(cost, U256::from(131_072_000_000_000u64));
    }

    #[test]
    fn test_blob_gas_price_cost_for_blobs() {
        let price = BlobGasPrice::from_gwei(1);
        let cost = price.cost_for_blobs(BlobCount::new(2));
        // 2 blobs = 262144 blob gas, at 1 gwei = 262,144,000,000,000 wei
        assert_eq!(cost, U256::from(262_144_000_000_000u64));
    }

    #[test]
    fn test_blob_gas_price_display() {
        let price = BlobGasPrice::from_gwei(25);
        let display = format!("{price}");
        assert!(display.contains("gwei"));
        assert!(display.contains("blob"));

        let small_price = BlobGasPrice::new(U256::from(100));
        let display = format!("{small_price}");
        assert!(display.contains("wei"));
    }

    #[test]
    fn test_blob_gas_price_serialization() {
        let price = BlobGasPrice::from_gwei(10);
        let json = serde_json::to_string(&price).unwrap();
        let deserialized: BlobGasPrice = serde_json::from_str(&json).unwrap();
        assert_eq!(price, deserialized);
    }

    #[test]
    fn test_gas_breakdown_new() {
        let breakdown = GasBreakdown::new();
        assert_eq!(breakdown.execution_gas_cost, U256::ZERO);
        assert_eq!(breakdown.blob_gas_cost, U256::ZERO);
        assert_eq!(breakdown.l1_data_fee, U256::ZERO);
        assert_eq!(breakdown.blob_count, BlobCount::ZERO);
        assert_eq!(breakdown.total_cost(), U256::ZERO);
    }

    #[test]
    fn test_gas_breakdown_builder() {
        let breakdown = GasBreakdown::builder()
            .execution_gas_cost(U256::from(1_000_000u64))
            .blob_gas_cost(U256::from(500_000u64))
            .l1_data_fee(U256::from(200_000u64))
            .blob_count(BlobCount::new(2))
            .blob_gas_price(BlobGasPrice::from_gwei(1))
            .build();

        assert_eq!(breakdown.execution_gas_cost, U256::from(1_000_000u64));
        assert_eq!(breakdown.blob_gas_cost, U256::from(500_000u64));
        assert_eq!(breakdown.l1_data_fee, U256::from(200_000u64));
        assert_eq!(breakdown.blob_count, BlobCount::new(2));
        assert_eq!(breakdown.total_cost(), U256::from(1_700_000u64));
    }

    #[test]
    fn test_gas_breakdown_has_blob_gas() {
        let without_blobs = GasBreakdown::new();
        assert!(!without_blobs.has_blob_gas());

        let with_blobs = GasBreakdown::builder()
            .blob_count(BlobCount::new(1))
            .build();
        assert!(with_blobs.has_blob_gas());
    }

    #[test]
    fn test_gas_breakdown_has_l1_data_fee() {
        let without_l1 = GasBreakdown::new();
        assert!(!without_l1.has_l1_data_fee());

        let with_l1 = GasBreakdown::builder()
            .l1_data_fee(U256::from(100u64))
            .build();
        assert!(with_l1.has_l1_data_fee());
    }

    #[test]
    fn test_gas_breakdown_merge() {
        let mut breakdown1 = GasBreakdown::builder()
            .execution_gas_cost(U256::from(1_000u64))
            .blob_gas_cost(U256::from(500u64))
            .blob_count(BlobCount::new(1))
            .build();

        let breakdown2 = GasBreakdown::builder()
            .execution_gas_cost(U256::from(2_000u64))
            .blob_gas_cost(U256::from(1_000u64))
            .blob_count(BlobCount::new(2))
            .build();

        breakdown1.merge(&breakdown2);

        assert_eq!(breakdown1.execution_gas_cost, U256::from(3_000u64));
        assert_eq!(breakdown1.blob_gas_cost, U256::from(1_500u64));
        assert_eq!(breakdown1.blob_count, BlobCount::new(3));
    }

    #[test]
    fn test_gas_breakdown_add() {
        let breakdown1 = GasBreakdown::builder()
            .execution_gas_cost(U256::from(1_000u64))
            .build();

        let breakdown2 = GasBreakdown::builder()
            .execution_gas_cost(U256::from(2_000u64))
            .build();

        let result = breakdown1 + breakdown2;
        assert_eq!(result.execution_gas_cost, U256::from(3_000u64));
    }

    #[test]
    fn test_gas_breakdown_display() {
        let breakdown = GasBreakdown::builder()
            .execution_gas_cost(U256::from(1_000_000u64))
            .blob_gas_cost(U256::from(500_000u64))
            .blob_count(BlobCount::new(2))
            .blob_gas_price(BlobGasPrice::from_gwei(1))
            .l1_data_fee(U256::from(200_000u64))
            .build();

        let display = format!("{breakdown}");
        assert!(display.contains("execution"));
        assert!(display.contains("blob"));
        assert!(display.contains("L1 data"));
        assert!(display.contains("total"));
    }

    #[test]
    fn test_gas_breakdown_serialization() {
        let breakdown = GasBreakdown::builder()
            .execution_gas_cost(U256::from(1_000_000u64))
            .blob_gas_cost(U256::from(500_000u64))
            .build();

        let json = serde_json::to_string(&breakdown).unwrap();
        let deserialized: GasBreakdown = serde_json::from_str(&json).unwrap();
        assert_eq!(breakdown, deserialized);
    }
}
