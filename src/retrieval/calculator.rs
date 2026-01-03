// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Combined calculator for gas costs and transfer amounts
//!
//! This module provides [`CombinedCalculator`], which retrieves both gas cost data
//! and transfer amounts for blockchain transactions in a single operation. This is
//! more efficient than separate queries when you need both pieces of information.
//!
//! # Usage
//!
//! Create a calculator with a provider for your target chain:
//!
//! ```ignore
//! use semioscan::retrieval::calculator::CombinedCalculator;
//! use alloy_provider::ProviderBuilder;
//!
//! let provider = ProviderBuilder::new().on_http(rpc_url);
//! let calculator = CombinedCalculator::new(provider);
//! ```
//!
//! For Ethereum-compatible chains (Ethereum, Arbitrum, Polygon, etc.), use
//! [`calculate_combined_data_ethereum`](CombinedCalculator::calculate_combined_data_ethereum):
//!
//! ```ignore
//! let result = calculator.calculate_combined_data_ethereum(
//!     chain,
//!     from_address,
//!     to_address,
//!     token_address,
//!     from_block,
//!     to_block,
//! ).await?;
//! ```
//!
//! For Optimism-based chains with L1 data fees (Optimism, Base, etc.), use
//! [`calculate_combined_data_optimism`](CombinedCalculator::calculate_combined_data_optimism):
//!
//! ```ignore
//! let result = calculator.calculate_combined_data_optimism(
//!     chain,
//!     from_address,
//!     to_address,
//!     token_address,
//!     from_block,
//!     to_block,
//! ).await?;
//! ```
//!
//! The calculator automatically handles:
//! - Rate limiting based on chain-specific configuration
//! - Block range chunking for large queries
//! - Parallel fetching of transaction and receipt data
//! - Error recovery (skips failed transactions and continues)
//!
//! See the `examples/` directory for complete usage examples.

use alloy_chains::NamedChain;
use alloy_network::{Ethereum, Network};
use alloy_primitives::{Address, BlockNumber, TxHash};
use alloy_provider::Provider;
use alloy_rpc_types::{Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use futures::future::join_all;
use op_alloy_network::Optimism;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{error, info, trace};

use crate::config::SemioscanConfig;
use crate::events::definitions::Transfer;
use crate::gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
use crate::tracing::spans;
use crate::types::gas::{GasAmount, GasPrice};

use super::gas_calculation::GasCalculationCore;
use super::types::{CombinedDataResult, GasAndAmountForTx};
use crate::errors::RetrievalError;

/// Log metadata extracted from RpcLog for batch processing.
///
/// Alloy's `RpcLog` contains Optional tx_hash and block_number fields.
/// This struct holds the validated, required fields along with the
/// decoded transfer value, ready for batch RPC fetching.
struct LogBatchEntry {
    tx_hash: TxHash,
    block_number: BlockNumber,
    transfer_value: alloy_primitives::U256,
}

pub struct CombinedCalculator<N: Network, P: Provider<N> + Send + Sync + Clone + 'static>
where
    N::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    N::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    provider: Arc<P>,
    config: SemioscanConfig,
    network_marker: std::marker::PhantomData<N>,
}

