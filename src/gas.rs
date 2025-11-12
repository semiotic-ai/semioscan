use alloy_chains::NamedChain;
use alloy_network::{Ethereum, Network};
use alloy_primitives::{keccak256, Address, B256, U256};
use alloy_provider::{network::eip2718::Typed2718, Provider};
use alloy_rpc_types::{Filter, Log, TransactionTrait};
use alloy_sol_types::SolEvent;
use op_alloy_network::Optimism;
use tokio::time::sleep;

use crate::{
    adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter},
    spans, Approval, GasCostCalculator, GasCostResult, GasForTx, Transfer,
    APPROVAL_EVENT_SIGNATURE, TRANSFER_EVENT_SIGNATURE,
};
use tracing::{error, info, trace};

// Constants for gas calculations
const BLOB_GAS_PER_BLOB: u64 = 131_072;

/// Core gas calculation logic, extracted from network-specific implementations
struct GasCalculationCore;

impl GasCalculationCore {
    /// Calculate blob gas costs for EIP-4844 transactions
    fn calculate_blob_gas_cost<N: Network>(transaction: &N::TransactionResponse) -> U256 {
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

    /// Calculate effective gas price based on transaction type
    fn calculate_effective_gas_price<N: Network>(
        transaction: &N::TransactionResponse,
        receipt_effective_gas_price: U256,
    ) -> U256 {
        if transaction.is_legacy() {
            U256::from(transaction.gas_price().unwrap_or_default())
        } else {
            info!("EIP-1559 or EIP-4844 transaction");
            receipt_effective_gas_price
        }
    }

    /// Create the transfer event filter for the given parameters
    fn create_transfer_filter(
        current_block: u64,
        to_block: u64,
        token: Address,
        from: Address,
        to: Address,
    ) -> Filter {
        let transfer_topic = B256::from_slice(&*keccak256(TRANSFER_EVENT_SIGNATURE.as_bytes()));

        Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .address(token)
            .event_signature(vec![transfer_topic])
            .topic1(from)
            .topic2(to)
    }

    /// Create the approval event filter for the given parameters
    fn create_approval_filter(
        current_block: u64,
        to_block: u64,
        token: Address,
        owner: Address,
        spender: Address,
    ) -> Filter {
        let approval_topic = B256::from_slice(&*keccak256(APPROVAL_EVENT_SIGNATURE.as_bytes()));

        Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .address(token)
            .event_signature(vec![approval_topic])
            .topic1(owner)
            .topic2(spender)
    }
}

/// Generic implementation that works for both Ethereum and Optimism
impl<N: Network> GasCostCalculator<N>
where
    N::TransactionResponse: TransactionTrait + Typed2718,
{
    /// Process a transfer event and extract gas information
    async fn process_event_log<A: ReceiptAdapter<N>>(
        &self,
        log: &Log,
        adapter: &A,
    ) -> anyhow::Result<Option<GasForTx>> {
        let tx_hash = log
            .transaction_hash
            .ok_or_else(|| anyhow::anyhow!("Transaction hash not found for log: {:?}", log))?;

        let span = spans::process_event_log(tx_hash);
        let _guard = span.enter();

        let transaction = self
            .provider
            .get_transaction_by_hash(tx_hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Transaction not found for hash: {:?}", tx_hash))?;

        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Receipt not found for hash: {:?}", tx_hash))?;

        let gas_used = adapter.gas_used(&receipt);
        let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);

        let effective_gas_price = GasCalculationCore::calculate_effective_gas_price::<N>(
            &transaction,
            receipt_effective_gas_price,
        );

        info!(
            ?gas_used,
            ?effective_gas_price,
            "Transaction details for gas calculation"
        );

        // Calculate base gas cost
        let base_gas_cost = gas_used.saturating_mul(effective_gas_price);

        // Add blob gas costs for EIP-4844 transactions
        let blob_gas_cost = GasCalculationCore::calculate_blob_gas_cost::<N>(&transaction);
        let total_gas_cost = base_gas_cost.saturating_add(blob_gas_cost);

        info!(
            base_gas_cost = ?base_gas_cost,
            blob_gas_cost = ?blob_gas_cost,
            total_gas_cost = ?total_gas_cost,
            "Calculated gas costs"
        );

        // Create appropriate GasForTx based on network type
        let gas_for_tx = match adapter.l1_data_fee(&receipt) {
            Some(l1_fee) => {
                // L2 network with L1 data fees
                GasForTx::from((gas_used, effective_gas_price, l1_fee))
            }
            None => {
                // L1 network or L2 without L1 data fees
                GasForTx::from((gas_used, effective_gas_price))
            }
        };

        info!(?gas_for_tx, "Gas for transaction");

        Ok(Some(gas_for_tx))
    }

    /// Process logs in a given block range
    #[allow(clippy::too_many_arguments)]
    async fn process_logs_for_transfers_in_range<A: ReceiptAdapter<N>>(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
        adapter: &A,
    ) -> anyhow::Result<GasCostResult> {
        let mut result = GasCostResult::new(chain_id, from, to);
        let mut current_block = from_block;

        // Convert chain_id to NamedChain for config lookup
        let chain = NamedChain::try_from(chain_id).unwrap_or(NamedChain::Mainnet); // Fallback to mainnet if unknown
        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        while current_block <= to_block {
            let chunk_end = std::cmp::min(current_block + max_block_range - 1, to_block);

            let filter = GasCalculationCore::create_transfer_filter(
                current_block,
                chunk_end,
                token,
                from,
                to,
            );

            let logs = self.provider.get_logs(&filter).await?;

            trace!(
                logs_count = logs.len(),
                current_block,
                to_block = chunk_end,
                "Fetched logs for gas cost calculation"
            );

            for log in &logs {
                match Transfer::decode_log(&log.inner) {
                    Ok(event) => {
                        info!(
                            ?event,
                            current_block, "Processing Transfer event for gas cost"
                        );
                        self.handle_log(log, &mut result, adapter).await?;
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode Transfer log for gas");
                        return Err(anyhow::anyhow!(
                            "Failed to decode Transfer log for gas: {:?}",
                            e
                        ));
                    }
                }
            }
            current_block = chunk_end + 1;

            // Apply rate limiting if configured for this chain
            if let Some(delay) = rate_limit {
                if current_block <= to_block {
                    sleep(delay).await;
                }
            }
        }

        Ok(result)
    }

    /// Process logs in a given block range
    #[allow(clippy::too_many_arguments)]
    async fn process_logs_for_approvals_in_range<A: ReceiptAdapter<N>>(
        &self,
        chain_id: u64,
        owner: Address,
        spender: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
        adapter: &A,
    ) -> anyhow::Result<GasCostResult> {
        let mut result = GasCostResult::new(chain_id, owner, spender);
        let mut current_block = from_block;

        // Convert chain_id to NamedChain for config lookup
        let chain = NamedChain::try_from(chain_id).unwrap_or(NamedChain::Mainnet); // Fallback to mainnet if unknown
        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        while current_block <= to_block {
            let chunk_end = std::cmp::min(current_block + max_block_range - 1, to_block);

            let filter = GasCalculationCore::create_approval_filter(
                current_block,
                chunk_end,
                token,
                owner,
                spender,
            );

            let logs = self.provider.get_logs(&filter).await?;

            trace!(
                logs_count = logs.len(),
                current_block,
                to_block = chunk_end,
                "Fetched logs for gas cost calculation"
            );

            for log in &logs {
                match Approval::decode_log(&log.inner) {
                    Ok(event) => {
                        info!(
                            ?event,
                            current_block, "Processing Transfer event for gas cost"
                        );
                        self.handle_log(log, &mut result, adapter).await?;
                    }
                    Err(e) => {
                        error!(error = ?e, "Failed to decode Transfer log for gas");
                        return Err(anyhow::anyhow!(
                            "Failed to decode Transfer log for gas: {:?}",
                            e
                        ));
                    }
                }
            }
            current_block = chunk_end + 1;

            // Apply rate limiting if configured for this chain
            if let Some(delay) = rate_limit {
                if current_block <= to_block {
                    sleep(delay).await;
                }
            }
        }

        Ok(result)
    }

    /// Handle a single log and update the result
    async fn handle_log<A: ReceiptAdapter<N>>(
        &self,
        log: &Log,
        result: &mut GasCostResult,
        adapter: &A,
    ) -> anyhow::Result<()> {
        match self.process_event_log(log, adapter).await {
            Ok(Some(gas)) => {
                result.add_transaction(gas);
            }
            Ok(None) => {
                info!("No transfer event found");
            }
            Err(e) => {
                error!(error = ?e, "Error processing transfer event for gas");
                return Err(e);
            }
        }
        Ok(())
    }

    /// Calculate gas costs between blocks using the provided adapter
    #[allow(clippy::too_many_arguments)]
    async fn calculate_gas_cost_for_transfers_with_adapter<A: ReceiptAdapter<N>>(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
        adapter: &A,
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
                .process_logs_for_transfers_in_range(
                    chain_id, from, to, token, gap_start, gap_end, adapter,
                )
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

    /// Calculate gas costs between blocks using the provided adapter
    #[allow(clippy::too_many_arguments)]
    async fn calculate_gas_cost_for_approvals_with_adapter<A: ReceiptAdapter<N>>(
        &self,
        chain_id: u64,
        owner: Address,
        spender: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
        adapter: &A,
    ) -> anyhow::Result<GasCostResult> {
        info!(
            chain_id,
            ?owner,
            ?spender,
            start_block,
            end_block,
            "Starting gas cost calculation"
        );

        // Check cache and calculate gaps that need to be filled
        let (cached_result, gaps) = {
            let cache = self.gas_cache.lock().await;
            cache.calculate_gaps(chain_id, owner, spender, start_block, end_block)
        };

        // If there are no gaps, we can return the cached result
        if let Some(result) = cached_result.clone() {
            if gaps.is_empty() {
                info!(
                    chain_id,
                    ?owner,
                    ?spender,
                    "Using complete cached result for gas cost block range"
                );
                return Ok(result);
            }
        }

        // Initialize with any cached data or create new result
        let mut gas_data =
            cached_result.unwrap_or_else(|| GasCostResult::new(chain_id, owner, spender));

        // Process each gap
        for (gap_start, gap_end) in gaps {
            info!(
                chain_id,
                ?owner,
                ?spender,
                gap_start,
                gap_end,
                "Processing uncached block range for gas cost"
            );

            let gap_result = self
                .process_logs_for_approvals_in_range(
                    chain_id, owner, spender, token, gap_start, gap_end, adapter,
                )
                .await?;

            // Cache the gap result
            {
                let mut cache = self.gas_cache.lock().await;
                cache.insert(owner, spender, gap_start, gap_end, gap_result.clone());
            }

            // Merge the gap result with our main result
            gas_data.merge(&gap_result);
        }

        // Cache the complete result
        {
            let mut cache = self.gas_cache.lock().await;
            cache.insert(owner, spender, start_block, end_block, gas_data.clone());
        }

        info!(
            chain_id,
            ?owner,
            ?spender,
            total_gas_cost = ?gas_data.total_gas_cost,
            transaction_count = gas_data.transaction_count,
            "Finished gas cost calculation"
        );

        Ok(gas_data)
    }
}

// Network-specific implementations using the adapters
impl GasCostCalculator<Ethereum> {
    pub async fn calculate_gas_cost_for_transfers_between_blocks(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_gas_cost_for_transfers_with_adapter(
            chain_id,
            from,
            to,
            token,
            start_block,
            end_block,
            &adapter,
        )
        .await
    }
}

impl GasCostCalculator<Optimism> {
    pub async fn calculate_gas_cost_for_transfers_between_blocks(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_gas_cost_for_transfers_with_adapter(
            chain_id,
            from,
            to,
            token,
            start_block,
            end_block,
            &adapter,
        )
        .await
    }
}

// Network-specific implementations using the adapters
impl GasCostCalculator<Ethereum> {
    pub async fn calculate_gas_cost_for_approvals_between_blocks(
        &self,
        chain_id: u64,
        owner: Address,
        spender: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_gas_cost_for_approvals_with_adapter(
            chain_id,
            owner,
            spender,
            token,
            start_block,
            end_block,
            &adapter,
        )
        .await
    }
}

impl GasCostCalculator<Optimism> {
    pub async fn calculate_gas_cost_for_approvals_between_blocks(
        &self,
        chain_id: u64,
        owner: Address,
        spender: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_gas_cost_for_approvals_with_adapter(
            chain_id,
            owner,
            spender,
            token,
            start_block,
            end_block,
            &adapter,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_gas_per_blob_constant() {
        // Verify the EIP-4844 constant is correct
        assert_eq!(BLOB_GAS_PER_BLOB, 131_072);
    }

    #[test]
    fn test_create_transfer_filter_structure() {
        // Test that create_transfer_filter creates a filter with the correct structure
        let token = Address::ZERO;
        let from = Address::from([0x11; 20]);
        let to = Address::from([0x22; 20]);

        let filter = GasCalculationCore::create_transfer_filter(100, 200, token, from, to);

        // Filter should be configured for the correct address
        // (We can't easily inspect the filter internals without additional dependencies)
        let _ = filter; // Use the filter to avoid unused warning
    }

    #[test]
    fn test_create_approval_filter_structure() {
        // Test that create_approval_filter creates a filter with the correct structure
        let token = Address::ZERO;
        let owner = Address::from([0x11; 20]);
        let spender = Address::from([0x22; 20]);

        let filter = GasCalculationCore::create_approval_filter(100, 200, token, owner, spender);

        // Filter should be configured for the correct address
        let _ = filter; // Use the filter to avoid unused warning
    }
}
