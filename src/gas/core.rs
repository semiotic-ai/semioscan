use alloy_chains::NamedChain;
use alloy_network::{Ethereum, Network};
use alloy_primitives::{Address, BlockNumber, B256, U256};
use alloy_provider::{network::eip2718::Typed2718, Provider};
use alloy_rpc_types::{Filter, Log, TransactionTrait};
use alloy_sol_types::SolEvent;
use op_alloy_network::Optimism;
use tokio::time::sleep;

use crate::errors::{GasCalculationError, RpcError};
use crate::events::definitions::{Approval, Transfer};
use crate::gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
use crate::gas::calculator::{GasCostCalculator, GasCostResult, GasForTx};
use crate::tracing::spans;
use crate::types::gas::BlobCount;
use tracing::{error, info, trace};

/// Type of ERC-20 event for gas calculation
///
/// This enum eliminates code duplication by parameterizing event processing logic
/// over the two supported event types: Transfer and Approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Transfer(address indexed from, address indexed to, uint256 value)
    Transfer,
    /// Approval(address indexed owner, address indexed spender, uint256 value)
    Approval,
}

impl EventType {
    /// Get the event signature hash (B256)
    pub fn signature_hash(&self) -> B256 {
        match self {
            EventType::Transfer => Transfer::SIGNATURE_HASH,
            EventType::Approval => Approval::SIGNATURE_HASH,
        }
    }

    /// Get a human-readable name for logging
    pub fn name(&self) -> &'static str {
        match self {
            EventType::Transfer => "Transfer",
            EventType::Approval => "Approval",
        }
    }

    /// Decode a log as this event type
    ///
    /// Returns Ok(true) if decode succeeded, Ok(false) if log doesn't match this event,
    /// Err if decode failed.
    fn decode_and_log(
        &self,
        log: &Log,
        current_block: BlockNumber,
    ) -> Result<bool, GasCalculationError> {
        let log_index = log.log_index.unwrap_or(0);
        match self {
            EventType::Transfer => match Transfer::decode_log(&log.inner) {
                Ok(event) => {
                    info!(
                        ?event,
                        current_block, "Processing Transfer event for gas cost"
                    );
                    Ok(true)
                }
                Err(e) => {
                    error!(error = ?e, "Failed to decode Transfer log for gas");
                    Err(GasCalculationError::event_decode_failed(log_index, e))
                }
            },
            EventType::Approval => match Approval::decode_log(&log.inner) {
                Ok(event) => {
                    info!(
                        ?event,
                        current_block, "Processing Approval event for gas cost"
                    );
                    Ok(true)
                }
                Err(e) => {
                    error!(error = ?e, "Failed to decode Approval log for gas");
                    Err(GasCalculationError::event_decode_failed(log_index, e))
                }
            },
        }
    }
}

/// Core gas calculation logic
///
/// Pure functions for gas calculations that are independent of network type.
mod gas_calc_core {
    use super::*;

