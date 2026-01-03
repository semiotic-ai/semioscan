// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! EIP-4844 Blob Gas Utilities
//!
//! This module provides utilities for working with EIP-4844 blob transactions,
//! including fetching blob base fees and estimating blob transaction costs.
//!
//! # Overview
//!
//! EIP-4844 introduced "blob-carrying transactions" that store data in a separate
//! blob space with its own pricing mechanism. The blob base fee follows a similar
//! mechanism to the EIP-1559 base fee, adjusting based on blob space utilization.
//!
//! # Using with Alloy's BlobGasFiller
//!
//! For transaction building, Alloy's `BlobGasFiller` (included in `RecommendedFillers`)
//! automatically fills the `max_fee_per_blob_gas` field:
//!
//! ```rust,ignore
//! use alloy_provider::ProviderBuilder;
//!
//! // RecommendedFillers includes BlobGasFiller for EIP-4844 support
//! let provider = ProviderBuilder::new()
//!     .with_recommended_fillers()
//!     .connect_http(url);
//!
//! // When sending a blob transaction, max_fee_per_blob_gas is auto-filled
//! let tx = TransactionRequest::default()
//!     .with_blob_sidecar(sidecar);
//! provider.send_transaction(tx).await?;
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use semioscan::gas::blob::{get_blob_base_fee, estimate_blob_cost};
//! use semioscan::BlobCount;
//!
//! // Get current blob base fee from latest block
//! let blob_base_fee = get_blob_base_fee(&provider).await?;
//! println!("Current blob base fee: {} wei", blob_base_fee);
//!
//! // Estimate cost for 3 blobs
//! let cost = estimate_blob_cost(&provider, BlobCount::new(3)).await?;
//! println!("Estimated cost for 3 blobs: {} wei", cost);
//! ```

use alloy_consensus::BlockHeader;
use alloy_eips::eip4844::{DATA_GAS_PER_BLOB, MAX_BLOBS_PER_BLOCK_DENCUN};
use alloy_eips::eip7840::BlobParams;
use alloy_network::{BlockResponse, Network};
use alloy_primitives::U256;
use alloy_provider::Provider;
use alloy_rpc_types::BlockNumberOrTag;

use crate::errors::RpcError;
use crate::types::gas::{BlobCount, BlobGasPrice};

/// Get the blob base fee from the latest block.
///
/// The blob base fee is extracted from the block header's `excess_blob_gas` field
/// and calculated according to EIP-4844 pricing rules.
///
/// # Arguments
///
/// * `provider` - An Alloy provider connected to an Ethereum node
///
/// # Returns
///
/// The current blob base fee in wei, or an error if the block cannot be fetched
/// or doesn't contain blob gas information.
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::gas::blob::get_blob_base_fee;
///
/// let blob_base_fee = get_blob_base_fee(&provider).await?;
/// println!("Blob base fee: {} wei", blob_base_fee);
/// ```
pub async fn get_blob_base_fee<N, P>(provider: &P) -> Result<BlobGasPrice, RpcError>
where
    N: Network,
    P: Provider<N>,
{
    // Get latest block number first, then fetch the block
    let latest_block_number = provider
        .get_block_number()
        .await
        .map_err(RpcError::get_block_number_failed)?;

    let block = provider
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await
        .map_err(|e| RpcError::get_block_failed(latest_block_number, e))?
        .ok_or_else(|| RpcError::BlockNotFound {
            block_number: latest_block_number,
        })?;

    // Extract blob base fee from the block header
    // The blob_fee() method is available on block headers post-Dencun
    // Uses Cancun (Dencun) parameters for blob gas pricing
    let blob_base_fee = block.header().blob_fee(BlobParams::cancun()).unwrap_or(0);

    Ok(BlobGasPrice::from(blob_base_fee))
}

/// Get the blob base fee from a specific block.
///
/// # Arguments
///
/// * `provider` - An Alloy provider connected to an Ethereum node
/// * `block_number` - The block number to fetch blob base fee from
///
/// # Returns
///
/// The blob base fee at the specified block in wei.
pub async fn get_blob_base_fee_at_block<N, P>(
    provider: &P,
    block_number: u64,
) -> Result<BlobGasPrice, RpcError>
where
    N: Network,
    P: Provider<N>,
{
    let block = provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await
        .map_err(|e| RpcError::get_block_failed(block_number, e))?
        .ok_or_else(|| RpcError::BlockNotFound { block_number })?;

    let blob_base_fee = block.header().blob_fee(BlobParams::cancun()).unwrap_or(0);

    Ok(BlobGasPrice::from(blob_base_fee))
}

