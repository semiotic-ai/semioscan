// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Receipt adapters for extracting gas data from different network types
//!
//! This module provides network-specific adapters for extracting gas cost information
//! from transaction receipts. Different blockchain networks have different receipt formats
//! and gas calculation rules, particularly regarding L1 data fees on L2 networks.
//!
//! # Network Types
//!
//! - **Ethereum**: L1 chains (Ethereum, Arbitrum, Polygon) with no L1 data fees
//! - **Optimism**: Optimism Stack chains (Base, Optimism, Mode) with L1 data fees
//!
//! # Example: Using the Ethereum adapter
//!
//! ```rust
//! use semioscan::{EthereumReceiptAdapter, ReceiptAdapter};
//! use alloy_network::Ethereum;
//!
//! // Create adapter for Ethereum-like chains (Ethereum, Arbitrum, Polygon)
//! let adapter = EthereumReceiptAdapter;
//!
//! // Adapter extracts gas_used, effective_gas_price, and l1_data_fee from receipts
//! // For Ethereum chains, l1_data_fee is always None
//! ```
//!
//! # Example: Using the Optimism adapter
//!
//! ```rust
//! use semioscan::{OptimismReceiptAdapter, ReceiptAdapter};
//! use op_alloy_network::Optimism;
//!
//! // Create adapter for Optimism Stack chains (Base, Optimism, Mode, Fraxtal)
//! let adapter = OptimismReceiptAdapter;
//!
//! // Adapter extracts gas_used, effective_gas_price, and l1_data_fee from receipts
//! // For Optimism chains, l1_data_fee is Some(U256) representing L1 posting costs
//! ```

use alloy_network::{Ethereum, Network};
use alloy_primitives::U256;
use op_alloy_network::Optimism;

/// Trait for network-specific receipt handling
///
/// Different blockchain networks use different receipt formats and have different
/// gas cost components. This trait abstracts over these differences to provide
/// a uniform interface for extracting gas data.
///
/// # Implementors
///
/// - [`EthereumReceiptAdapter`]: For L1 chains without L1 data fees
/// - [`OptimismReceiptAdapter`]: For Optimism Stack chains with L1 data fees
///
/// # Example: Generic function using ReceiptAdapter
///
/// ```rust,ignore
/// use semioscan::ReceiptAdapter;
/// use alloy_network::Network;
///
/// fn calculate_total_cost<N: Network>(
///     adapter: &impl ReceiptAdapter<N>,
///     receipt: &N::ReceiptResponse
/// ) -> alloy_primitives::U256 {
///     let gas_used = adapter.gas_used(receipt);
///     let price = adapter.effective_gas_price(receipt);
///     let l1_fee = adapter.l1_data_fee(receipt).unwrap_or_default();
///
///     gas_used.saturating_mul(price).saturating_add(l1_fee)
/// }
/// ```
pub trait ReceiptAdapter<N: Network> {
    /// Extract the amount of gas used by a transaction
    ///
    /// # Returns
    ///
    /// The gas consumed by the transaction execution
    fn gas_used(&self, receipt: &N::ReceiptResponse) -> U256;

    /// Extract the effective gas price paid for the transaction
    ///
    /// For EIP-1559 transactions, this is the actual price paid per gas unit,
    /// which may be lower than the max fee per gas.
    ///
    /// # Returns
    ///
    /// The effective gas price in wei per gas unit
    fn effective_gas_price(&self, receipt: &N::ReceiptResponse) -> U256;

    /// Extract the L1 data fee (for L2 chains only)
    ///
    /// On Optimism Stack chains, transactions pay an additional fee to cover
    /// the cost of posting transaction data to the L1 chain (Ethereum).
    ///
    /// # Returns
    ///
    /// - `Some(fee)`: L1 data fee in wei (for Optimism Stack chains)
    /// - `None`: No L1 data fee (for Ethereum, Arbitrum, Polygon)
    fn l1_data_fee(&self, receipt: &N::ReceiptResponse) -> Option<U256>;
}

/// Receipt adapter for Ethereum and Ethereum-like chains
///
/// Use this adapter for chains that don't have L1 data fees:
/// - Ethereum (L1)
/// - Arbitrum (L2 with different fee model)
/// - Polygon (L1)
/// - Avalanche (L1)
/// - BNB Chain (L1)
///
/// # Example
///
/// ```rust
/// use semioscan::{EthereumReceiptAdapter, ReceiptAdapter};
/// use alloy_network::Ethereum;
///
/// let adapter = EthereumReceiptAdapter;
/// // Use adapter with transaction receipts to extract gas data
/// ```
pub struct EthereumReceiptAdapter;

impl ReceiptAdapter<Ethereum> for EthereumReceiptAdapter {
    fn gas_used(&self, receipt: &<Ethereum as Network>::ReceiptResponse) -> U256 {
        U256::from(receipt.gas_used)
    }

    fn effective_gas_price(&self, receipt: &<Ethereum as Network>::ReceiptResponse) -> U256 {
        U256::from(receipt.effective_gas_price)
    }

    fn l1_data_fee(&self, _receipt: &<Ethereum as Network>::ReceiptResponse) -> Option<U256> {
        None // Ethereum L1 as well as chains like Arbitrum and Polygon don't have L1 data fees
    }
}