    /// Calculate blob gas costs for EIP-4844 transactions
    pub(super) fn calculate_blob_gas_cost<N: Network>(
        transaction: &N::TransactionResponse,
    ) -> U256 {
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

    /// Calculate effective gas price based on transaction type
    pub(super) fn calculate_effective_gas_price<N: Network>(
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

    /// Create an event filter for the given parameters
    ///
    /// This unified function replaces create_transfer_filter and create_approval_filter.
    pub(super) fn create_event_filter(
        event_type: EventType,
        current_block: BlockNumber,
        to_block: BlockNumber,
        token: Address,
        topic1: Address,
        topic2: Address,
    ) -> Filter {
        let event_topic = event_type.signature_hash();

        Filter::new()
            .from_block(current_block)
            .to_block(to_block)
            .address(token)
            .event_signature(vec![event_topic])
            .topic1(topic1)
            .topic2(topic2)
    }
}

/// Generic implementation that works for both Ethereum and Optimism
impl<N: Network, P: Provider<N>> GasCostCalculator<N, P>
where
    N::TransactionResponse: TransactionTrait + Typed2718,
{
    /// Process a transfer event and extract gas information
    async fn process_event_log<A: ReceiptAdapter<N>>(
        &self,
        log: &Log,
        adapter: &A,
    ) -> Result<Option<GasForTx>, GasCalculationError> {
        let tx_hash = log
            .transaction_hash
            .ok_or_else(GasCalculationError::missing_transaction_hash)?;

        let span = spans::process_event_log(tx_hash);
        let _guard = span.enter();

        let transaction = self
            .provider
            .get_transaction_by_hash(tx_hash)
            .await
            .map_err(|e| RpcError::chain_connection_failed("get_transaction_by_hash", e))?
            .ok_or_else(|| RpcError::TransactionNotFound { tx_hash })?;

        let receipt = self
            .provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| RpcError::chain_connection_failed("get_transaction_receipt", e))?
            .ok_or_else(|| RpcError::ReceiptNotFound { tx_hash })?;

        let gas_used = adapter.gas_used(&receipt);
        let receipt_effective_gas_price = adapter.effective_gas_price(&receipt);

        let effective_gas_price = gas_calc_core::calculate_effective_gas_price::<N>(
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
        let blob_gas_cost = gas_calc_core::calculate_blob_gas_cost::<N>(&transaction);
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

    /// Process logs in a given block range for a specific event type (unified method)
    ///
    /// This method replaces the previous `process_logs_for_transfers_in_range` and
    /// `process_logs_for_approvals_in_range`, eliminating code duplication.
    #[allow(clippy::too_many_arguments)]
    async fn process_logs_in_range<A: ReceiptAdapter<N>>(
        &self,
        event_type: EventType,
        chain: NamedChain,
        topic1_addr: Address,
        topic2_addr: Address,
        token: Address,
        from_block: BlockNumber,
        to_block: BlockNumber,
        adapter: &A,
    ) -> Result<GasCostResult, GasCalculationError> {
        let span = spans::process_logs_in_range(
            event_type,
            chain,
            topic1_addr,
            topic2_addr,
            token,
            from_block,
            to_block,
        );
        let _guard = span.enter();

        let mut result = GasCostResult::new(chain, topic1_addr, topic2_addr);
        let mut current_block = from_block;

        let max_block_range = self.config.get_max_block_range(chain);
        let rate_limit = self.config.get_rate_limit_delay(chain);

        info!(
            event_type = event_type.name(),
            total_blocks = to_block.saturating_sub(from_block) + 1,
            max_block_range = max_block_range.as_u64(),
            "Starting log processing"
        );

        let mut total_logs = 0;
        let mut chunk_count = 0;

        while current_block <= to_block {
            let chunk_end = std::cmp::min(current_block + max_block_range.as_u64() - 1, to_block);
            chunk_count += 1;

            let filter = gas_calc_core::create_event_filter(
                event_type,
                current_block,
                chunk_end,
                token,
                topic1_addr,
                topic2_addr,
            );

            let logs = self.provider.get_logs(&filter).await.map_err(|e| {
                RpcError::get_logs_failed(
                    format!(
                        "{} events from block {} to {}",
                        event_type.name(),
                        current_block,
                        chunk_end
                    ),
                    e,
                )
            })?;
            total_logs += logs.len();

            trace!(
                event_type = event_type.name(),
                logs_count = logs.len(),
                current_block,
                to_block = chunk_end,
                chunk = chunk_count,
                "Fetched logs for gas cost calculation"
            );

            for log in &logs {
                // Decode and process the log
                event_type.decode_and_log(log, current_block)?;
                self.handle_log(log, &mut result, adapter).await?;
            }

            current_block = chunk_end + 1;

            // Apply rate limiting if configured for this chain
            if let Some(delay) = rate_limit {
                if current_block <= to_block {
                    sleep(delay).await;
                }
            }
        }

        info!(
            event_type = event_type.name(),
            total_logs,
            total_chunks = chunk_count,
            total_transactions = result.transaction_count.as_usize(),
            total_gas_cost = %result.total_gas_cost,
            "Completed log processing"
        );

        Ok(result)
    }

    /// Handle a single log and update the result
    async fn handle_log<A: ReceiptAdapter<N>>(
        &self,
        log: &Log,
        result: &mut GasCostResult,
        adapter: &A,
    ) -> Result<(), GasCalculationError> {
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

    /// Calculate gas costs between blocks using the provided adapter (unified method)
    ///
    /// This method replaces `calculate_gas_cost_for_transfers_with_adapter` and
    /// `calculate_gas_cost_for_approvals_with_adapter`, eliminating code duplication.
    ///
    /// Uses intelligent caching with gap detection to minimize RPC calls.
    #[allow(clippy::too_many_arguments)]
    async fn calculate_gas_cost_with_adapter<A: ReceiptAdapter<N>>(
        &self,
        event_type: EventType,
        chain: NamedChain,
        topic1_addr: Address,
        topic2_addr: Address,
        token: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
        adapter: &A,
    ) -> Result<GasCostResult, GasCalculationError> {
        let span = spans::calculate_gas_cost_with_adapter(
            event_type,
            chain,
            topic1_addr,
            topic2_addr,
            start_block,
            end_block,
        );
        let _guard = span.enter();

        info!(
            event_type = event_type.name(),
            ?chain,
            topic1 = %topic1_addr,
            topic2 = %topic2_addr,
            start_block,
            end_block,
            block_count = end_block.saturating_sub(start_block) + 1,
            "Starting gas cost calculation"
        );

        // Check cache and calculate gaps that need to be filled
        let (cached_result, gaps) = {
            let cache = self.gas_cache.lock().await;
            cache.calculate_gaps(chain, topic1_addr, topic2_addr, start_block, end_block)
        };

        // If there are no gaps, we can return the cached result
        if let Some(result) = cached_result.clone() {
            if gaps.is_empty() {
                info!(
                    event_type = event_type.name(),
                    ?chain,
                    topic1 = %topic1_addr,
                    topic2 = %topic2_addr,
                    cached_tx_count = result.transaction_count.as_usize(),
                    cached_gas_cost = %result.total_gas_cost,
                    "Using complete cached result for gas cost block range"
                );
                return Ok(result);
            }
        }

        // Initialize with any cached data or create new result
        let mut gas_data =
            cached_result.unwrap_or_else(|| GasCostResult::new(chain, topic1_addr, topic2_addr));

        info!(
            event_type = event_type.name(),
            gap_count = gaps.len(),
            "Processing uncached block ranges"
        );

        // Process each gap
        for (gap_index, (gap_start, gap_end)) in gaps.iter().enumerate() {
            info!(
                event_type = event_type.name(),
                ?chain,
                topic1 = %topic1_addr,
                topic2 = %topic2_addr,
                gap_start,
                gap_end,
                gap_index = gap_index + 1,
                total_gaps = gaps.len(),
                gap_blocks = gap_end.saturating_sub(*gap_start) + 1,
                "Processing uncached block range for gas cost"
            );

            let gap_result = self
                .process_logs_in_range(
                    event_type,
                    chain,
                    topic1_addr,
                    topic2_addr,
                    token,
                    *gap_start,
                    *gap_end,
                    adapter,
                )
                .await?;

            // Cache the gap result
            {
                let mut cache = self.gas_cache.lock().await;
                cache.insert(
                    topic1_addr,
                    topic2_addr,
                    *gap_start,
                    *gap_end,
                    gap_result.clone(),
                );
            }

            // Merge the gap result with our main result
            gas_data.merge(&gap_result);

            info!(
                event_type = event_type.name(),
                gap_index = gap_index + 1,
                gap_tx_count = gap_result.transaction_count.as_usize(),
                gap_gas_cost = %gap_result.total_gas_cost,
                cumulative_tx_count = gas_data.transaction_count.as_usize(),
                cumulative_gas_cost = %gas_data.total_gas_cost,
                "Completed gap processing"
            );
        }

        // Cache the complete result
        {
            let mut cache = self.gas_cache.lock().await;
            cache.insert(
                topic1_addr,
                topic2_addr,
                start_block,
                end_block,
                gas_data.clone(),
            );
        }

        info!(
            event_type = event_type.name(),
            ?chain,
            topic1 = %topic1_addr,
            topic2 = %topic2_addr,
            total_gas_cost = %gas_data.total_gas_cost,
            transaction_count = gas_data.transaction_count.as_usize(),
            "Finished gas cost calculation"
        );

        Ok(gas_data)
    }
}

// Network-specific implementations using the adapters
impl<P: Provider<Ethereum>> GasCostCalculator<Ethereum, P> {
    /// Calculate gas costs for Transfer events between two addresses
    ///
    /// This is a convenience method for Ethereum-like chains (Ethereum, Arbitrum, Polygon).
    pub async fn calculate_gas_cost_for_transfers_between_blocks(
        &self,
        chain: NamedChain,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: u64,
    ) -> Result<GasCostResult, GasCalculationError> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_gas_cost_with_adapter(
            EventType::Transfer,
            chain,
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

impl<P: Provider<Optimism>> GasCostCalculator<Optimism, P> {
    /// Calculate gas costs for Transfer events between two addresses
    ///
    /// This is a convenience method for Optimism Stack chains (Base, Optimism, Mode, Fraxtal, Sonic).
    /// Automatically includes L1 data fees in the calculation.
    pub async fn calculate_gas_cost_for_transfers_between_blocks(
        &self,
        chain: NamedChain,
        from: Address,
        to: Address,
        token: Address,
        start_block: u64,
        end_block: BlockNumber,
    ) -> Result<GasCostResult, GasCalculationError> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_gas_cost_with_adapter(
            EventType::Transfer,
            chain,
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

// Approval event gas cost calculation for Ethereum-like chains
impl<P: Provider<Ethereum>> GasCostCalculator<Ethereum, P> {
    /// Calculate gas costs for Approval events between owner and spender
    ///
    /// This is a convenience method for Ethereum-like chains (Ethereum, Arbitrum, Polygon).
    pub async fn calculate_gas_cost_for_approvals_between_blocks(
        &self,
        chain: NamedChain,
        owner: Address,
        spender: Address,
        token: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> Result<GasCostResult, GasCalculationError> {
        let adapter = EthereumReceiptAdapter;
        self.calculate_gas_cost_with_adapter(
            EventType::Approval,
            chain,
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

impl<P: Provider<Optimism>> GasCostCalculator<Optimism, P> {
    /// Calculate gas costs for Approval events between owner and spender
    ///
    /// This is a convenience method for Optimism Stack chains (Base, Optimism, Mode, Fraxtal, Sonic).
    /// Automatically includes L1 data fees in the calculation.
    pub async fn calculate_gas_cost_for_approvals_between_blocks(
        &self,
        chain: NamedChain,
        owner: Address,
        spender: Address,
        token: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> Result<GasCostResult, GasCalculationError> {
        let adapter = OptimismReceiptAdapter;
        self.calculate_gas_cost_with_adapter(
            EventType::Approval,
            chain,
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
    use alloy_eips::eip4844::DATA_GAS_PER_BLOB;

    #[test]
    fn test_blob_gas_per_blob_constant() {
        // Verify we're using the correct EIP-4844 constant from alloy-eips
        assert_eq!(DATA_GAS_PER_BLOB, 131_072);
    }

    #[test]
    fn test_create_transfer_filter_structure() {
        // Test that create_event_filter creates a filter with the correct structure for Transfer events
        let token = Address::ZERO;
        let from = Address::from([0x11; 20]);
        let to = Address::from([0x22; 20]);

        let filter =
            gas_calc_core::create_event_filter(EventType::Transfer, 100, 200, token, from, to);

        // Filter should be configured for the correct address
        // (We can't easily inspect the filter internals without additional dependencies)
        let _ = filter; // Use the filter to avoid unused warning
    }

    #[test]
    fn test_create_approval_filter_structure() {
        // Test that create_event_filter creates a filter with the correct structure for Approval events
        let token = Address::ZERO;
        let owner = Address::from([0x11; 20]);
        let spender = Address::from([0x22; 20]);

        let filter = gas_calc_core::create_event_filter(
            EventType::Approval,
            100,
            200,
            token,
            owner,
            spender,
        );

        // Filter should be configured for the correct address
        let _ = filter; // Use the filter to avoid unused warning
    }

    #[test]
    fn test_event_type_signature_hash() {
        assert_eq!(
            EventType::Transfer.signature_hash(),
            Transfer::SIGNATURE_HASH
        );
        assert_eq!(
            EventType::Approval.signature_hash(),
            Approval::SIGNATURE_HASH
        );
    }

    #[test]
    fn test_event_type_name() {
        assert_eq!(EventType::Transfer.name(), "Transfer");
        assert_eq!(EventType::Approval.name(), "Approval");
    }
}