/// Estimate the cost of including blobs in a transaction.
///
/// This function fetches the current blob base fee and calculates the
/// total blob gas cost for the specified number of blobs.
///
/// # Arguments
///
/// * `provider` - An Alloy provider connected to an Ethereum node
/// * `blob_count` - The number of blobs (1-6, per EIP-4844 limits)
///
/// # Returns
///
/// The estimated blob gas cost in wei.
///
/// # Example
///
/// ```rust,ignore
/// use semioscan::{BlobCount, gas::blob::estimate_blob_cost};
///
/// // Estimate cost for 2 blobs
/// let cost = estimate_blob_cost(&provider, BlobCount::new(2)).await?;
/// println!("Estimated cost: {} wei", cost);
/// ```
pub async fn estimate_blob_cost<N, P>(provider: &P, blob_count: BlobCount) -> Result<U256, RpcError>
where
    N: Network,
    P: Provider<N>,
{
    let blob_base_fee = get_blob_base_fee(provider).await?;
    Ok(blob_base_fee.cost_for_blobs(blob_count))
}

/// Calculate blob gas for a given blob count.
///
/// Each blob requires `DATA_GAS_PER_BLOB` (131,072) gas units.
/// This is a pure function that doesn't require network access.
///
/// # Arguments
///
/// * `blob_count` - The number of blobs
///
/// # Returns
///
/// The total blob gas required.
///
/// # Example
///
/// ```rust
/// use semioscan::{BlobCount, gas::blob::calculate_blob_gas};
///
/// let gas = calculate_blob_gas(BlobCount::new(3));
/// assert_eq!(gas, 393_216); // 3 * 131_072
/// ```
pub const fn calculate_blob_gas(blob_count: BlobCount) -> u64 {
    blob_count.as_usize() as u64 * DATA_GAS_PER_BLOB
}

/// Get the maximum blob gas per block (Dencun upgrade).
///
/// Returns the maximum blob gas that can be used in a single block,
/// which is `MAX_BLOBS_PER_BLOCK_DENCUN` * `DATA_GAS_PER_BLOB`.
///
/// # Returns
///
/// The maximum blob gas per block (786,432 gas for 6 blobs).
pub const fn max_blob_gas_per_block() -> u64 {
    MAX_BLOBS_PER_BLOCK_DENCUN as u64 * DATA_GAS_PER_BLOB
}

/// Estimate total transaction cost including execution and blob gas.
///
/// This combines execution gas cost estimation with blob gas cost for
/// a complete picture of EIP-4844 transaction costs.
///
/// # Arguments
///
/// * `execution_gas` - Estimated execution gas (e.g., from `eth_estimateGas`)
/// * `gas_price` - Execution gas price in wei
/// * `blob_count` - Number of blobs in the transaction
/// * `blob_gas_price` - Blob gas price in wei
///
/// # Returns
///
/// Total estimated transaction cost in wei.
///
/// # Example
///
/// ```rust
/// use alloy_primitives::U256;
/// use semioscan::{BlobCount, BlobGasPrice, gas::blob::estimate_total_tx_cost};
///
/// let total = estimate_total_tx_cost(
///     21_000,                           // execution gas
///     U256::from(30_000_000_000u64),    // 30 gwei gas price
///     BlobCount::new(2),                // 2 blobs
///     BlobGasPrice::from_gwei(1),       // 1 gwei blob gas price
/// );
/// // execution: 21000 * 30 gwei = 630,000 gwei
/// // blob: 2 * 131072 * 1 gwei = 262,144 gwei
/// // total: 892,144 gwei
/// ```
pub fn estimate_total_tx_cost(
    execution_gas: u64,
    gas_price: U256,
    blob_count: BlobCount,
    blob_gas_price: BlobGasPrice,
) -> U256 {
    let execution_cost = U256::from(execution_gas).saturating_mul(gas_price);
    let blob_cost = blob_gas_price.cost_for_blobs(blob_count);
    execution_cost.saturating_add(blob_cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_blob_gas() {
        assert_eq!(calculate_blob_gas(BlobCount::new(0)), 0);
        assert_eq!(calculate_blob_gas(BlobCount::new(1)), 131_072);
        assert_eq!(calculate_blob_gas(BlobCount::new(2)), 262_144);
        assert_eq!(calculate_blob_gas(BlobCount::new(6)), 786_432);
    }

    #[test]
    fn test_max_blob_gas_per_block() {
        assert_eq!(max_blob_gas_per_block(), 786_432);
    }

    #[test]
    fn test_estimate_total_tx_cost() {
        let execution_gas = 21_000u64;
        let gas_price = U256::from(30_000_000_000u64); // 30 gwei
        let blob_count = BlobCount::new(2);
        let blob_gas_price = BlobGasPrice::from_gwei(1);

        let total = estimate_total_tx_cost(execution_gas, gas_price, blob_count, blob_gas_price);

        // execution: 21000 * 30 gwei = 630,000,000,000,000 wei
        // blob: 2 * 131072 * 1 gwei = 262,144,000,000,000 wei
        // total: 892,144,000,000,000 wei
        let expected_execution = U256::from(630_000_000_000_000u64);
        let expected_blob = U256::from(262_144_000_000_000u64);
        assert_eq!(total, expected_execution + expected_blob);
    }

    #[test]
    fn test_estimate_total_tx_cost_no_blobs() {
        let total = estimate_total_tx_cost(
            21_000,
            U256::from(30_000_000_000u64),
            BlobCount::ZERO,
            BlobGasPrice::ZERO,
        );

        // Only execution cost
        assert_eq!(total, U256::from(630_000_000_000_000u64));
    }
}
