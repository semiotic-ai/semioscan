//! Core gas calculation logic

use alloy_eips::Typed2718;
use alloy_network::Network;
use alloy_primitives::{Address, BlockNumber, U256};
use alloy_rpc_types::{Filter, TransactionTrait};
use alloy_sol_types::SolEvent;

use crate::events::definitions::Transfer;
use crate::types::gas::BlobCount;

/// Core gas calculation logic
pub struct GasCalculationCore;

impl GasCalculationCore {
    pub(crate) fn calculate_blob_gas_cost<N: Network>(transaction: &N::TransactionResponse) -> U256
    where
        N::TransactionResponse: TransactionTrait + alloy_provider::network::eip2718::Typed2718,
    {
        if !transaction.is_eip4844() {
            return U256::ZERO;
        }
        let blob_count = BlobCount::new(
            transaction
                .blob_versioned_hashes()
                .map(|hashes| hashes.len())
                .unwrap_or_default(),
        );
        let blob_gas_used = blob_count.to_blob_gas_amount();
        let blob_gas_price = U256::from(transaction.max_fee_per_blob_gas().unwrap_or_default());
        blob_gas_used.as_u256().saturating_mul(blob_gas_price)
    }

    pub(crate) fn calculate_effective_gas_price<N: Network>(
        transaction: &N::TransactionResponse,
        receipt_effective_gas_price: U256,
    ) -> U256
    where
        N::TransactionResponse: TransactionTrait + alloy_provider::network::eip2718::Typed2718,
    {
        if transaction.is_legacy() || transaction.is_eip2930() {
            // Legacy or EIP-2930
            U256::from(transaction.gas_price().unwrap_or_default())
        } else {
            // EIP-1559 or EIP-4844
            receipt_effective_gas_price
        }
    }

    pub(crate) fn create_transfer_filter(
        current_block: BlockNumber,
        to_block: BlockNumber,
        token_address: Address,
        from_address: Address, // topic1
        to_address: Address,   // topic2
    ) -> Filter {
        let transfer_topic_hash = Transfer::SIGNATURE_HASH;
        Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .address(token_address)
            .event_signature(transfer_topic_hash) // This takes B256, not Vec<B256>
            .topic1(from_address)
            .topic2(to_address)
    }
}