/// Receipt adapter for Optimism Stack chains
///
/// Use this adapter for chains that have L1 data fees:
/// - Base
/// - Optimism
/// - Mode
/// - Fraxtal
/// - Sonic
///
/// These chains pay an additional L1 data fee to cover the cost of posting
/// transaction data to Ethereum L1.
///
/// # Example
///
/// ```rust
/// use semioscan::{OptimismReceiptAdapter, ReceiptAdapter};
/// use op_alloy_network::Optimism;
///
/// let adapter = OptimismReceiptAdapter;
/// // Use adapter with transaction receipts to extract gas data including L1 fees
/// ```
pub struct OptimismReceiptAdapter;

impl ReceiptAdapter<Optimism> for OptimismReceiptAdapter {
    fn gas_used(&self, receipt: &<Optimism as Network>::ReceiptResponse) -> U256 {
        U256::from(receipt.inner.gas_used)
    }

    fn effective_gas_price(&self, receipt: &<Optimism as Network>::ReceiptResponse) -> U256 {
        U256::from(receipt.inner.effective_gas_price)
    }

    fn l1_data_fee(&self, receipt: &<Optimism as Network>::ReceiptResponse) -> Option<U256> {
        Some(U256::from(receipt.l1_block_info.l1_fee.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an Ethereum receipt with known gas values for testing
    fn create_ethereum_receipt(
        gas_used: u64,
        effective_gas_price: u128,
    ) -> <Ethereum as Network>::ReceiptResponse {
        let json = serde_json::json!({
            "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "blockNumber": "0x1",
            "transactionIndex": "0x0",
            "from": "0x0000000000000000000000000000000000000000",
            "to": "0x0000000000000000000000000000000000000000",
            "cumulativeGasUsed": format!("0x{:x}", gas_used),
            "gasUsed": format!("0x{:x}", gas_used),
            "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "type": "0x2"
        });

        serde_json::from_value(json).expect("Failed to create test Ethereum receipt")
    }

    /// Create an Optimism receipt with known gas values and L1 fee for testing
    fn create_optimism_receipt(
        gas_used: u64,
        effective_gas_price: u128,
        l1_fee: Option<u128>,
    ) -> <Optimism as Network>::ReceiptResponse {
        let json = serde_json::json!({
            "transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "blockNumber": "0x1",
            "transactionIndex": "0x0",
            "from": "0x0000000000000000000000000000000000000000",
            "to": "0x0000000000000000000000000000000000000000",
            "cumulativeGasUsed": format!("0x{:x}", gas_used),
            "gasUsed": format!("0x{:x}", gas_used),
            "effectiveGasPrice": format!("0x{:x}", effective_gas_price),
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "type": "0x2",
            "l1Fee": l1_fee.map(|fee| format!("0x{:x}", fee)),
            "l1GasUsed": "0x0",
            "l1GasPrice": "0x0",
            "l1FeeScalar": "1.0"
        });

        serde_json::from_value(json).expect("Failed to create test Optimism receipt")
    }

    #[test]
    fn ethereum_adapter_extracts_gas_used() {
        let adapter = EthereumReceiptAdapter;
        let receipt = create_ethereum_receipt(50_000, 30_000_000_000);

        let gas_used = adapter.gas_used(&receipt);

        assert_eq!(gas_used, U256::from(50_000));
    }

    #[test]
    fn ethereum_adapter_extracts_effective_gas_price() {
        let adapter = EthereumReceiptAdapter;
        let receipt = create_ethereum_receipt(50_000, 30_000_000_000);

        let price = adapter.effective_gas_price(&receipt);

        assert_eq!(price, U256::from(30_000_000_000_u128));
    }

    #[test]
    fn ethereum_adapter_returns_none_for_l1_fee() {
        let adapter = EthereumReceiptAdapter;
        let receipt = create_ethereum_receipt(50_000, 30_000_000_000);

        let l1_fee = adapter.l1_data_fee(&receipt);

        assert_eq!(l1_fee, None);
    }

    #[test]
    fn optimism_adapter_extracts_gas_used() {
        let adapter = OptimismReceiptAdapter;
        let receipt = create_optimism_receipt(75_000, 20_000_000_000, Some(1_000_000));

        let gas_used = adapter.gas_used(&receipt);

        assert_eq!(gas_used, U256::from(75_000));
    }

    #[test]
    fn optimism_adapter_extracts_effective_gas_price() {
        let adapter = OptimismReceiptAdapter;
        let receipt = create_optimism_receipt(75_000, 20_000_000_000, Some(1_000_000));

        let price = adapter.effective_gas_price(&receipt);

        assert_eq!(price, U256::from(20_000_000_000_u128));
    }

    #[test]
    fn optimism_adapter_extracts_l1_fee_when_present() {
        let adapter = OptimismReceiptAdapter;
        let receipt = create_optimism_receipt(75_000, 20_000_000_000, Some(1_500_000));

        let l1_fee = adapter.l1_data_fee(&receipt);

        assert_eq!(l1_fee, Some(U256::from(1_500_000)));
    }

    #[test]
    fn optimism_adapter_returns_zero_when_l1_fee_is_none() {
        let adapter = OptimismReceiptAdapter;
        let receipt = create_optimism_receipt(75_000, 20_000_000_000, None);

        let l1_fee = adapter.l1_data_fee(&receipt);

        // Implementation returns Some(0) when l1_fee is None in receipt
        assert_eq!(l1_fee, Some(U256::ZERO));
    }

    #[test]
    fn adapter_trait_object_safety() {
        // Verify that ReceiptAdapter can be used as a trait object (dynamic dispatch)
        let _ethereum_adapter: &dyn ReceiptAdapter<Ethereum> = &EthereumReceiptAdapter;
        let _optimism_adapter: &dyn ReceiptAdapter<Optimism> = &OptimismReceiptAdapter;
    }
}
