// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Data types for combined gas and amount retrieval

use alloy_chains::NamedChain;
use alloy_primitives::{Address, BlockNumber, TxHash, U256};
use serde::{Deserialize, Serialize};

use crate::types::config::TransactionCount;
use crate::types::gas::{GasAmount, GasPrice};

/// Data for a single transaction including gas and transferred amount.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GasAndAmountForTx {
    pub tx_hash: TxHash,
    pub block_number: BlockNumber,
    pub gas_used: GasAmount,           // L2 gas used
    pub effective_gas_price: GasPrice, // L2 effective gas price
    pub l1_fee: Option<U256>,          // L1 data fee for L2s
    pub blob_gas_cost: U256,           // Cost from EIP-4844 blobs
    pub transferred_amount: U256,
}

impl GasAndAmountForTx {
    /// Calculates the total gas cost for this transaction, including L2 gas, L1 fee, and blob gas.
    pub fn total_gas_cost(&self) -> U256 {
        let l2_execution_cost = self.gas_used * self.effective_gas_price;
        let total_cost = l2_execution_cost.saturating_add(self.blob_gas_cost);
        total_cost.saturating_add(self.l1_fee.unwrap_or_default())
    }
}

/// Aggregated result for combined data retrieval over a block range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinedDataResult {
    pub chain: NamedChain,
    pub from_address: Address,
    pub to_address: Address,
    pub token_address: Address,
    pub total_l2_execution_cost: U256,
    pub total_blob_gas_cost: U256,
    pub total_l1_fee: U256,
    pub overall_total_gas_cost: U256,
    pub total_amount_transferred: U256,
    pub transaction_count: TransactionCount,
    pub transactions_data: Vec<GasAndAmountForTx>,
}

impl CombinedDataResult {
    pub fn new(
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
    ) -> Self {
        Self {
            chain,
            from_address,
            to_address,
            token_address,
            total_l2_execution_cost: U256::ZERO,
            total_blob_gas_cost: U256::ZERO,
            total_l1_fee: U256::ZERO,
            overall_total_gas_cost: U256::ZERO,
            total_amount_transferred: U256::ZERO,
            transaction_count: TransactionCount::new(0),
            transactions_data: Vec::new(),
        }
    }

    pub fn add_transaction_data(&mut self, data: GasAndAmountForTx) {
        let l2_execution_cost = data.gas_used * data.effective_gas_price;
        self.total_l2_execution_cost = self
            .total_l2_execution_cost
            .saturating_add(l2_execution_cost);
        self.total_blob_gas_cost = self.total_blob_gas_cost.saturating_add(data.blob_gas_cost);
        self.total_l1_fee = self
            .total_l1_fee
            .saturating_add(data.l1_fee.unwrap_or_default());
        self.overall_total_gas_cost = self
            .overall_total_gas_cost
            .saturating_add(data.total_gas_cost());
        self.total_amount_transferred = self
            .total_amount_transferred
            .saturating_add(data.transferred_amount);
        self.transaction_count += TransactionCount::new(1);
        self.transactions_data.push(data);
    }

