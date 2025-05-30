use alloy_network::Ethereum;
use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{network::eip2718::Typed2718, Provider};
use alloy_rpc_types::{Filter, Log, TransactionTrait};
use alloy_sol_types::SolEvent;
use axum::{extract::Query, extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{
    CalculateGasCommand, GasCostCalculator, GasCostResult, SemioscanHandle, Transfer,
    MAX_BLOCK_RANGE,
};
use tracing::{error, info};

use crate::Command;

impl GasCostCalculator<Ethereum> {
    async fn process_transfer_event(&self, log: &Log) -> anyhow::Result<Option<(U256, U256)>> {
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
        Ok(None)
    }

    async fn process_logs_in_range(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let mut result = GasCostResult::new(chain_id, from, to);
        let mut current_block = from_block;

        let transfer_signature = "Transfer(address,address,uint256)";
        let transfer_topic = B256::from_slice(&*keccak256(transfer_signature.as_bytes()));

        while current_block <= to_block {
            let to_block = std::cmp::min(current_block + MAX_BLOCK_RANGE - 1, to_block);

            // Create a filter for all swap events at the router address
            let filter = Filter::new()
                .from_block(current_block)
                .to_block(to_block)
                .address(token)
                .event_signature(vec![transfer_topic])
                .topic1(from)
                .topic2(to);

            let logs = self.provider.get_logs(&filter).await?;

            info!(
                logs_count = logs.len(),
                current_block, to_block, "Fetched logs for gas cost calculation"
            );

            for log in &logs {
                match Transfer::decode_log(&log.inner) {
                    Ok(event) => {
                        info!(
                            event = ?event,
                            "Processing Transfer event for gas cost"
                        );

                        self.handle_log(log, &mut result).await?;
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode Transfer log for gas");
                    }
                }
            }
            current_block = to_block + 1;
        }

        Ok(result)
    }

    async fn handle_log(&self, log: &Log, result: &mut GasCostResult) -> anyhow::Result<()> {
        match self.process_transfer_event(log).await {
            Ok(Some((gas_used, effective_gas_price))) => {
                result.add_transaction(gas_used, effective_gas_price);
            }
            Ok(None) => {
                info!("No transfer event found");
            }
            Err(e) => {
                error!(error = ?e, "Error processing SwapMulti event for gas");
            }
        }
        Ok(())
    }

    pub async fn calculate_gas_cost_between_blocks(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        info!(
            chain_id,
            ?from,
            ?to,
            start_block,
            end_block,
            "Starting gas cost calculation"
        );

        // Check cache and calculate gaps that need to be filled
        let (cached_result, gaps) = {
            let cache = self.gas_cache.lock().await;
            cache.calculate_gaps(chain_id, from, to, start_block, end_block)
        };

        // If there are no gaps, we can return the cached result
        if let Some(result) = cached_result.clone() {
            if gaps.is_empty() {
                info!(
                    chain_id,
                    ?from,
                    ?to,
                    "Using complete cached result for gas cost block range"
                );
                return Ok(result);
            }
        }

        // Initialize with any cached data or create new result
        let mut gas_data = cached_result.unwrap_or_else(|| GasCostResult::new(chain_id, from, to));

        // Process each gap
        for (gap_start, gap_end) in gaps {
            info!(
                chain_id,
                ?from,
                ?to,
                gap_start,
                gap_end,
                "Processing uncached block range for gas cost"
            );

            let gap_result = self
                .process_logs_in_range(chain_id, from, to, token, gap_start, gap_end)
                .await?;

            // Cache the gap result
            {
                let mut cache = self.gas_cache.lock().await;
                cache.insert(from, to, gap_start, gap_end, gap_result.clone());
            }

            // Merge the gap result with our main result
            gas_data.merge(&gap_result);
        }

        // Cache the complete result
        {
            let mut cache = self.gas_cache.lock().await;
            cache.insert(from, to, start_block, end_block, gas_data.clone());
        }

        info!(
            chain_id,
            ?from,
            ?to,
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
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
}

/// Response for the gas cost endpoint.
#[derive(Debug, Serialize)]
pub struct GasResponse {
    pub total_gas_cost: String,
    pub transaction_count: usize,
    pub from: String,
    pub to: String,
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
            from: params.from,
            to: params.to,
            token: params.token,
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
            from: result.from.to_string(),
            to: result.to.to_string(),
        })),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to receive gas cost response".to_string()),
    }
}
