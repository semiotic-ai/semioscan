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
//! - Bounded serial fallback plus explicit partial-failure metadata when enrichment still fails
//!
//! See the `examples/` directory for complete usage examples.

use alloy_chains::NamedChain;
use alloy_eips::Typed2718;
use alloy_network::{AnyRpcTransaction, Ethereum, Network};
use alloy_primitives::{Address, BlockNumber, TxHash};
use alloy_provider::Provider;
use alloy_rpc_types::{Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use alloy_transport::TransportError;
use futures::future::join_all;
use op_alloy_network::Optimism;
use std::{borrow::Cow, error::Error as StdError, sync::Arc};
use tokio::time::sleep;
use tracing::{error, info, trace, warn, Instrument};

use crate::config::SemioscanConfig;
use crate::events::definitions::Transfer;
use crate::gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
use crate::tracing::spans;
use crate::types::gas::{GasAmount, GasPrice};

use super::gas_calculation::GasCalculationCore;
use super::types::{
    CombinedDataLookupAttempt, CombinedDataLookupFailure, CombinedDataLookupPass,
    CombinedDataLookupStage, CombinedDataResult, GasAndAmountForTx,
};
use crate::errors::RetrievalError;

/// Log metadata extracted from RpcLog for batch processing.
///
/// Alloy's `RpcLog` contains Optional tx_hash and block_number fields.
/// This struct holds the validated, required fields along with the
/// decoded transfer value, ready for batch RPC fetching.
#[derive(Debug, Clone, Copy)]
struct LogBatchEntry {
    tx_hash: TxHash,
    block_number: BlockNumber,
    transfer_value: alloy_primitives::U256,
}

#[derive(Debug, Clone, Copy)]
struct TransactionGasData {
    gas_price_override: Option<alloy_primitives::U256>,
    blob_gas_cost: alloy_primitives::U256,
}

impl TransactionGasData {
    fn from_transaction<T>(transaction: &T) -> Self
    where
        T: TransactionTrait + alloy_provider::network::eip2718::Typed2718,
    {
        Self {
            gas_price_override: GasCalculationCore::gas_price_override(transaction),
            blob_gas_cost: GasCalculationCore::calculate_blob_gas_cost(transaction),
        }
    }

    fn effective_gas_price(
        self,
        receipt_effective_gas_price: alloy_primitives::U256,
    ) -> alloy_primitives::U256 {
        self.gas_price_override
            .unwrap_or(receipt_effective_gas_price)
    }
}

fn collect_error_chain(error: &(dyn StdError + 'static)) -> Vec<String> {
    let mut chain = vec![error.to_string()];
    let mut source = error.source();

    while let Some(err) = source {
        chain.push(err.to_string());
        source = err.source();
    }

    chain
}

#[allow(deprecated)]
fn transport_error_string(error: &RetrievalError) -> Option<String> {
    match error {
        RetrievalError::Rpc(crate::errors::RpcError::GetLogsFailed { source, .. })
        | RetrievalError::Rpc(crate::errors::RpcError::ChainConnectionFailed { source, .. })
        | RetrievalError::Rpc(crate::errors::RpcError::RequestFailed { source, .. })
        | RetrievalError::Rpc(crate::errors::RpcError::GetBlockNumberFailed { source })
        | RetrievalError::Rpc(crate::errors::RpcError::GetBlockFailed { source, .. }) => {
            Some(source.to_string())
        }
        _ => None,
    }
}

fn build_lookup_attempt(
    pass: CombinedDataLookupPass,
    stage: CombinedDataLookupStage,
    error: &RetrievalError,
) -> CombinedDataLookupAttempt {
    CombinedDataLookupAttempt {
        pass,
        stage,
        error: error.to_string(),
        error_chain: collect_error_chain(error),
        transport_error: transport_error_string(error),
    }
}

fn build_lookup_failure(
    entry: LogBatchEntry,
    pass: CombinedDataLookupPass,
    stage: CombinedDataLookupStage,
    error: RetrievalError,
) -> CombinedDataLookupFailure {
    CombinedDataLookupFailure {
        tx_hash: entry.tx_hash,
        block_number: entry.block_number,
        transfer_value: entry.transfer_value,
        attempts: vec![build_lookup_attempt(pass, stage, &error)],
    }
}

fn log_combined_data_skip(
    failure: &CombinedDataLookupFailure,
    chain: NamedChain,
    from_address: Address,
    to_address: Address,
    token_address: Address,
    from_block: BlockNumber,
    to_block: BlockNumber,
) {
    let fallback_attempts = failure
        .attempts
        .iter()
        .filter(|attempt| attempt.pass == CombinedDataLookupPass::SerialFallback)
        .count();
    if let Some(final_attempt) = failure.final_attempt() {
        error!(
            ?chain,
            %from_address,
            %to_address,
            %token_address,
            from_block,
            to_block,
            ?failure.tx_hash,
            block_number = failure.block_number,
            transfer_value = ?failure.transfer_value,
            lookup_stage = ?final_attempt.stage,
            attempt_count = failure.attempts.len(),
            fallback_attempts,
            error = %final_attempt.error,
            error_chain = ?final_attempt.error_chain,
            transport_error = ?final_attempt.transport_error,
            attempt_history = ?failure.attempts,
            "Error processing decoded transfer for combined data. Skipping transfer and marking result partial."
        );
    }
}

fn lookup_request_failed(
    tx_hash: TxHash,
    stage: CombinedDataLookupStage,
    error: TransportError,
) -> RetrievalError {
    RetrievalError::Rpc(crate::errors::RpcError::request_failed(
        format!("{operation}({tx_hash})", operation = stage.operation_name()),
        error,
    ))
}

fn should_attempt_permissive_tx_decode(chain: NamedChain, error: &TransportError) -> bool {
    // The observed zkSync incident shape is an Alloy deserialization error
    // (`missing field accessList`), so match the structured error variant
    // instead of a brittle rendered-string substring.
    matches!(chain, NamedChain::ZkSync | NamedChain::ZkSyncTestnet) && error.is_deser_error()
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

    fn process_lookup_results<A: ReceiptAdapter<N> + Send + Sync>(
        entry: LogBatchEntry,
        tx_result: Result<Option<TransactionGasData>, CombinedDataLookupFailure>,
        receipt_result: Result<Option<N::ReceiptResponse>, TransportError>,
        pass: CombinedDataLookupPass,
        adapter: &A,
    ) -> Result<GasAndAmountForTx, CombinedDataLookupFailure> {
        let tx_hash = entry.tx_hash;

        let transaction = tx_result?.ok_or_else(|| {
            build_lookup_failure(
                entry,
                pass,
                CombinedDataLookupStage::Transaction,
                RetrievalError::missing_transaction(&tx_hash.to_string()),
            )
        })?;

        let receipt = receipt_result
            .map_err(|error| {
                build_lookup_failure(
                    entry,
                    pass,
                    CombinedDataLookupStage::Receipt,
                    lookup_request_failed(tx_hash, CombinedDataLookupStage::Receipt, error),
                )
            })?
            .ok_or_else(|| {
                build_lookup_failure(
                    entry,
                    pass,
                    CombinedDataLookupStage::Receipt,
                    RetrievalError::missing_receipt(&tx_hash.to_string()),
                )
            })?;

        let gas_used = adapter.gas_used(&receipt);
        let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);
        let l1_fee = adapter.l1_data_fee(&receipt);

        let effective_gas_price = transaction.effective_gas_price(receipt_effective_gas_price);
        let blob_gas_cost = transaction.blob_gas_cost;

        Ok(GasAndAmountForTx {
            tx_hash,
            block_number: entry.block_number,
            gas_used: GasAmount::from(gas_used),
            effective_gas_price: GasPrice::from(effective_gas_price),
            l1_fee,
            transferred_amount: entry.transfer_value,
            blob_gas_cost,
        })
    }

    async fn fetch_transaction_gas_data(
        &self,
        chain: NamedChain,
        entry: LogBatchEntry,
        pass: CombinedDataLookupPass,
    ) -> Result<Option<TransactionGasData>, CombinedDataLookupFailure> {
        let tx_hash = entry.tx_hash;

        match self.provider.get_transaction_by_hash(tx_hash).await {
            Ok(transaction) => Ok(transaction
                .as_ref()
                .map(TransactionGasData::from_transaction)),
            Err(error) if should_attempt_permissive_tx_decode(chain, &error) => {
                warn!(
                    ?chain,
                    ?tx_hash,
                    original_error = %error,
                    "Typed transaction lookup failed; retrying with permissive raw transaction decoding"
                );

                match self
                    .provider
                    .raw_request::<_, Option<AnyRpcTransaction>>(
                        Cow::Borrowed("eth_getTransactionByHash"),
                        (tx_hash,),
                    )
                    .await
                {
                    Ok(transaction) => {
                        if let Some(transaction) = transaction.as_ref() {
                            info!(
                                ?chain,
                                ?tx_hash,
                                tx_type = transaction.ty(),
                                "Recovered transaction lookup with permissive raw transaction decoding"
                            );
                        }

                        Ok(transaction
                            .as_ref()
                            .map(TransactionGasData::from_transaction))
                    }
                    Err(raw_error) => {
                        warn!(
                            ?chain,
                            ?tx_hash,
                            original_error = %error,
                            raw_fallback_error = %raw_error,
                            "Permissive raw transaction decoding failed after typed lookup error"
                        );

                        let typed_failure = lookup_request_failed(
                            tx_hash,
                            CombinedDataLookupStage::Transaction,
                            error,
                        );
                        let raw_fallback_failure =
                            RetrievalError::Rpc(crate::errors::RpcError::request_failed(
                                format!("permissive_raw_get_transaction_by_hash({tx_hash})"),
                                raw_error,
                            ));
                        let mut failure = build_lookup_failure(
                            entry,
                            pass,
                            CombinedDataLookupStage::Transaction,
                            typed_failure,
                        );
                        failure.attempts.push(build_lookup_attempt(
                            pass,
                            CombinedDataLookupStage::Transaction,
                            &raw_fallback_failure,
                        ));

                        Err(failure)
                    }
                }
            }
            Err(error) => Err(build_lookup_failure(
                entry,
                pass,
                CombinedDataLookupStage::Transaction,
                lookup_request_failed(tx_hash, CombinedDataLookupStage::Transaction, error),
            )),
        }
    }

    async fn fetch_tx_data_for_entry<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain: NamedChain,
        entry: LogBatchEntry,
        pass: CombinedDataLookupPass,
        adapter: &A,
    ) -> Result<GasAndAmountForTx, CombinedDataLookupFailure> {
        let provider = self.provider.clone();
        let tx_hash = entry.tx_hash;
        let span = spans::process_log_for_combined_data(tx_hash);

        // The serial fallback intentionally re-fetches both tx and receipt even if
        // only one side failed in the batch pass. That keeps the retry path simple
        // and symmetric at the cost of at most one redundant RPC with current bounds.
        let (tx_result, receipt_result) = async move {
            tokio::join!(
                self.fetch_transaction_gas_data(chain, entry, pass),
                provider.get_transaction_receipt(tx_hash)
            )
        }
        .instrument(span)
        .await;

        Self::process_lookup_results(entry, tx_result, receipt_result, pass, adapter)
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
        chain: NamedChain,
        log_entries: &[LogBatchEntry],
        adapter: &A,
    ) -> Vec<Result<GasAndAmountForTx, CombinedDataLookupFailure>> {
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
            .copied()
            .map(|entry| async move {
                self.fetch_tx_data_for_entry(chain, entry, CombinedDataLookupPass::Batch, adapter)
                    .await
            })
            .collect();

        // Execute all fetches in parallel
        // When CallBatchLayer is enabled, these may be batched together
        join_all(fetch_futures).await
    }

    async fn retry_failed_tx_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain: NamedChain,
        mut failure: CombinedDataLookupFailure,
        max_attempts: usize,
        adapter: &A,
    ) -> (Result<GasAndAmountForTx, CombinedDataLookupFailure>, usize) {
        let entry = LogBatchEntry {
            tx_hash: failure.tx_hash,
            block_number: failure.block_number,
            transfer_value: failure.transfer_value,
        };

        let mut attempts = 0;
        while attempts < max_attempts {
            attempts += 1;
            warn!(
                ?failure.tx_hash,
                block_number = failure.block_number,
                transfer_value = ?failure.transfer_value,
                attempt = attempts,
                max_attempts,
                "Retrying combined data lookup serially after batch failure"
            );

            match self
                .fetch_tx_data_for_entry(
                    chain,
                    entry,
                    CombinedDataLookupPass::SerialFallback,
                    adapter,
                )
                .await
            {
                Ok(data) => return (Ok(data), attempts),
                Err(retry_failure) => failure.attempts.extend(retry_failure.attempts),
            }
        }

        (Err(failure), attempts)
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
        async {
            let mut result =
                CombinedDataResult::new(chain, from_address, to_address, token_address);
            let mut current_block = from_block;

            // Get config values for this chain
            let max_block_range = self.config.get_max_block_range(chain);
            let rate_limit = self.config.get_rate_limit_delay(chain);
            let serial_lookup_fallback_attempts =
                self.config.get_serial_lookup_fallback_attempts(chain);

            while current_block <= to_block {
                let chunk_end =
                    std::cmp::min(current_block + max_block_range.as_u64() - 1, to_block);

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
                            "get_logs for blocks {current_block}-{chunk_end} on {chain:?}"
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
                let batch_results = self.batch_fetch_tx_data(chain, &log_entries, adapter).await;

                // Process batch results
                let mut batch_failures = Vec::new();
                for batch_result in batch_results {
                    match batch_result {
                        Ok(data) => {
                            result.add_transaction_data(data);
                        }
                        Err(failure) => {
                            batch_failures.push(failure);
                        }
                    }
                }

                if !batch_failures.is_empty() {
                    if serial_lookup_fallback_attempts == 0 {
                        warn!(
                            failed_lookups = batch_failures.len(),
                            "Batch combined lookups failed and serial fallback is disabled for this chain"
                        );
                    } else {
                        warn!(
                            failed_lookups = batch_failures.len(),
                            max_attempts_per_lookup = serial_lookup_fallback_attempts,
                            "Retrying failed combined lookups serially after batch pass"
                        );
                    }
                }

                // The fallback pass is intentionally sequential across failures to avoid
                // reproducing the original burst pattern against the provider.
                for batch_failure in batch_failures {
                    let (retry_result, fallback_attempts) = self
                        .retry_failed_tx_data(
                            chain,
                            batch_failure,
                            serial_lookup_fallback_attempts,
                            adapter,
                        )
                        .await;
                    result
                        .retrieval_metadata
                        .record_fallback_attempts(fallback_attempts);

                    match retry_result {
                        Ok(data) => {
                            result.retrieval_metadata.record_fallback_recovery();
                            result.add_transaction_data(data);
                        }
                        Err(failure) => {
                            log_combined_data_skip(
                                &failure,
                                chain,
                                from_address,
                                to_address,
                                token_address,
                                from_block,
                                to_block,
                            );
                            result.retrieval_metadata.record_partial_failure(failure);
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
            info!(
                ?chain,
                %from_address,
                %to_address,
                %token_address,
                from_block,
                to_block,
                transactions_found = result.transaction_count.as_usize(),
                skipped_logs = result.retrieval_metadata.skipped_logs,
                fallback_attempts = result.retrieval_metadata.fallback_attempts,
                fallback_recovered = result.retrieval_metadata.fallback_recovered,
                "Finished processing block range"
            );
            Ok(result)
        }
        .instrument(span)
        .await
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
        async {
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
        .instrument(span)
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_json_rpc as j;
    use alloy_network::Network;
    use alloy_primitives::{address, Address, LogData, B256, U256};
    use alloy_provider::{ProviderBuilder, RootProvider};
    use alloy_rpc_client::RpcClient;
    use alloy_sol_types::SolValue;
    use alloy_transport::{TransportErrorKind, TransportFut, TransportResult};
    use serde_json::json;
    use std::{
        borrow::Cow,
        collections::{HashMap, VecDeque},
        sync::{Arc, Mutex},
        task::{Context, Poll},
    };

    use crate::SemioscanConfigBuilder;

    #[derive(Clone, Debug, Default)]
    struct MethodResponseTransport {
        responses: Arc<Mutex<HashMap<String, VecDeque<j::ResponsePayload>>>>,
        request_counts: Arc<Mutex<HashMap<String, usize>>>,
    }

    impl MethodResponseTransport {
        fn push_success<R: serde::Serialize>(&self, method: &str, response: &R) {
            let serialized = serde_json::to_string(response).expect("response should serialize");
            let payload = j::ResponsePayload::Success(
                serde_json::value::RawValue::from_string(serialized)
                    .expect("response should convert to raw JSON"),
            );
            self.responses
                .lock()
                .expect("responses lock")
                .entry(method.to_string())
                .or_default()
                .push_back(payload);
        }

        fn push_failure_msg(&self, method: &str, message: impl Into<Cow<'static, str>>) {
            self.responses
                .lock()
                .expect("responses lock")
                .entry(method.to_string())
                .or_default()
                .push_back(j::ResponsePayload::internal_error_message(message.into()));
        }

        fn request_count(&self, method: &str) -> usize {
            self.request_counts
                .lock()
                .expect("request_counts lock")
                .get(method)
                .copied()
                .unwrap_or_default()
        }

        fn map_request(&self, request: j::SerializedRequest) -> TransportResult<j::Response> {
            let method = request.method().to_string();

            {
                let mut request_counts = self.request_counts.lock().expect("request_counts lock");
                *request_counts.entry(method.clone()).or_default() += 1;
            }

            let payload = self
                .responses
                .lock()
                .expect("responses lock")
                .entry(method.clone())
                .or_default()
                .pop_front()
                .ok_or_else(|| {
                    TransportErrorKind::custom_str(&format!(
                        "no mocked response queued for method {method}"
                    ))
                })?;

            Ok(j::Response {
                id: request.id().clone(),
                payload,
            })
        }

        async fn handle(self, request: j::RequestPacket) -> TransportResult<j::ResponsePacket> {
            Ok(match request {
                j::RequestPacket::Single(request) => {
                    j::ResponsePacket::Single(self.map_request(request)?)
                }
                // Fail fast when a batched test request is under-specified so missing
                // fixtures show up immediately instead of being converted into per-item
                // JSON-RPC failures that are harder to diagnose in unit tests.
                j::RequestPacket::Batch(requests) => j::ResponsePacket::Batch(
                    requests
                        .into_iter()
                        .map(|request| self.map_request(request))
                        .collect::<TransportResult<_>>()?,
                ),
            })
        }
    }

    impl tower::Service<j::RequestPacket> for MethodResponseTransport {
        type Response = j::ResponsePacket;
        type Error = alloy_transport::TransportError;
        type Future = TransportFut<'static>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: j::RequestPacket) -> Self::Future {
            Box::pin(self.clone().handle(request))
        }
    }

    fn create_transfer_log(
        tx_hash: TxHash,
        block_number: BlockNumber,
        token_address: Address,
        from_address: Address,
        to_address: Address,
        transfer_value: U256,
    ) -> RpcLog {
        RpcLog {
            inner: alloy_primitives::Log {
                address: token_address,
                data: LogData::new(
                    vec![
                        Transfer::SIGNATURE_HASH,
                        from_address.into_word(),
                        to_address.into_word(),
                    ],
                    transfer_value.abi_encode().into(),
                )
                .expect("valid log data"),
            },
            block_hash: Some(B256::repeat_byte(0x11)),
            block_number: Some(block_number),
            block_timestamp: Some(1_700_000_000),
            transaction_hash: Some(tx_hash),
            transaction_index: Some(0),
            log_index: Some(0),
            removed: false,
        }
    }

    fn create_test_transaction(
        tx_hash: TxHash,
        from_address: Address,
        to_address: Address,
    ) -> <Ethereum as Network>::TransactionResponse {
        serde_json::from_value(json!({
            "hash": tx_hash,
            "nonce": "0x1",
            "blockHash": B256::repeat_byte(0x22),
            "blockNumber": "0x64",
            "transactionIndex": "0x0",
            "from": from_address,
            "to": to_address,
            "value": "0x0",
            "gasPrice": "0x3a29f0f8",
            "gas": "0x5208",
            "maxFeePerGas": "0xba43b7400",
            "maxPriorityFeePerGas": "0x5f5e100",
            "input": "0x",
            "r": B256::repeat_byte(0x33),
            "s": B256::repeat_byte(0x44),
            "v": "0x0",
            "yParity": "0x0",
            "chainId": "0x1",
            "accessList": [],
            "type": "0x2"
        }))
        .expect("valid transaction response")
    }

    fn create_zksync_transaction_without_access_list(
        tx_hash: TxHash,
        from_address: Address,
        to_address: Address,
    ) -> serde_json::Value {
        json!({
            "hash": tx_hash,
            "nonce": "0x5d",
            "blockHash": B256::repeat_byte(0x22),
            "blockNumber": "0x41aa3d2",
            "transactionIndex": "0x0",
            "from": from_address,
            "value": "0x0",
            "gasPrice": "0x2b275d0",
            "gas": "0x1a5c69",
            "input": "0x",
            "yParity": "0x1",
            "v": "0x1",
            "r": B256::repeat_byte(0x33),
            "s": B256::repeat_byte(0x44),
            "type": "0x2",
            "maxFeePerGas": "0x564eba1",
            "maxPriorityFeePerGas": "0x1",
            "chainId": "0x144",
            "l1BatchNumber": "0x7be1b",
            "l1BatchTxIndex": "0x7d",
            "to": to_address
        })
    }

    #[test]
    fn permissive_tx_decode_guard_only_accepts_zksync_deser_errors() {
        let deser_error =
            serde_json::from_str::<u64>("\"not-a-number\"").expect_err("response should fail");
        let zksync_error = TransportError::deser_err(deser_error, "\"not-a-number\"");
        let transport_error = TransportError::from(TransportErrorKind::custom_str("boom"));

        assert!(should_attempt_permissive_tx_decode(
            NamedChain::ZkSync,
            &zksync_error
        ));
        assert!(should_attempt_permissive_tx_decode(
            NamedChain::ZkSyncTestnet,
            &zksync_error
        ));
        assert!(!should_attempt_permissive_tx_decode(
            NamedChain::Mainnet,
            &zksync_error
        ));
        assert!(!should_attempt_permissive_tx_decode(
            NamedChain::ZkSync,
            &transport_error
        ));
    }

    #[tokio::test]
    async fn zksync_raw_fallback_failure_is_recorded_in_partial_metadata() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::ZkSync;
        let from_address = address!("0x0D05a7D3448512B78fa8A9e46c4872C88C4a0D05");
        let to_address = address!("0x5E1c87A1589BCC4325Db77Be49874941b2297a7B");
        let token_address = address!("0x1d17CBcF0D6D143135aE902365D2E5e2A16538D4");
        let tx_hash = TxHash::from(B256::repeat_byte(0xEE));
        let transfer_value = U256::from(51_057_101_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                68_854_738,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &create_zksync_transaction_without_access_list(tx_hash, from_address, to_address),
        );
        transport.push_failure_msg(
            "eth_getTransactionByHash",
            "raw fallback failed during batch",
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &create_zksync_transaction_without_access_list(tx_hash, from_address, to_address),
        );
        transport.push_failure_msg(
            "eth_getTransactionByHash",
            "raw fallback failed during retry",
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                68_854_738,
                68_854_738,
            )
            .await
            .expect("combined calculation should return partial metadata instead of erroring");

        assert!(result.is_partial());
        assert_eq!(result.retrieval_metadata.skipped_logs, 1);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 1);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);
        assert_eq!(result.retrieval_metadata.partial_failures.len(), 1);
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 4);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 2);

        let failure = &result.retrieval_metadata.partial_failures[0];
        assert_eq!(failure.attempts.len(), 4);
        assert!(failure.attempts[0]
            .error
            .contains("get_transaction_by_hash"));
        assert!(failure.attempts[1]
            .error
            .contains("permissive_raw_get_transaction_by_hash"));
        assert!(failure.attempts[1]
            .transport_error
            .as_deref()
            .is_some_and(|error| error.contains("raw fallback failed during batch")));
        assert!(failure
            .final_attempt()
            .and_then(|attempt| attempt.transport_error.as_deref())
            .is_some_and(|error| error.contains("raw fallback failed during retry")));
    }

    fn create_test_receipt(
        tx_hash: TxHash,
        from_address: Address,
        to_address: Address,
        gas_used: u64,
        effective_gas_price: u128,
    ) -> <Ethereum as Network>::ReceiptResponse {
        serde_json::from_value(json!({
            "transactionHash": tx_hash,
            "blockHash": B256::repeat_byte(0x22),
            "blockNumber": "0x64",
            "transactionIndex": "0x0",
            "from": from_address,
            "to": to_address,
            "cumulativeGasUsed": format!("0x{gas_used:x}"),
            "gasUsed": format!("0x{gas_used:x}"),
            "effectiveGasPrice": format!("0x{effective_gas_price:x}"),
            "logs": [],
            "logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "status": "0x1",
            "type": "0x2"
        }))
        .expect("valid receipt response")
    }

    fn create_calculator(
        transport: MethodResponseTransport,
    ) -> CombinedCalculator<Ethereum, RootProvider<Ethereum>> {
        create_calculator_with_config(transport, SemioscanConfig::default())
    }

    fn create_calculator_with_config(
        transport: MethodResponseTransport,
        config: SemioscanConfig,
    ) -> CombinedCalculator<Ethereum, RootProvider<Ethereum>> {
        let provider = ProviderBuilder::default().connect_client(RpcClient::new(transport, true));
        CombinedCalculator::with_config(provider, config)
    }

    #[tokio::test]
    async fn successful_lookup_returns_complete_result_without_partial_metadata() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::Mainnet;
        let from_address = address!("0xa111111111111111111111111111111111111111");
        let to_address = address!("0xb222222222222222222222222222222222222222");
        let token_address = address!("0xc333333333333333333333333333333333333333");
        let tx_hash = TxHash::from(B256::repeat_byte(0x10));
        let transfer_value = U256::from(1_234_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                42,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &Some(create_test_transaction(tx_hash, from_address, to_address)),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                42,
                42,
            )
            .await
            .expect("combined calculation should succeed");

        assert!(!result.is_partial());
        assert_eq!(result.transaction_count.as_usize(), 1);
        assert_eq!(result.transactions_data.len(), 1);
        assert_eq!(result.total_amount_transferred, transfer_value);
        assert_eq!(result.retrieval_metadata.skipped_logs, 0);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 0);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);
        assert!(result.retrieval_metadata.partial_failures.is_empty());
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 1);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 1);
    }

    #[tokio::test]
    async fn tx_lookup_failure_marks_result_partial_and_surfaces_metadata() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::Mainnet;
        let from_address = address!("0x1111111111111111111111111111111111111111");
        let to_address = address!("0x2222222222222222222222222222222222222222");
        let token_address = address!("0x3333333333333333333333333333333333333333");
        let tx_hash = TxHash::from(B256::repeat_byte(0xAA));
        let transfer_value = U256::from(777_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                100,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_failure_msg("eth_getTransactionByHash", "batch tx lookup failed");
        transport.push_failure_msg("eth_getTransactionByHash", "fallback tx lookup failed");
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                100,
                100,
            )
            .await
            .expect("combined calculation should return partial result");

        assert!(result.is_partial());
        assert_eq!(result.transaction_count.as_usize(), 0);
        assert_eq!(result.transactions_data.len(), 0);
        assert_eq!(result.total_amount_transferred, U256::ZERO);
        assert_eq!(result.retrieval_metadata.skipped_logs, 1);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 1);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);
        assert_eq!(result.retrieval_metadata.skipped_tx_hashes(), vec![tx_hash]);

        let failure = &result.retrieval_metadata.partial_failures[0];
        assert_eq!(failure.tx_hash, tx_hash);
        assert_eq!(failure.block_number, 100);
        assert_eq!(failure.transfer_value, transfer_value);
        assert_eq!(failure.attempts.len(), 2);
        assert_eq!(failure.attempts[0].pass, CombinedDataLookupPass::Batch);
        assert_eq!(
            failure.attempts[0].stage,
            CombinedDataLookupStage::Transaction
        );
        assert_eq!(
            failure.attempts[1].pass,
            CombinedDataLookupPass::SerialFallback
        );
        assert_eq!(
            failure.attempts[1].stage,
            CombinedDataLookupStage::Transaction
        );
        assert!(failure.attempts[0]
            .transport_error
            .as_deref()
            .expect("batch transport error should be present")
            .contains("batch tx lookup failed"));
        assert!(failure.attempts[1]
            .transport_error
            .as_deref()
            .expect("fallback transport error should be present")
            .contains("fallback tx lookup failed"));
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 2);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 2);
    }

    #[tokio::test]
    async fn receipt_lookup_failure_marks_result_partial_and_surfaces_metadata() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::Mainnet;
        let from_address = address!("0x4444444444444444444444444444444444444444");
        let to_address = address!("0x5555555555555555555555555555555555555555");
        let token_address = address!("0x6666666666666666666666666666666666666666");
        let tx_hash = TxHash::from(B256::repeat_byte(0xBB));
        let transfer_value = U256::from(888_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                200,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &Some(create_test_transaction(tx_hash, from_address, to_address)),
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &Some(create_test_transaction(tx_hash, from_address, to_address)),
        );
        transport.push_failure_msg("eth_getTransactionReceipt", "batch receipt lookup failed");
        transport.push_failure_msg(
            "eth_getTransactionReceipt",
            "fallback receipt lookup failed",
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                200,
                200,
            )
            .await
            .expect("combined calculation should return partial result");

        assert!(result.is_partial());
        assert_eq!(result.retrieval_metadata.skipped_logs, 1);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 1);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);

        let failure = &result.retrieval_metadata.partial_failures[0];
        assert_eq!(failure.tx_hash, tx_hash);
        assert_eq!(failure.attempts.len(), 2);
        assert_eq!(failure.attempts[0].pass, CombinedDataLookupPass::Batch);
        assert_eq!(failure.attempts[0].stage, CombinedDataLookupStage::Receipt);
        assert_eq!(
            failure.attempts[1].pass,
            CombinedDataLookupPass::SerialFallback
        );
        assert_eq!(failure.attempts[1].stage, CombinedDataLookupStage::Receipt);
        assert!(failure.attempts[0]
            .transport_error
            .as_deref()
            .expect("batch transport error should be present")
            .contains("batch receipt lookup failed"));
        assert!(failure.attempts[1]
            .transport_error
            .as_deref()
            .expect("fallback transport error should be present")
            .contains("fallback receipt lookup failed"));
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 2);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 2);
    }

    #[tokio::test]
    async fn serial_fallback_recovers_transaction_lookup_without_marking_partial() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::Mainnet;
        let from_address = address!("0x7777777777777777777777777777777777777777");
        let to_address = address!("0x8888888888888888888888888888888888888888");
        let token_address = address!("0x9999999999999999999999999999999999999999");
        let tx_hash = TxHash::from(B256::repeat_byte(0xCC));
        let transfer_value = U256::from(999_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                300,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_failure_msg("eth_getTransactionByHash", "batch tx lookup failed");
        transport.push_success(
            "eth_getTransactionByHash",
            &Some(create_test_transaction(tx_hash, from_address, to_address)),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                300,
                300,
            )
            .await
            .expect("combined calculation should succeed after fallback");

        assert!(!result.is_partial());
        assert_eq!(result.transaction_count.as_usize(), 1);
        assert_eq!(result.transactions_data.len(), 1);
        assert_eq!(result.total_amount_transferred, transfer_value);
        assert_eq!(result.retrieval_metadata.skipped_logs, 0);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 1);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 1);
        assert!(result.retrieval_metadata.partial_failures.is_empty());
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 2);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 2);
    }

    #[tokio::test]
    async fn zero_configured_serial_fallback_attempts_skip_retry_pass() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::ZkSync;
        let from_address = address!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let to_address = address!("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let token_address = address!("0xcccccccccccccccccccccccccccccccccccccccc");
        let tx_hash = TxHash::from(B256::repeat_byte(0xCD));
        let transfer_value = U256::from(1_111_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                301,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_failure_msg("eth_getTransactionByHash", "batch tx lookup failed");
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let config = SemioscanConfigBuilder::new()
            .chain_serial_lookup_fallback_attempts(chain, 0)
            .build();
        let calculator = create_calculator_with_config(transport.clone(), config);
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                301,
                301,
            )
            .await
            .expect("combined calculation should return partial result");

        assert!(result.is_partial());
        assert_eq!(result.retrieval_metadata.skipped_logs, 1);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 0);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 1);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 1);
    }

    #[tokio::test]
    async fn zksync_missing_access_list_uses_permissive_tx_decode_and_stays_complete() {
        let transport = MethodResponseTransport::default();
        let chain = NamedChain::ZkSync;
        let from_address = address!("0x0D05a7D3448512B78fa8A9e46c4872C88C4a0D05");
        let to_address = address!("0x5E1c87A1589BCC4325Db77Be49874941b2297a7B");
        let token_address = address!("0x1d17CBcF0D6D143135aE902365D2E5e2A16538D4");
        let tx_hash = TxHash::from(B256::repeat_byte(0xDD));
        let transfer_value = U256::from(51_057_101_u64);

        transport.push_success(
            "eth_getLogs",
            &vec![create_transfer_log(
                tx_hash,
                68_854_738,
                token_address,
                from_address,
                to_address,
                transfer_value,
            )],
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &create_zksync_transaction_without_access_list(tx_hash, from_address, to_address),
        );
        transport.push_success(
            "eth_getTransactionByHash",
            &create_zksync_transaction_without_access_list(tx_hash, from_address, to_address),
        );
        transport.push_success(
            "eth_getTransactionReceipt",
            &Some(create_test_receipt(
                tx_hash,
                from_address,
                to_address,
                21_000,
                100,
            )),
        );

        let calculator = create_calculator(transport.clone());
        let result = calculator
            .calculate_combined_data_ethereum(
                chain,
                from_address,
                to_address,
                token_address,
                68_854_738,
                68_854_738,
            )
            .await
            .expect("combined calculation should recover from zkSync tx shape mismatch");

        assert!(!result.is_partial());
        assert_eq!(result.transaction_count.as_usize(), 1);
        assert_eq!(result.transactions_data.len(), 1);
        assert_eq!(result.total_amount_transferred, transfer_value);
        assert_eq!(result.retrieval_metadata.skipped_logs, 0);
        assert_eq!(result.retrieval_metadata.fallback_attempts, 0);
        assert_eq!(result.retrieval_metadata.fallback_recovered, 0);
        assert!(result.retrieval_metadata.partial_failures.is_empty());
        assert_eq!(transport.request_count("eth_getTransactionByHash"), 2);
        assert_eq!(transport.request_count("eth_getTransactionReceipt"), 1);
    }
}
