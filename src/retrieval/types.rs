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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GasAndAmountForTx {
    /// Transaction hash for the enriched transfer.
    pub tx_hash: TxHash,
    /// Block number containing the transaction.
    pub block_number: BlockNumber,
    /// L2 gas used by the transaction.
    pub gas_used: GasAmount,
    /// Effective L2 gas price charged for the transaction.
    pub effective_gas_price: GasPrice,
    /// Optional L1 data fee charged by L2 chains that expose it in the receipt.
    pub l1_fee: Option<U256>,
    /// Additional blob gas cost for EIP-4844 transactions.
    pub blob_gas_cost: U256,
    /// ERC-20 amount transferred by the decoded log this transaction matched.
    pub transferred_amount: U256,
}

impl GasAndAmountForTx {
    /// Calculates the total gas cost for this transaction, including L2 gas, L1 fee, and blob gas.
    #[must_use]
    pub fn total_gas_cost(&self) -> U256 {
        let l2_execution_cost = self.gas_used * self.effective_gas_price;
        let total_cost = l2_execution_cost.saturating_add(self.blob_gas_cost);
        total_cost.saturating_add(self.l1_fee.unwrap_or_default())
    }
}

/// Which follow-up RPC lookup failed while enriching a decoded transfer log.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CombinedDataLookupStage {
    Transaction,
    Receipt,
}

impl CombinedDataLookupStage {
    #[must_use]
    pub const fn operation_name(self) -> &'static str {
        match self {
            Self::Transaction => "get_transaction_by_hash",
            Self::Receipt => "get_transaction_receipt",
        }
    }
}

/// Which pass produced a lookup error while enriching a decoded transfer log.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CombinedDataLookupPass {
    Batch,
    SerialFallback,
}

/// Diagnostic details for one failed tx/receipt lookup attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedDataLookupAttempt {
    pub pass: CombinedDataLookupPass,
    pub stage: CombinedDataLookupStage,
    pub error: String,
    pub error_chain: Vec<String>,
    pub transport_error: Option<String>,
}

/// Structured metadata describing a decoded transfer that could not be fully enriched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedDataLookupFailure {
    pub tx_hash: TxHash,
    pub block_number: BlockNumber,
    pub transfer_value: U256,
    pub attempts: Vec<CombinedDataLookupAttempt>,
}

impl CombinedDataLookupFailure {
    #[must_use]
    pub fn final_attempt(&self) -> Option<&CombinedDataLookupAttempt> {
        self.attempts.last()
    }
}

/// Retrieval metadata for combined data calculations.
///
/// This exposes whether decoded transfers had to be skipped after bounded retry/fallback logic,
/// so callers can reject partial accounting results instead of silently persisting them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedDataRetrievalMetadata {
    pub skipped_logs: usize,
    pub fallback_attempts: usize,
    pub fallback_recovered: usize,
    pub partial_failures: Vec<CombinedDataLookupFailure>,
}

impl CombinedDataRetrievalMetadata {
    #[must_use]
    pub fn has_partial_failures(&self) -> bool {
        !self.partial_failures.is_empty()
    }

    #[must_use]
    pub fn skipped_tx_hashes(&self) -> Vec<TxHash> {
        self.partial_failures
            .iter()
            .map(|failure| failure.tx_hash)
            .collect()
    }

    pub fn record_fallback_attempts(&mut self, attempts: usize) {
        self.fallback_attempts += attempts;
    }

    pub fn record_fallback_recovery(&mut self) {
        self.fallback_recovered += 1;
    }

    pub fn record_partial_failure(&mut self, failure: CombinedDataLookupFailure) {
        self.skipped_logs += 1;
        self.partial_failures.push(failure);
    }

    pub fn merge(&mut self, other: &CombinedDataRetrievalMetadata) {
        self.skipped_logs += other.skipped_logs;
        self.fallback_attempts += other.fallback_attempts;
        self.fallback_recovered += other.fallback_recovered;
        self.partial_failures
            .extend(other.partial_failures.iter().cloned());
    }
}