impl<N: Network, P: Provider<N> + Send + Sync + Clone + 'static> CombinedCalculator<N, P>
where
    N::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    N::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    /// Create a new combined calculator with default configuration
    pub fn new(provider: P) -> Self {
        Self::with_config(provider, SemioscanConfig::default())
    }

    /// Create a new combined calculator with custom configuration
    pub fn with_config(provider: P, config: SemioscanConfig) -> Self {
        Self {
            provider: Arc::new(provider),
            config,
            network_marker: std::marker::PhantomData,
        }
    }

    /// Batch fetches transaction and receipt data for multiple logs.
    ///
    /// # Performance Optimization
    ///
    /// This method uses parallel fetching via `futures::join_all` to fetch all
    /// transactions and receipts concurrently. When combined with Alloy's
    /// `CallBatchLayer`, these parallel requests can be automatically batched,
    /// reducing network overhead.
    ///
    /// # Arguments
    ///
    /// * `log_entries` - Pre-validated log entries with tx hashes and decoded values
    /// * `adapter` - Network-specific receipt adapter
    ///
    /// # Returns
    ///
    /// A vector of results, each containing either the processed gas/amount data
    /// or an error for that specific transaction.
    async fn batch_fetch_tx_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        log_entries: &[LogBatchEntry],
        adapter: &A,
    ) -> Vec<Result<GasAndAmountForTx, (TxHash, RetrievalError)>> {
        if log_entries.is_empty() {
            return vec![];
        }

        info!(
            count = log_entries.len(),
            "Batch fetching transaction data for logs"
        );

        // Create futures for all transaction and receipt fetches
        let fetch_futures: Vec<_> = log_entries
            .iter()
            .map(|entry| {
                let provider = self.provider.clone();
                let tx_hash = entry.tx_hash;
                let block_number = entry.block_number;
                let transfer_value = entry.transfer_value;

                async move {
                    let span = spans::process_log_for_combined_data(tx_hash);
                    let _guard = span.enter();

                    // Fetch transaction and receipt in parallel
                    let (tx_result, receipt_result) = tokio::join!(
                        provider.get_transaction_by_hash(tx_hash),
                        provider.get_transaction_receipt(tx_hash)
                    );

                    (
                        tx_hash,
                        block_number,
                        transfer_value,
                        tx_result,
                        receipt_result,
                    )
                }
            })
            .collect();

        // Execute all fetches in parallel
        // When CallBatchLayer is enabled, these may be batched together
        let results = join_all(fetch_futures).await;

        // Process results
        results
            .into_iter()
            .map(|(tx_hash, block_number, transfer_value, tx_result, receipt_result)| {
                // Process transaction result
                let transaction = tx_result
                    .map_err(|e| {
                        (
                            tx_hash,
                            RetrievalError::Rpc(crate::errors::RpcError::chain_connection_failed(
                                format!("get_transaction_by_hash({tx_hash})"),
                                e,
                            )),
                        )
                    })?
                    .ok_or_else(|| {
                        (
                            tx_hash,
                            RetrievalError::missing_transaction(&tx_hash.to_string()),
                        )
                    })?;

                // Process receipt result
                let receipt = receipt_result
                    .map_err(|e| {
                        (
                            tx_hash,
                            RetrievalError::Rpc(crate::errors::RpcError::chain_connection_failed(
                                format!("get_transaction_receipt({tx_hash})"),
                                e,
                            )),
                        )
                    })?
                    .ok_or_else(|| {
                        (tx_hash, RetrievalError::missing_receipt(&tx_hash.to_string()))
                    })?;

                // Extract gas data
                let gas_used = adapter.gas_used(&receipt);
                let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);
                let l1_fee = adapter.l1_data_fee(&receipt);

                let effective_gas_price = GasCalculationCore::calculate_effective_gas_price::<N>(
                    &transaction,
                    receipt_effective_gas_price,
                );

                let blob_gas_cost = GasCalculationCore::calculate_blob_gas_cost::<N>(&transaction);

                Ok(GasAndAmountForTx {
                    tx_hash,
                    block_number,
                    gas_used: GasAmount::from(gas_used),
                    effective_gas_price: GasPrice::from(effective_gas_price),
                    l1_fee,
                    transferred_amount: transfer_value,
                    blob_gas_cost,
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_block_range_for_combined_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
        adapter: &A,
    ) -> Result<CombinedDataResult, RetrievalError> {
        let span = spans::process_block_range_for_combined_data(
            chain,
            from_address,
            to_address,
            token_address,
            from_block,
            to_block,
        );
        let _guard = span.enter();

        let mut result = CombinedDataResult::new(chain, from_address, to_address, token_address);
        let mut current_block = from_block;

        // Get config values for this chain
        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        while current_block <= to_block {
            let chunk_end = std::cmp::min(current_block + max_block_range.as_u64() - 1, to_block);

            let filter = GasCalculationCore::create_transfer_filter(
                current_block,
                chunk_end,
                token_address,
                from_address,
                to_address,
            );

            trace!(?filter, current_block, chunk_end, "Fetching logs");
            let logs: Vec<RpcLog> = self.provider.get_logs(&filter).await.map_err(|e| {
                RetrievalError::Rpc(crate::errors::RpcError::get_logs_failed(
                    format!(
                        "get_logs for blocks {}-{} on {:?}",
                        current_block, chunk_end, chain
                    ),
                    e,
                ))
            })?;
            trace!(
                logs_count = logs.len(),
                current_block,
                chunk_end,
                "Fetched logs"
            );

            // First pass: Decode all logs and collect entries for batch fetching
            let mut log_entries = Vec::with_capacity(logs.len());
            for rpc_log_entry in &logs {
                match Transfer::decode_log(&rpc_log_entry.inner) {
                    Ok(transfer_event_data) => {
                        let tx_hash = match rpc_log_entry.transaction_hash {
                            Some(hash) => hash,
                            None => {
                                error!("Missing transaction hash in log entry");
                                continue;
                            }
                        };
                        let block_number = match rpc_log_entry.block_number {
                            Some(num) => num,
                            None => {
                                error!("Missing block number in log entry");
                                continue;
                            }
                        };

                        info!(
                            ?chain, ?from_address, ?to_address, ?token_address,
                            amount = ?transfer_event_data.value,
                            block = block_number,
                            ?tx_hash,
                            "Decoded Transfer event for batch processing"
                        );

                        log_entries.push(LogBatchEntry {
                            tx_hash,
                            block_number,
                            transfer_value: transfer_event_data.value,
                        });
                    }
                    Err(e) => {
                        error!(error = %e, log_data = ?rpc_log_entry.data(), log_topics = ?rpc_log_entry.topics(), "Failed to decode Transfer log. Skipping log.");
                        // Continue with other logs
                    }
                }
            }

            // Second pass: Batch fetch all transaction and receipt data
            let batch_results = self.batch_fetch_tx_data(&log_entries, adapter).await;

            // Process batch results
            for batch_result in batch_results {
                match batch_result {
                    Ok(data) => {
                        result.add_transaction_data(data);
                    }
                    Err((tx_hash, e)) => {
                        error!(error = %e, ?tx_hash, "Error processing log for combined data. Skipping log.");
                        // Continue with other logs
                    }
                }
            }

            current_block = chunk_end + 1;

            // Apply rate limiting if configured for this chain
            if let Some(delay) = rate_limit {
                if current_block <= to_block {
                    trace!(?chain, ?delay, "Applying rate limit delay");
                    sleep(delay).await;
                }
            }
        }
        info!(?chain, %from_address, %to_address, %token_address, from_block, to_block, transactions_found = result.transaction_count.as_usize(), "Finished processing block range");
        Ok(result)
    }

    /// Calculates combined transfer amount and gas cost data.
    /// Caching is not implemented in this version but can be added by adapting GasCostCache logic.
    #[allow(clippy::too_many_arguments)]
    pub async fn calculate_combined_data_with_adapter<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
        adapter: &A,
    ) -> Result<CombinedDataResult, RetrievalError> {
        let span = spans::calculate_combined_data_with_adapter(
            chain,
            from_address,
            to_address,
            token_address,
            from_block,
            to_block,
        );
        let _guard = span.enter();

        let result = self
            .process_block_range_for_combined_data(
                chain,
                from_address,
                to_address,
                token_address,
                from_block,
                to_block,
                adapter,
            )
            .await?;

        Ok(result)
    }
}

// Network-specific public methods

impl<P: Provider<Ethereum> + Send + Sync + Clone + 'static> CombinedCalculator<Ethereum, P>
where
    <Ethereum as Network>::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    <Ethereum as Network>::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    #[allow(clippy::too_many_arguments)]
    pub async fn calculate_combined_data_ethereum(
        &self,
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
    ) -> Result<CombinedDataResult, RetrievalError> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_combined_data_with_adapter(
            chain,
            from_address,
            to_address,
            token_address,
            from_block,
            to_block,
            &adapter,
        )
        .await
    }
}

impl<P: Provider<Optimism> + Send + Sync + Clone + 'static> CombinedCalculator<Optimism, P>
where
    <Optimism as Network>::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    <Optimism as Network>::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    #[allow(clippy::too_many_arguments)]
    pub async fn calculate_combined_data_optimism(
        &self,
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
    ) -> Result<CombinedDataResult, RetrievalError> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_combined_data_with_adapter(
            chain,
            from_address,
            to_address,
            token_address,
            from_block,
            to_block,
            &adapter,
        )
        .await
    }
}