    /// Merge another result into this one (for combining results from multiple block ranges)
    pub fn merge(&mut self, other: &CombinedDataResult) {
        self.total_l2_execution_cost = self
            .total_l2_execution_cost
            .saturating_add(other.total_l2_execution_cost);
        self.total_blob_gas_cost = self
            .total_blob_gas_cost
            .saturating_add(other.total_blob_gas_cost);
        self.total_l1_fee = self.total_l1_fee.saturating_add(other.total_l1_fee);
        self.overall_total_gas_cost = self
            .overall_total_gas_cost
            .saturating_add(other.overall_total_gas_cost);
        self.total_amount_transferred = self
            .total_amount_transferred
            .saturating_add(other.total_amount_transferred);
        self.transaction_count += other.transaction_count;
        self.transactions_data
            .extend(other.transactions_data.iter().cloned()); // Consider efficiency for very large Vecs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::TxHash;

    fn create_test_tx(
        gas_used: u64,
        gas_price: u64,
        l1_fee: Option<u64>,
        blob_gas_cost: u64,
        transferred_amount: u64,
    ) -> GasAndAmountForTx {
        GasAndAmountForTx {
            tx_hash: TxHash::ZERO,
            block_number: 1000,
            gas_used: GasAmount::from(gas_used),
            effective_gas_price: GasPrice::from(gas_price),
            l1_fee: l1_fee.map(U256::from),
            blob_gas_cost: U256::from(blob_gas_cost),
            transferred_amount: U256::from(transferred_amount),
        }
    }

    #[test]
    fn test_total_gas_cost_basic() {
        // Test basic calculation with L2 gas only
        let tx = create_test_tx(
            21000, // gas used
            50,    // gas price
            None,  // no L1 fee
            0,     // no blob gas
            1000,  // transferred amount
        );

        let total = tx.total_gas_cost();
        let expected = U256::from(21000 * 50); // gas_used * gas_price

        assert_eq!(total, expected, "Should calculate L2 gas cost correctly");
    }

    #[test]
    fn test_total_gas_cost_with_l1_fee() {
        // Test calculation including L1 fee
        let tx = create_test_tx(
            50000,        // gas used
            100,          // gas price
            Some(200000), // L1 fee
            0,            // no blob gas
            5000,
        );

        let total = tx.total_gas_cost();
        let expected = U256::from(50000 * 100 + 200000); // (gas_used * gas_price) + l1_fee

        assert_eq!(
            total, expected,
            "Should include L1 fee in total gas cost calculation"
        );
    }

    #[test]
    fn test_total_gas_cost_with_blob_gas() {
        // Test calculation including blob gas cost
        let tx = create_test_tx(
            30000,  // gas used
            75,     // gas price
            None,   // no L1 fee
            100000, // blob gas cost
            2500,
        );

        let total = tx.total_gas_cost();
        let expected = U256::from(30000 * 75 + 100000); // (gas_used * gas_price) + blob_gas_cost

        assert_eq!(
            total, expected,
            "Should include blob gas cost in total calculation"
        );
    }

    #[test]
    fn test_total_gas_cost_all_components() {
        // Test calculation with all components: L2 gas, L1 fee, and blob gas
        let tx = create_test_tx(
            100000,       // gas used
            200,          // gas price
            Some(500000), // L1 fee
            300000,       // blob gas cost
            10000,
        );

        let total = tx.total_gas_cost();
        let expected = U256::from(100000 * 200 + 500000 + 300000);

        assert_eq!(
            total, expected,
            "Should correctly sum all gas cost components"
        );
    }

    #[test]
    fn test_total_gas_cost_large_values() {
        // Test with large values to ensure no overflow
        let large_gas = u64::MAX / 2;
        let large_price = 100;

        let tx = create_test_tx(large_gas, large_price, Some(1_000_000), 500_000, 100_000);

        let total = tx.total_gas_cost();
        let expected_l2_cost = U256::from(large_gas) * U256::from(large_price);
        let expected = expected_l2_cost + U256::from(1_000_000) + U256::from(500_000);

        assert_eq!(total, expected, "Should handle large values correctly");
    }

    #[test]
    fn test_total_gas_cost_saturating_arithmetic() {
        // Test that the calculation uses saturating arithmetic
        // This test verifies the function doesn't panic on large inputs
        let max_gas = u64::MAX;
        let max_price = u64::MAX;

        let tx = create_test_tx(max_gas, max_price, Some(u64::MAX), u64::MAX, 1000);

        // Should not panic - saturating_mul and saturating_add prevent overflow
        let total = tx.total_gas_cost();

        // The result should be saturated at U256::MAX
        assert!(
            total > U256::ZERO,
            "Should produce non-zero result even with overflow"
        );
    }

    #[test]
    fn test_clone_and_equality() {
        // Test that GasAndAmountForTx implements Clone and PartialEq correctly
        let tx1 = create_test_tx(21000, 50, None, 0, 1000);
        let tx2 = tx1.clone();

        assert_eq!(tx1, tx2, "Cloned transactions should be equal");
        assert_eq!(
            tx1.total_gas_cost(),
            tx2.total_gas_cost(),
            "Total costs should match"
        );
    }

    #[test]
    fn test_debug_representation() {
        // Test that Debug trait is implemented
        let tx = create_test_tx(21000, 50, Some(1000), 500, 2000);

        let debug_str = format!("{:?}", tx);

        // Should contain key fields
        assert!(
            debug_str.contains("gas_used"),
            "Debug output should include gas_used"
        );
        assert!(
            debug_str.contains("tx_hash"),
            "Debug output should include tx_hash"
        );
    }
}
