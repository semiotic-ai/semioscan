use alloy_chains::NamedChain;
use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{network::eip2718::Typed2718, Provider, RootProvider};
use alloy_rpc_types::{Filter, Log, TransactionTrait};
use alloy_sol_types::SolEvent;
use axum::{extract::Query, extract::State, Json};
use odos_sdk::{
    OdosChain,
    OdosV2Router::{Swap, SwapMulti},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

use crate::{CalculateGasCommand, GasCache, SemioscanHandle};
use tracing::{debug, error, info};

use crate::{Command, RouterType};

#[derive(Default, Debug, Clone, Serialize)]
pub struct GasCostResult {
    pub chain_id: u64,
    pub signer_address: Address,
    pub total_gas_cost: U256,
    pub transaction_count: usize,
}

impl GasCostResult {
    pub fn new(chain_id: u64, signer_address: Address) -> Self {
        Self {
            signer_address,
            chain_id,
            total_gas_cost: U256::ZERO,
            transaction_count: 0,
        }
    }

    fn add_transaction(&mut self, gas_used: U256, effective_gas_price: U256) {
        let gas_cost = gas_used.saturating_mul(effective_gas_price);
        self.total_gas_cost = self.total_gas_cost.saturating_add(gas_cost);
        self.transaction_count += 1;
    }

    /// Merge another gas cost result into this one
    pub fn merge(&mut self, other: &Self) {
        self.total_gas_cost = self.total_gas_cost.saturating_add(other.total_gas_cost);
        self.transaction_count += other.transaction_count;
    }

    /// Get the total gas cost formatted as a string
    pub fn formatted_gas_cost(&self) -> String {
        self.format_gas_cost()
    }

    fn format_gas_cost(&self) -> String {
        let gas_cost = self.total_gas_cost;

        const DECIMALS: u8 = 18; // All EVM chains use 18 decimals
        let divisor = U256::from(10).pow(U256::from(DECIMALS));

        let whole = gas_cost / divisor;
        let fractional = gas_cost % divisor;

        // Convert fractional part to string with leading zeros
        let fractional_str = format!("{:0width$}", fractional, width = DECIMALS as usize);

        // Format with proper decimal places, ensuring we don't have trailing zeros
        format!("{}.{}", whole, fractional_str.trim_end_matches('0'))
    }
}

// Event signatures as constants
const MULTI_SWAP_SIGNATURE: &str =
    "SwapMulti(address,uint256[],address[],uint256[],address[],uint32)";
const SINGLE_SWAP_SIGNATURE: &str = "Swap(address,uint256,address,uint256,address,int256,uint32)";

// Maximum number of blocks to query in a single request
const MAX_BLOCK_RANGE: u64 = 2_000;

pub struct GasCostCalculator {
    provider: RootProvider,
    gas_cache: Arc<TokioMutex<GasCache>>,
}

impl GasCostCalculator {
    pub fn new(provider: RootProvider) -> Self {
        Self {
            provider,
            gas_cache: Arc::new(TokioMutex::new(GasCache::default())),
        }
    }

    pub fn with_cache(provider: RootProvider, gas_cache: Arc<TokioMutex<GasCache>>) -> Self {
        Self {
            provider,
            gas_cache,
        }
    }

    async fn process_swap_event(
        &self,
        log: &Log,
        event: &SwapMulti,
        signer_address: Address,
        output_token: Address,
    ) -> anyhow::Result<Option<(U256, U256)>> {
        if event.sender != signer_address {
            debug!("Skipping swap not initiated by the specified signer address");
            return Ok(None);
        }

        info!(
            event = ?event,
            "Processing SwapMulti event for gas cost"
        );

        // Check if the specified output token is in the output tokens
        if event.tokensOut.contains(&output_token) {
            if let Some(tx_hash) = log.transaction_hash {
                if let Some(transaction) = self.provider.get_transaction_by_hash(tx_hash).await? {
                    let receipt = self
                        .provider
                        .get_transaction_receipt(tx_hash)
                        .await?
                        .unwrap();

                    let gas_used = U256::from(receipt.gas_used);

                    // Get the effective gas price based on transaction type
                    let effective_gas_price = if transaction.is_legacy_gas() {
                        // For legacy transactions, use gas_price directly
                        U256::from(transaction.gas_price().unwrap_or_default())
                    } else {
                        // For EIP-1559 and EIP-4844, use the effective_gas_price from receipt
                        info!("EIP-1559 or EIP-4844 transaction");
                        U256::from(receipt.effective_gas_price)
                    };

                    info!(
                        gas_used = ?gas_used,
                        effective_gas_price = ?effective_gas_price,
                        "Transaction details for gas calculation"
                    );

                    // Calculate regular gas cost (gas_used * effective_gas_price)
                    let regular_gas_cost = gas_used.saturating_mul(effective_gas_price);

                    // For EIP-4844 transactions, we need to add blob gas costs
                    let total_gas_cost = if transaction.is_eip4844() {
                        // EIP-4844 transaction
                        let blob_gas_used = U256::from(
                            transaction
                                .blob_versioned_hashes()
                                .map(|hashes| hashes.len() * 131072) // Each blob is 131072 gas
                                .unwrap_or_default(),
                        );

                        let blob_gas_price =
                            U256::from(transaction.max_fee_per_blob_gas().unwrap_or_default());
                        let blob_cost = blob_gas_used.saturating_mul(blob_gas_price);

                        // Regular gas cost + blob gas cost
                        regular_gas_cost.saturating_add(blob_cost)
                    } else {
                        // Regular gas cost for other transaction types
                        info!("Regular gas cost for other transaction types");
                        regular_gas_cost
                    };

                    info!(
                        regular_gas_cost = ?regular_gas_cost,
                        total_gas_cost = ?total_gas_cost,
                        "Calculated gas costs"
                    );

                    return Ok(Some((gas_used, effective_gas_price)));
                }
            }
        }
        Ok(None)
    }

    async fn process_single_swap_event(
        &self,
        log: &Log,
        event: &Swap,
        signer_address: Address,
        output_token: Address,
    ) -> anyhow::Result<Option<(U256, U256)>> {
        if event.sender != signer_address {
            debug!("Skipping single swap not initiated by the specified signer address");
            return Ok(None);
        }

        info!(
            event = ?event,
            "Processing single Swap event for gas cost"
        );

        // Check if the specified output token is in the output tokens
        if event.outputToken == output_token {
            if let Some(tx_hash) = log.transaction_hash {
                if let Some(transaction) = self.provider.get_transaction_by_hash(tx_hash).await? {
                    let gas_used = U256::from(
                        self.provider
                            .get_transaction_receipt(tx_hash)
                            .await?
                            .unwrap()
                            .gas_used,
                    );

                    // Get the effective gas price based on transaction type
                    let effective_gas_price = if transaction.is_legacy_gas() {
                        // For legacy transactions, use gas_price directly
                        U256::from(transaction.gas_price().unwrap_or_default())
                    } else {
                        // For EIP-1559 and EIP-4844, use effective_gas_price
                        U256::from(transaction.effective_gas_price.unwrap_or_default())
                    };

                    info!(
                        effective_gas_price = ?effective_gas_price,
                        "Effective gas price"
                    );

                    // For EIP-4844 transactions, we need to add blob gas costs
                    let total_gas_cost = if transaction.ty() == 3 {
                        // EIP-4844 transaction
                        let blob_gas_used = U256::from(
                            transaction
                                .blob_versioned_hashes()
                                .map(|hashes| hashes.len() * 131072) // Each blob is 131072 gas
                                .unwrap_or_default(),
                        );

                        let blob_gas_price =
                            U256::from(transaction.max_fee_per_blob_gas().unwrap_or_default());
                        let blob_cost = blob_gas_used.saturating_mul(blob_gas_price);

                        // Regular gas cost + blob gas cost
                        gas_used
                            .saturating_mul(effective_gas_price)
                            .saturating_add(blob_cost)
                    } else {
                        // Regular gas cost for other transaction types
                        gas_used.saturating_mul(effective_gas_price)
                    };

                    return Ok(Some((gas_used, total_gas_cost)));
                }
            }
        }
        Ok(None)
    }

    async fn process_logs_in_range(
        &self,
        chain_id: u64,
        signer_address: Address,
        output_token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let mut result = GasCostResult::new(chain_id, signer_address);
        let mut current_block = start_block;

        let chain = NamedChain::try_from(chain_id)
            .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

        let router_address = chain.v2_router_address();

        // Precompute event topics to avoid repeated calculations
        let multi_swap_topic = B256::from_slice(&*keccak256(MULTI_SWAP_SIGNATURE.as_bytes()));
        let single_swap_topic = B256::from_slice(&*keccak256(SINGLE_SWAP_SIGNATURE.as_bytes()));

        while current_block <= end_block {
            let to_block = std::cmp::min(current_block + MAX_BLOCK_RANGE - 1, end_block);

            // Create a filter for all swap events at the router address
            let filter = Filter::new()
                .from_block(current_block)
                .to_block(to_block)
                .address(router_address)
                .event_signature(vec![multi_swap_topic, single_swap_topic]);

            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    info!(
                        logs_count = logs.len(),
                        current_block, to_block, "Fetched logs for gas cost calculation"
                    );

                    for log in logs {
                        if let Some(topics) = log.topics().into() {
                            if topics.is_empty() {
                                continue;
                            }

                            let topic = topics[0];

                            if topic == multi_swap_topic {
                                info!("Processing multi swap log");
                                self.handle_multi_swap_log(
                                    &log,
                                    signer_address,
                                    output_token,
                                    &mut result,
                                )
                                .await?;
                            } else if topic == single_swap_topic {
                                info!("Processing single swap log");
                                self.handle_single_swap_log(
                                    &log,
                                    signer_address,
                                    output_token,
                                    &mut result,
                                )
                                .await?;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(
                        error = ?e,
                        current_block,
                        to_block,
                        "Error fetching logs for gas cost block range"
                    );
                }
            }
            current_block = to_block + 1;
        }

        Ok(result)
    }

    async fn handle_multi_swap_log(
        &self,
        log: &Log,
        signer_address: Address,
        output_token: Address,
        result: &mut GasCostResult,
    ) -> anyhow::Result<()> {
        match SwapMulti::decode_log(&log.inner) {
            Ok(event) => {
                match self
                    .process_swap_event(log, &event, signer_address, output_token)
                    .await
                {
                    Ok(Some((gas_used, effective_gas_price))) => {
                        result.add_transaction(gas_used, effective_gas_price);
                    }
                    Ok(None) => {} // Not our signer or doesn't output the specified token
                    Err(e) => {
                        error!(error = ?e, "Error processing SwapMulti event for gas");
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, "Failed to decode SwapMulti log for gas");
            }
        }
        Ok(())
    }

    async fn handle_single_swap_log(
        &self,
        log: &Log,
        signer_address: Address,
        output_token: Address,
        result: &mut GasCostResult,
    ) -> anyhow::Result<()> {
        match Swap::decode_log(&log.inner) {
            Ok(event) => {
                match self
                    .process_single_swap_event(log, &event, signer_address, output_token)
                    .await
                {
                    Ok(Some((gas_used, effective_gas_price))) => {
                        result.add_transaction(gas_used, effective_gas_price);
                    }
                    Ok(None) => {} // Not our signer or doesn't output the specified token
                    Err(e) => {
                        error!(error = ?e, "Error processing Swap event for gas");
                    }
                }
            }
            Err(e) => {
                error!(error = ?e, "Failed to decode Swap log for gas");
            }
        }
        Ok(())
    }

    pub async fn calculate_gas_cost_between_blocks(
        &self,
        chain_id: u64,
        signer_address: Address,
        output_token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        info!(
            ?signer_address,
            start_block, end_block, "Starting gas cost calculation"
        );

        // Check cache and calculate gaps that need to be filled
        let (cached_result, gaps) = {
            let cache = self.gas_cache.lock().await;
            cache.calculate_gaps(chain_id, signer_address, start_block, end_block)
        };

        // If there are no gaps, we can return the cached result
        if let Some(result) = cached_result.clone() {
            if gaps.is_empty() {
                info!(
                    ?signer_address,
                    "Using complete cached result for gas cost block range"
                );
                return Ok(result);
            }
        }

        // Initialize with any cached data or create new result
        let mut gas_data =
            cached_result.unwrap_or_else(|| GasCostResult::new(chain_id, signer_address));

        // Process each gap
        for (gap_start, gap_end) in gaps {
            info!(
                ?signer_address,
                gap_start, gap_end, "Processing uncached block range for gas cost"
            );

            let gap_result = self
                .process_logs_in_range(chain_id, signer_address, output_token, gap_start, gap_end)
                .await?;

            // Cache the gap result
            {
                let mut cache = self.gas_cache.lock().await;
                cache.insert(signer_address, gap_start, gap_end, gap_result.clone());
            }

            // Merge the gap result with our main result
            gas_data.merge(&gap_result);
        }

        // Cache the complete result
        {
            let mut cache = self.gas_cache.lock().await;
            cache.insert(signer_address, start_block, end_block, gas_data.clone());
        }

        info!(
            ?signer_address,
            total_gas_cost = ?gas_data.total_gas_cost,
            transaction_count = gas_data.transaction_count,
            "Finished gas cost calculation"
        );

        Ok(gas_data)
    }
}

/// Query parameters for the gas cost endpoint.
#[derive(Debug, Deserialize)]
pub struct GasQuery {
    pub chain_id: u64,
    pub signer_address: Address,
    pub output_token: Address,
    pub from_block: u64,
    pub to_block: u64,
}

/// Response for the gas cost endpoint.
#[derive(Debug, Serialize)]
pub struct GasResponse {
    pub total_gas_cost: String,
    pub transaction_count: usize,
    pub signer_address: String,
}

/// Handler for the gas cost endpoint.
pub async fn get_gas_cost(
    State(gas_job): State<SemioscanHandle>,
    Query(params): Query<GasQuery>,
) -> Result<Json<GasResponse>, String> {
    info!(params = ?params, "Received gas cost request");

    let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

    gas_job
        .tx
        .send(Command::CalculateGas(CalculateGasCommand {
            router_type: RouterType::V2,
            signer_address: params.signer_address,
            output_token: params.output_token,
            from_block: params.from_block,
            to_block: params.to_block,
            chain_id: params.chain_id,
            responder: responder_tx,
        }))
        .await
        .map_err(|_| "Failed to send gas calculation command".to_string())?;

    match responder_rx.await {
        Ok(Ok(result)) => Ok(Json(GasResponse {
            total_gas_cost: result.total_gas_cost.to_string(),
            transaction_count: result.transaction_count,
            signer_address: result.signer_address.to_string(),
        })),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to receive gas cost response".to_string()),
    }
}
