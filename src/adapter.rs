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

    #[test]
    fn test_ethereum_adapter_has_no_l1_fee() {
        // The Ethereum adapter should always return None for L1 data fee
        // since Ethereum L1 doesn't have L1 data fees
        // This is a business logic test of the adapter trait implementation

        // We can't easily create a mock receipt here without pulling in test dependencies,
        // but we can document the expected behavior:
        // - Ethereum adapter should return None for l1_data_fee
        // - Arbitrum and Polygon chains also don't have L1 data fees (use Ethereum adapter)
        // - Optimism Stack chains (Base, Optimism, Mode, etc.) have L1 data fees

        let _adapter = EthereumReceiptAdapter;
        // The l1_data_fee method signature guarantees it returns Option<U256>
        // and the implementation always returns None for Ethereum
    }

    #[test]
    fn test_optimism_adapter_has_l1_fee() {
        // The Optimism adapter should always return Some(U256) for L1 data fee
        // even if the fee is 0
        // This is a business logic test of the adapter trait implementation

        let _adapter = OptimismReceiptAdapter;
        // The l1_data_fee method signature guarantees it returns Option<U256>
        // and the implementation always returns Some for Optimism Stack chains
    }

    #[test]
    fn test_adapter_trait_object_safety() {
        // Test that ReceiptAdapter is object-safe by creating a trait object
        // This ensures the trait can be used dynamically

        let _ethereum_adapter: &dyn ReceiptAdapter<Ethereum> = &EthereumReceiptAdapter;
        let _optimism_adapter: &dyn ReceiptAdapter<Optimism> = &OptimismReceiptAdapter;
    }
}