/// Aggregated result for combined data retrieval over a block range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[serde(default)]
    pub retrieval_metadata: CombinedDataRetrievalMetadata,
}

impl CombinedDataResult {
    #[must_use]
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
            retrieval_metadata: CombinedDataRetrievalMetadata::default(),
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
            .extend(other.transactions_data.iter().cloned());
        self.retrieval_metadata.merge(&other.retrieval_metadata);
    }

    #[must_use]
    pub fn is_partial(&self) -> bool {
        self.retrieval_metadata.has_partial_failures()
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

    #[test]
    fn test_combined_result_is_partial_when_metadata_has_failures() {
        let mut result = CombinedDataResult::new(
            NamedChain::Mainnet,
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
        );

        result
            .retrieval_metadata
            .record_partial_failure(CombinedDataLookupFailure {
                tx_hash: TxHash::repeat_byte(0x11),
                block_number: 123,
                transfer_value: U256::from(42_u64),
                attempts: vec![CombinedDataLookupAttempt {
                    pass: CombinedDataLookupPass::Batch,
                    stage: CombinedDataLookupStage::Transaction,
                    error: "RPC error".to_string(),
                    error_chain: vec!["RPC error".to_string(), "inner transport".to_string()],
                    transport_error: Some("inner transport".to_string()),
                }],
            });

        assert!(result.is_partial());
        assert!(result.retrieval_metadata.has_partial_failures());
        assert_eq!(
            result.retrieval_metadata.skipped_tx_hashes(),
            vec![TxHash::repeat_byte(0x11)]
        );
    }

    #[test]
    fn test_metadata_has_partial_failures_tracks_failure_entries_not_skip_counter() {
        let metadata = CombinedDataRetrievalMetadata {
            skipped_logs: 0,
            fallback_attempts: 0,
            fallback_recovered: 0,
            partial_failures: vec![CombinedDataLookupFailure {
                tx_hash: TxHash::repeat_byte(0x22),
                block_number: 456,
                transfer_value: U256::from(7_u64),
                attempts: vec![CombinedDataLookupAttempt {
                    pass: CombinedDataLookupPass::Batch,
                    stage: CombinedDataLookupStage::Receipt,
                    error: "missing receipt".to_string(),
                    error_chain: vec!["missing receipt".to_string()],
                    transport_error: None,
                }],
            }],
        };

        assert!(metadata.has_partial_failures());
    }

    #[test]
    fn test_combined_result_merge_includes_retrieval_metadata() {
        let mut left = CombinedDataResult::new(
            NamedChain::Mainnet,
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
        );
        let mut right = CombinedDataResult::new(
            NamedChain::Mainnet,
            Address::ZERO,
            Address::ZERO,
            Address::ZERO,
        );

        left.retrieval_metadata.fallback_attempts = 1;
        left.retrieval_metadata.fallback_recovered = 1;
        right
            .retrieval_metadata
            .record_partial_failure(CombinedDataLookupFailure {
                tx_hash: TxHash::repeat_byte(0x22),
                block_number: 456,
                transfer_value: U256::from(99_u64),
                attempts: vec![CombinedDataLookupAttempt {
                    pass: CombinedDataLookupPass::SerialFallback,
                    stage: CombinedDataLookupStage::Receipt,
                    error: "missing receipt".to_string(),
                    error_chain: vec!["missing receipt".to_string()],
                    transport_error: None,
                }],
            });

        left.merge(&right);

        assert_eq!(left.retrieval_metadata.fallback_attempts, 1);
        assert_eq!(left.retrieval_metadata.fallback_recovered, 1);
        assert_eq!(left.retrieval_metadata.skipped_logs, 1);
        assert_eq!(left.retrieval_metadata.partial_failures.len(), 1);
        assert_eq!(
            left.retrieval_metadata.skipped_tx_hashes(),
            vec![TxHash::repeat_byte(0x22)]
        );
    }
}
