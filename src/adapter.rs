//! This module contains the trait and implementations for the receipt adapter.
//! It is used to calculate the gas cost of an event.

use alloy_network::{Ethereum, Network};
use alloy_primitives::U256;
use op_alloy_network::Optimism;

/// Trait for network-specific receipt handling
pub trait ReceiptAdapter<N: Network> {
    fn gas_used(&self, receipt: &N::ReceiptResponse) -> U256;
    fn effective_gas_price(&self, receipt: &N::ReceiptResponse) -> U256;
    fn l1_data_fee(&self, receipt: &N::ReceiptResponse) -> Option<U256>;
}

/// Ethereum receipt adapter
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

/// Optimism receipt adapter
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
