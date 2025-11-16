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
use alloy_primitives::{Address, BlockNumber};
use alloy_provider::Provider;
use alloy_rpc_types::{Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use op_alloy_network::Optimism;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{error, info, trace, warn};

use crate::config::SemioscanConfig;
use crate::events::definitions::Transfer;
use crate::gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
use crate::tracing::spans;
use crate::types::gas::{GasAmount, GasPrice};

use super::gas_calculation::GasCalculationCore;
use super::types::{CombinedDataResult, GasAndAmountForTx};
use crate::errors::RetrievalError;

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

    async fn process_log_for_combined_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        rpc_log_entry: &RpcLog,
        adapter: &A,
        transfer_event: &Transfer, // Decoded event
    ) -> Result<Option<GasAndAmountForTx>, RetrievalError> {
        let tx_hash = rpc_log_entry
            .transaction_hash
            .ok_or_else(RetrievalError::missing_transaction_hash)?;

        let span = spans::process_log_for_combined_data(tx_hash);
        let _guard = span.enter();

        let transaction_fut = self.provider.get_transaction_by_hash(tx_hash);
        let receipt_fut = self.provider.get_transaction_receipt(tx_hash);

        let (transaction_res, receipt_res) = tokio::join!(transaction_fut, receipt_fut);

        let transaction = transaction_res
            .map_err(|e| {
                RetrievalError::Rpc(crate::errors::RpcError::chain_connection_failed(
                    format!("get_transaction_by_hash({})", tx_hash),
                    e,
                ))
            })?
            .ok_or_else(|| RetrievalError::missing_transaction(&tx_hash.to_string()))?;

        let receipt = receipt_res
            .map_err(|e| {
                RetrievalError::Rpc(crate::errors::RpcError::chain_connection_failed(
                    format!("get_transaction_receipt({})", tx_hash),
                    e,
                ))
            })?
            .ok_or_else(|| RetrievalError::missing_receipt(&tx_hash.to_string()))?;

        let gas_used = adapter.gas_used(&receipt);
        let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);
        let l1_fee = adapter.l1_data_fee(&receipt);

        let effective_gas_price = GasCalculationCore::calculate_effective_gas_price::<N>(
            &transaction,
            receipt_effective_gas_price,
        );

        let blob_gas_cost = GasCalculationCore::calculate_blob_gas_cost::<N>(&transaction);

        let block_number = rpc_log_entry
            .block_number
            .ok_or_else(RetrievalError::missing_block_number)?;

        Ok(Some(GasAndAmountForTx {
            tx_hash,
            block_number,
            gas_used: GasAmount::from(gas_used),
            effective_gas_price: GasPrice::from(effective_gas_price),
            l1_fee,
            transferred_amount: transfer_event.value,
            blob_gas_cost,
        }))
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

            for rpc_log_entry in &logs {
                match Transfer::decode_log(&rpc_log_entry.inner) {
                    Ok(transfer_event_data) => {
                        info!(
                            ?chain, ?from_address, ?to_address, ?token_address,
                            amount = ?transfer_event_data.value,
                            block = rpc_log_entry.block_number,
                            tx_hash = ?rpc_log_entry.transaction_hash,
                            "Processing Transfer event"
                        );

                        match self
                            .process_log_for_combined_data(
                                rpc_log_entry,
                                adapter,
                                &transfer_event_data,
                            )
                            .await
                        {
                            Ok(Some(data)) => {
                                result.add_transaction_data(data);
                            }
                            Ok(None) => {
                                warn!("No transfer event found for log: {:?}", rpc_log_entry);
                            }
                            Err(e) => {
                                error!(error = %e, tx_hash = ?rpc_log_entry.transaction_hash, "Error processing log for combined data. Skipping log.");
                                // Continue with other logs
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, log_data = ?rpc_log_entry.data(), log_topics = ?rpc_log_entry.topics(), "Failed to decode Transfer log. Skipping log.");
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
