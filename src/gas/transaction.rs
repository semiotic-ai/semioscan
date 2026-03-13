// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

use alloy_eips::Typed2718;
use alloy_primitives::U256;
use alloy_rpc_types::TransactionTrait;

use crate::types::gas::BlobCount;

#[must_use]
pub(crate) fn gas_price_override<T>(transaction: &T) -> Option<U256>
where
    T: TransactionTrait + Typed2718,
{
    if transaction.is_legacy() || transaction.is_eip2930() {
        Some(U256::from(transaction.gas_price().unwrap_or_default()))
    } else {
        None
    }
}

#[must_use]
pub(crate) fn effective_gas_price<T>(transaction: &T, receipt_effective_gas_price: U256) -> U256
where
    T: TransactionTrait + Typed2718,
{
    gas_price_override(transaction).unwrap_or(receipt_effective_gas_price)
}

#[must_use]
pub(crate) fn blob_gas_cost<T>(transaction: &T) -> U256
where
    T: TransactionTrait + Typed2718,
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
