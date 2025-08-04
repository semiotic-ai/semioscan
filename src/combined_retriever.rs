use alloy_network::{eip2718::Typed2718, Ethereum, Network};
use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use op_alloy_network::Optimism;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, instrument, trace, warn, Level};

use crate::{
    EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter, Transfer,
    TRANSFER_EVENT_SIGNATURE,
};

const BLOB_GAS_PER_BLOB: u64 = 131_072; // EIP-4844 blob gas per blob
const MAX_BLOCK_RANGE: u64 = 499; // Max blocks to query in one go (0-499 = 500 blocks)

/// Core gas calculation logic (adapted from gas.rs)
pub struct GasCalculationCore;
impl GasCalculationCore {
    fn calculate_blob_gas_cost<N: Network>(transaction: &N::TransactionResponse) -> U256
    where
        N::TransactionResponse: TransactionTrait + alloy_provider::network::eip2718::Typed2718,
    {
        if !transaction.is_eip4844() {
            return U256::ZERO;
        }
        let blob_count = transaction
            .blob_versioned_hashes()
            .map(|hashes| hashes.len())
            .unwrap_or_default();
        let blob_gas_used = U256::from(blob_count * BLOB_GAS_PER_BLOB as usize);
        let blob_gas_price = U256::from(transaction.max_fee_per_blob_gas().unwrap_or_default());
        blob_gas_used.saturating_mul(blob_gas_price)
    }

    fn calculate_effective_gas_price<N: Network>(
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

    fn create_transfer_filter(
        current_block: u64,
        to_block: u64,
        token_address: Address,
        from_address: Address, // topic1
        to_address: Address,   // topic2
    ) -> Filter {
        let transfer_topic_hash = keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes());
        Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .address(token_address)
            .event_signature(transfer_topic_hash) // This takes B256, not Vec<B256>
            .topic1(from_address)
            .topic2(to_address)
    }
}

/// Data for a single transaction including gas and transferred amount.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GasAndAmountForTx {
    pub tx_hash: B256,
    pub gas_used: U256,            // L2 gas used
    pub effective_gas_price: U256, // L2 effective gas price
    pub l1_fee: Option<U256>,      // L1 data fee for L2s
    pub blob_gas_cost: U256,       // Cost from EIP-4844 blobs
    pub transferred_amount: U256,
}

impl GasAndAmountForTx {
    /// Calculates the total gas cost for this transaction, including L2 gas, L1 fee, and blob gas.
    pub fn total_gas_cost(&self) -> U256 {
        let l2_execution_cost = self.gas_used.saturating_mul(self.effective_gas_price);
        let total_cost = l2_execution_cost.saturating_add(self.blob_gas_cost);
        total_cost.saturating_add(self.l1_fee.unwrap_or_default())
    }
}

/// Aggregated result for combined data retrieval over a block range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinedDataResult {
    pub chain_id: u64,
    pub from_address: Address,
    pub to_address: Address,
    pub token_address: Address,
    pub total_l2_execution_cost: U256,
    pub total_blob_gas_cost: U256,
    pub total_l1_fee: U256,
    pub overall_total_gas_cost: U256,
    pub total_amount_transferred: U256,
    pub transaction_count: usize,
    pub transactions_data: Vec<GasAndAmountForTx>,
}

impl CombinedDataResult {
    pub fn new(
        chain_id: u64,
        from_address: Address,
        to_address: Address,
        token_address: Address,
    ) -> Self {
        Self {
            chain_id,
            from_address,
            to_address,
            token_address,
            total_l2_execution_cost: U256::ZERO,
            total_blob_gas_cost: U256::ZERO,
            total_l1_fee: U256::ZERO,
            overall_total_gas_cost: U256::ZERO,
            total_amount_transferred: U256::ZERO,
            transaction_count: 0,
            transactions_data: Vec::new(),
        }
    }

    pub fn add_transaction_data(&mut self, data: GasAndAmountForTx) {
        self.total_amount_transferred = self
            .total_amount_transferred
            .saturating_add(data.transferred_amount);

        let l2_execution_cost = data.gas_used.saturating_mul(data.effective_gas_price);
        self.total_l2_execution_cost = self
            .total_l2_execution_cost
            .saturating_add(l2_execution_cost);
        self.total_blob_gas_cost = self.total_blob_gas_cost.saturating_add(data.blob_gas_cost);

        if let Some(l1_fee) = data.l1_fee {
            self.total_l1_fee = self.total_l1_fee.saturating_add(l1_fee);
        }

        self.overall_total_gas_cost = self
            .total_l2_execution_cost
            .saturating_add(self.total_blob_gas_cost)
            .saturating_add(self.total_l1_fee);

        self.transactions_data.push(data);
        self.transaction_count += 1;
    }

    pub fn merge(&mut self, other: &CombinedDataResult) {
        if self.chain_id != other.chain_id
            || self.from_address != other.from_address
            || self.to_address != other.to_address
            || self.token_address != other.token_address
        {
            error!(self_params = ?(self.chain_id, self.from_address, self.to_address, self.token_address),
                   other_params = ?(other.chain_id, other.from_address, other.to_address, other.token_address),
                   "Attempted to merge CombinedDataResult with mismatched parameters.");
            return;
        }

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
            .extend(other.transactions_data.iter().cloned()); // Consider efficiency for very large Vecs
    }
}

pub struct CombinedCalculator<N: Network, P: Provider<N> + Send + Sync + Clone + 'static>
where
    N::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    N::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    provider: Arc<P>,
    network_marker: std::marker::PhantomData<N>,
}

impl<N: Network, P: Provider<N> + Send + Sync + Clone + 'static> CombinedCalculator<N, P>
where
    N::TransactionResponse:
        TransactionTrait + alloy_provider::network::eip2718::Typed2718 + Send + Sync + Clone,
    N::ReceiptResponse: Send + Sync + std::fmt::Debug + Clone,
{
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            network_marker: std::marker::PhantomData,
        }
    }

    #[instrument(skip(self, rpc_log_entry, adapter, transfer_event), fields(tx_hash = ?rpc_log_entry.transaction_hash), ret(level = Level::TRACE))]
    async fn process_log_for_combined_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        rpc_log_entry: &RpcLog,
        adapter: &A,
        transfer_event: &Transfer, // Decoded event
    ) -> anyhow::Result<Option<GasAndAmountForTx>> {
        let tx_hash = rpc_log_entry.transaction_hash.ok_or_else(|| {
            anyhow::anyhow!("Transaction hash not found for log: {:?}", rpc_log_entry)
        })?;

        let transaction_fut = self.provider.get_transaction_by_hash(tx_hash);
        let receipt_fut = self.provider.get_transaction_receipt(tx_hash);

        let (transaction_res, receipt_res) = tokio::join!(transaction_fut, receipt_fut);

        let transaction = transaction_res?
            .ok_or_else(|| anyhow::anyhow!("Transaction not found for hash: {}", tx_hash))?;
        let receipt = receipt_res?
            .ok_or_else(|| anyhow::anyhow!("Receipt not found for hash: {}", tx_hash))?;

        let gas_used = adapter.gas_used(&receipt);
        let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);
        let l1_fee = adapter.l1_data_fee(&receipt);

        let effective_gas_price = GasCalculationCore::calculate_effective_gas_price::<N>(
            &transaction,
            receipt_effective_gas_price,
        );

        let blob_gas_cost = GasCalculationCore::calculate_blob_gas_cost::<N>(&transaction);

        Ok(Some(GasAndAmountForTx {
            tx_hash,
            gas_used,
            effective_gas_price,
            l1_fee,
            transferred_amount: transfer_event.value,
            blob_gas_cost,
        }))
    }

    #[instrument(skip(self, adapter))]
    #[allow(clippy::too_many_arguments)]
    async fn process_block_range_for_combined_data<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain_id: u64,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        from_block: u64,
        to_block: u64,
        adapter: &A,
    ) -> anyhow::Result<CombinedDataResult> {
        let mut result = CombinedDataResult::new(chain_id, from_address, to_address, token_address);
        let mut current_block = from_block;

        while current_block <= to_block {
            let chunk_end = std::cmp::min(current_block + MAX_BLOCK_RANGE, to_block);

            let filter = GasCalculationCore::create_transfer_filter(
                current_block,
                chunk_end,
                token_address,
                from_address,
                to_address,
            );

            trace!(?filter, current_block, chunk_end, "Fetching logs");
            let logs: Vec<RpcLog> = self.provider.get_logs(&filter).await?;
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
                            chain_id, ?from_address, ?to_address, ?token_address,
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

            // Delay for specific chains to avoid rate limiting
            if chain_id.eq(&146) && current_block <= to_block {
                trace!(chain_id, "Applying delay for chain 146");
                sleep(Duration::from_millis(250)).await;
            }
        }
        info!(chain_id, %from_address, %to_address, %token_address, from_block, to_block, transactions_found = result.transaction_count, "Finished processing block range");
        Ok(result)
    }

    /// Calculates combined transfer amount and gas cost data.
    /// Caching is not implemented in this version but can be added by adapting GasCostCache logic.
    #[instrument(skip(self, adapter), level = "info", ret(level = Level::INFO))]
    #[allow(clippy::too_many_arguments)]
    pub async fn calculate_combined_data_with_adapter<A: ReceiptAdapter<N> + Send + Sync>(
        &self,
        chain_id: u64,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        start_block: u64,
        end_block: u64,
        adapter: &A,
    ) -> anyhow::Result<CombinedDataResult> {
        self.process_block_range_for_combined_data(
            chain_id,
            from_address,
            to_address,
            token_address,
            start_block,
            end_block,
            adapter,
        )
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
        chain_id: u64,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<CombinedDataResult> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_combined_data_with_adapter(
            chain_id,
            from_address,
            to_address,
            token_address,
            start_block,
            end_block,
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
        chain_id: u64,
        from_address: Address,
        to_address: Address,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<CombinedDataResult> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_combined_data_with_adapter(
            chain_id,
            from_address,
            to_address,
            token_address,
            start_block,
            end_block,
            &adapter,
        )
        .await
    }
}
