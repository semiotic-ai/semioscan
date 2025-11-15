use alloy_chains::NamedChain;
use alloy_network::{eip2718::Typed2718, Ethereum, Network};
use alloy_primitives::{Address, BlockNumber, TxHash, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use bigdecimal::BigDecimal;
use op_alloy_network::Optimism;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{error, info, trace, warn};

use crate::config::constants::stablecoins::BSC_BINANCE_PEG_USDC;
use crate::config::SemioscanConfig;
use crate::events::definitions::Transfer;
use crate::gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
use crate::tracing::spans;
use crate::types::config::TransactionCount;
use crate::types::gas::{BlobCount, GasAmount, GasPrice};

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
        current_block: BlockNumber,
        to_block: BlockNumber,
        token_address: Address,
        from_address: Address, // topic1
        to_address: Address,   // topic2
    ) -> Filter {
        let transfer_topic_hash = Transfer::SIGNATURE_HASH;
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
    pub tx_hash: TxHash,
    pub block_number: BlockNumber,
    pub gas_used: GasAmount,           // L2 gas used
    pub effective_gas_price: GasPrice, // L2 effective gas price
    pub l1_fee: Option<U256>,          // L1 data fee for L2s
    pub blob_gas_cost: U256,           // Cost from EIP-4844 blobs
    pub transferred_amount: U256,
}

impl GasAndAmountForTx {
    /// Calculates the total gas cost for this transaction, including L2 gas, L1 fee, and blob gas.
    pub fn total_gas_cost(&self) -> U256 {
        let l2_execution_cost = self.gas_used * self.effective_gas_price;
        let total_cost = l2_execution_cost.saturating_add(self.blob_gas_cost);
        total_cost.saturating_add(self.l1_fee.unwrap_or_default())
    }

    /// Convert to display format with custom token decimal precision
    pub fn to_display(&self, token_precision: DecimalPrecision) -> GasAndAmountDisplay {
        let l2_execution_cost = self.gas_used * self.effective_gas_price;
        let total_cost = l2_execution_cost
            .saturating_add(self.blob_gas_cost)
            .saturating_add(self.l1_fee.unwrap_or_default());

        GasAndAmountDisplay {
            tx_hash: self.tx_hash.to_string(),
            gas_used: self.gas_used.to_string(),
            effective_gas_price_gwei: format_wei_to_gwei(self.effective_gas_price.as_u256()),
            l1_fee_eth: self.l1_fee.map(format_wei_to_eth),
            blob_gas_cost_eth: format_wei_to_eth(self.blob_gas_cost),
            total_gas_cost_eth: format_wei_to_eth(total_cost),
            transferred_amount_usdc: format_token_amount(self.transferred_amount, token_precision),
        }
    }
}

/// Aggregated result for combined data retrieval over a block range.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinedDataResult {
    pub chain: NamedChain,
    pub from_address: Address,
    pub to_address: Address,
    pub token_address: Address,
    pub total_l2_execution_cost: U256,
    pub total_blob_gas_cost: U256,
    pub total_l1_fee: U256,
    pub overall_total_gas_cost: U256,
    pub total_amount_transferred: U256,
    pub transaction_count: TransactionCount,
    pub transactions_data: Vec<GasAndAmountForTx>,
}

/// Human-readable version of CombinedDataResult with formatted values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedDataDisplay {
    pub chain: NamedChain,
    pub from_address: String,
    pub to_address: String,
    pub token_address: String,
    pub total_l2_execution_cost_eth: String,
    pub total_blob_gas_cost_eth: String,
    pub total_l1_fee_eth: String,
    pub overall_total_gas_cost_eth: String,
    pub total_amount_transferred_usdc: String,
    pub transaction_count: TransactionCount,
    pub transactions_data: Vec<GasAndAmountDisplay>,
}

/// Human-readable version of GasAndAmountForTx
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasAndAmountDisplay {
    pub tx_hash: String,
    pub gas_used: String,
    pub effective_gas_price_gwei: String,
    pub l1_fee_eth: Option<String>,
    pub blob_gas_cost_eth: String,
    pub total_gas_cost_eth: String,
    pub transferred_amount_usdc: String,
}

/// Convert wei (U256) to ETH string with 18 decimals
fn format_wei_to_eth(wei: U256) -> String {
    // ETH has 18 decimals
    let eth_divisor = U256::from(1_000_000_000_000_000_000u128); // 10^18
    let eth_whole = wei / eth_divisor;
    let eth_fractional = wei % eth_divisor;

    // Format with 18 decimal places, removing trailing zeros
    let fractional_str = format!("{:018}", eth_fractional);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", eth_whole)
    } else {
        // Always use decimal notation, never scientific notation
        format!("{}.{}", eth_whole, trimmed)
    }
}

/// Convert wei (U256) to Gwei string
fn format_wei_to_gwei(wei: U256) -> String {
    // Gwei has 9 decimals (10^9 wei = 1 Gwei)
    let gwei_divisor = U256::from(1_000_000_000u64); // 10^9
    let gwei_whole = wei / gwei_divisor;
    let gwei_fractional = wei % gwei_divisor;

    // Format with 9 decimal places, removing trailing zeros
    let fractional_str = format!("{:09}", gwei_fractional);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", gwei_whole)
    } else {
        format!("{}.{}", gwei_whole, trimmed)
    }
}

/// Convert token raw amount (U256) to string with specified decimal precision
/// Most chains use 6 decimals for USDC, but BNB Chain uses 18 decimals
fn format_token_amount(raw_amount: U256, precision: DecimalPrecision) -> String {
    let decimals = precision.decimals();
    if decimals == 0 {
        return raw_amount.to_string();
    }

    // Calculate divisor: 10^decimals
    let divisor = U256::from(10u64).pow(U256::from(decimals));
    let whole = raw_amount / divisor;
    let fractional = raw_amount % divisor;

    // Format with correct decimal places, removing trailing zeros
    let fractional_str = format!("{:0width$}", fractional, width = decimals as usize);
    let trimmed = fractional_str.trim_end_matches('0');

    if trimmed.is_empty() {
        format!("{}", whole)
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

/// Decimal precision for blockchain values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecimalPrecision {
    /// USDC and most stablecoins use 6 decimals
    Usdc,
    /// BSC Binance-Peg USDC uses 18 decimals (non-standard)
    BinancePegUsdc,
    /// Native tokens (ETH, BNB, MATIC, etc.) and gas costs use 18 decimals
    NativeToken,
}

impl DecimalPrecision {
    /// Get the number of decimals as a u8
    pub fn decimals(self) -> u8 {
        match self {
            DecimalPrecision::Usdc => 6,
            DecimalPrecision::BinancePegUsdc => 18,
            DecimalPrecision::NativeToken => 18,
        }
    }
}

/// Get the decimal precision for a specific token on a specific chain.
/// Native tokens (Address::ZERO) use 18 decimals.
/// Most USDC tokens use 6 decimals, but BSC Binance-Peg USDC uses 18 decimals.
///
/// # Arguments
/// * `chain` - The named chain
/// * `token_address` - The token contract address (Address::ZERO for native token)
///
/// # Returns
/// The appropriate DecimalPrecision for this token
pub fn get_token_decimal_precision(chain: NamedChain, token_address: Address) -> DecimalPrecision {
    // Native token (ETH, BNB, MATIC, etc.) uses 18 decimals
    if token_address == Address::ZERO {
        return DecimalPrecision::NativeToken;
    }

    // BSC Binance-Peg USDC has 18 decimals instead of 6
    if matches!(chain, NamedChain::BinanceSmartChain) && token_address == BSC_BINANCE_PEG_USDC {
        DecimalPrecision::BinancePegUsdc // 18 decimals
    } else {
        DecimalPrecision::Usdc // 6 decimals
    }
}

/// Convert U256 to BigDecimal with decimal scaling for database storage.
/// This function properly handles large decimal places (like 18 for ETH) without overflow.
///
/// # Arguments
/// * `value` - The raw U256 value (e.g., wei for ETH, smallest unit for tokens)
/// * `precision` - The decimal precision (Usdc = 6, BinancePegUsdc = 18, NativeToken = 18)
///
/// # Returns
/// A BigDecimal representing the human-readable value
///
/// # Example
/// ```ignore
/// let wei = U256::from(1_000_000_000_000_000_000u128); // 1 ETH in wei
/// let eth = u256_to_bigdecimal(wei, DecimalPrecision::NativeToken); // Returns BigDecimal "1.0"
/// ```
pub fn u256_to_bigdecimal(value: U256, precision: DecimalPrecision) -> BigDecimal {
    // Use U256 divisor to avoid i64 overflow for large exponents
    let divisor = match precision {
        DecimalPrecision::Usdc => U256::from(1_000_000u64), // 10^6
        DecimalPrecision::BinancePegUsdc | DecimalPrecision::NativeToken => {
            U256::from(1_000_000_000_000_000_000u128) // 10^18
        }
    };

    // Perform division in U256 space to get whole and fractional parts
    let whole = value / divisor;
    let fractional = value % divisor;

    // Convert to BigDecimal
    let whole_decimal =
        BigDecimal::from_str(&whole.to_string()).unwrap_or_else(|_| BigDecimal::from(0));
    let fractional_decimal =
        BigDecimal::from_str(&fractional.to_string()).unwrap_or_else(|_| BigDecimal::from(0));
    let divisor_decimal =
        BigDecimal::from_str(&divisor.to_string()).unwrap_or_else(|_| BigDecimal::from(1));

    whole_decimal + (fractional_decimal / divisor_decimal)
}

impl CombinedDataResult {
    pub fn new(
        chain: NamedChain,
        from_address: Address,
        to_address: Address,
        token_address: Address,
    ) -> Self {
        Self {
            chain,
            from_address,
            to_address,
            token_address,
            total_l2_execution_cost: U256::ZERO,
            total_blob_gas_cost: U256::ZERO,
            total_l1_fee: U256::ZERO,
            overall_total_gas_cost: U256::ZERO,
            total_amount_transferred: U256::ZERO,
            transaction_count: TransactionCount::ZERO,
            transactions_data: Vec::new(),
        }
    }

    /// Convert to display format with custom token decimal precision
    pub fn to_display(&self, token_precision: DecimalPrecision) -> CombinedDataDisplay {
        CombinedDataDisplay {
            chain: self.chain,
            from_address: format!("{:#x}", self.from_address),
            to_address: format!("{:#x}", self.to_address),
            token_address: format!("{:#x}", self.token_address),
            total_l2_execution_cost_eth: format_wei_to_eth(self.total_l2_execution_cost),
            total_blob_gas_cost_eth: format_wei_to_eth(self.total_blob_gas_cost),
            total_l1_fee_eth: format_wei_to_eth(self.total_l1_fee),
            overall_total_gas_cost_eth: format_wei_to_eth(self.overall_total_gas_cost),
            total_amount_transferred_usdc: format_token_amount(
                self.total_amount_transferred,
                token_precision,
            ),
            transaction_count: self.transaction_count,
            transactions_data: self
                .transactions_data
                .iter()
                .map(|tx| tx.to_display(token_precision))
                .collect(),
        }
    }

    pub fn add_transaction_data(&mut self, data: GasAndAmountForTx) {
        self.total_amount_transferred = self
            .total_amount_transferred
            .saturating_add(data.transferred_amount);

        let l2_execution_cost = data.gas_used * data.effective_gas_price;
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
        self.transaction_count.increment();
    }

    pub fn merge(&mut self, other: &CombinedDataResult) {
        if self.chain != other.chain
            || self.from_address != other.from_address
            || self.to_address != other.to_address
            || self.token_address != other.token_address
        {
            error!(self_params = ?(self.chain, self.from_address, self.to_address, self.token_address),
                   other_params = ?(other.chain, other.from_address, other.to_address, other.token_address),
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
    ) -> anyhow::Result<Option<GasAndAmountForTx>> {
        let tx_hash = rpc_log_entry.transaction_hash.ok_or_else(|| {
            anyhow::anyhow!("Transaction hash not found for log: {:?}", rpc_log_entry)
        })?;

        let span = spans::process_log_for_combined_data(tx_hash);
        let _guard = span.enter();

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

        let block_number = rpc_log_entry.block_number.ok_or_else(|| {
            anyhow::anyhow!("Block number not found for log: {:?}", rpc_log_entry)
        })?;

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
    ) -> anyhow::Result<CombinedDataResult> {
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
    ) -> anyhow::Result<CombinedDataResult> {
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
    ) -> anyhow::Result<CombinedDataResult> {
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
    ) -> anyhow::Result<CombinedDataResult> {
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
    use alloy_primitives::b256;

    /// Helper to create a test transaction with specified values
    fn create_test_tx(
        gas_used: u64,
        effective_gas_price: u64,
        l1_fee: Option<u64>,
        blob_gas_cost: u64,
        transferred_amount: u64,
    ) -> GasAndAmountForTx {
        GasAndAmountForTx {
            tx_hash: b256!("0000000000000000000000000000000000000000000000000000000000000001"),
            block_number: 1000,
            gas_used: GasAmount::new(gas_used),
            effective_gas_price: GasPrice::new(effective_gas_price),
            l1_fee: l1_fee.map(U256::from),
            blob_gas_cost: U256::from(blob_gas_cost),
            transferred_amount: U256::from(transferred_amount),
        }
    }

    #[test]
    fn test_total_gas_cost_l1_only() {
        // L1 transaction: gas_used * effective_gas_price
        let tx = create_test_tx(
            21000, // gas_used
            50,    // effective_gas_price (50 Gwei)
            None,  // no L1 fee
            0,     // no blob gas
            1000,  // transferred amount (irrelevant for this test)
        );

        let total = tx.total_gas_cost();
        let expected = U256::from(21000) * U256::from(50); // 1,050,000
        assert_eq!(
            total, expected,
            "L1 transaction cost should be gas_used * effective_gas_price"
        );
    }

    #[test]
    fn test_total_gas_cost_l2_with_l1_fee() {
        // L2 transaction: (gas_used * effective_gas_price) + l1_fee
        let tx = create_test_tx(
            100000,     // gas_used
            10,         // effective_gas_price (10 Gwei)
            Some(5000), // L1 data fee
            0,          // no blob gas
            2000,
        );

        let total = tx.total_gas_cost();
        let l2_execution = U256::from(100000) * U256::from(10); // 1,000,000
        let expected = l2_execution + U256::from(5000); // 1,005,000
        assert_eq!(total, expected, "L2 transaction should include L1 fee");
    }

    #[test]
    fn test_total_gas_cost_with_blob_gas() {
        // EIP-4844 transaction with blob gas costs
        let tx = create_test_tx(
            50000, // gas_used
            20,    // effective_gas_price
            None,  // no L1 fee (L1 chain)
            10000, // blob gas cost
            3000,
        );

        let total = tx.total_gas_cost();
        let base_cost = U256::from(50000) * U256::from(20); // 1,000,000
        let expected = base_cost + U256::from(10000); // 1,010,000
        assert_eq!(total, expected, "Should include blob gas cost");
    }

    #[test]
    fn test_total_gas_cost_l2_with_all_components() {
        // L2 transaction with all cost components
        let tx = create_test_tx(
            75000,      // gas_used
            15,         // effective_gas_price
            Some(8000), // L1 data fee
            12000,      // blob gas cost
            5000,
        );

        let total = tx.total_gas_cost();
        let l2_execution = U256::from(75000) * U256::from(15); // 1,125,000
        let expected = l2_execution + U256::from(12000) + U256::from(8000); // 1,145,000
        assert_eq!(total, expected, "Should include all cost components");
    }

    #[test]
    fn test_total_gas_cost_zero_values() {
        // Edge case: all zeros
        let tx = create_test_tx(0, 0, None, 0, 0);

        let total = tx.total_gas_cost();
        assert_eq!(total, U256::ZERO, "Zero inputs should give zero cost");
    }

    #[test]
    fn test_total_gas_cost_large_values() {
        // Test with large values to ensure no overflow
        let large_gas = 10_000_000_u64;
        let large_price = 1_000_u64;

        let tx = create_test_tx(large_gas, large_price, Some(1_000_000), 500_000, 100_000);

        let total = tx.total_gas_cost();
        let expected_l2_cost = U256::from(large_gas) * U256::from(large_price);
        let expected = expected_l2_cost + U256::from(1_000_000) + U256::from(500_000);

        assert_eq!(total, expected, "Should handle large values correctly");
    }

    #[test]
    fn test_total_gas_cost_saturating_arithmetic() {
        // Test that the calculation uses saturating arithmetic
        // This test verifies the function doesn't panic on large inputs
        let max_gas = u64::MAX;
        let max_price = u64::MAX;

        let tx = create_test_tx(max_gas, max_price, Some(u64::MAX), u64::MAX, 1000);

        // Should not panic - saturating_mul and saturating_add prevent overflow
        let total = tx.total_gas_cost();

        // The result should be saturated at U256::MAX
        assert!(
            total > U256::ZERO,
            "Should produce non-zero result even with overflow"
        );
    }

    #[test]
    fn test_to_display_conversion_usdc() {
        // Test conversion to display format with USDC (6 decimals)
        let tx = create_test_tx(
            21000,
            50_000_000_000_u64, // 50 Gwei
            None,
            0,
            1_000_000, // 1 USDC (6 decimals)
        );

        let display = tx.to_display(DecimalPrecision::Usdc);

        // Verify fields are properly formatted
        assert_eq!(
            display.gas_used, "21000",
            "Gas used should be simple number"
        );
        // The format should be exactly "1" (trailing zeros removed)
        assert_eq!(
            display.transferred_amount_usdc, "1",
            "Should format 1 USDC as '1'"
        );
    }

    #[test]
    fn test_to_display_conversion_18_decimals() {
        // Test with 18-decimal token (like ETH/WETH)
        let tx = create_test_tx(
            100000,
            10_000_000_000_u64, // 10 Gwei
            Some(5000),
            0,
            1_000_000_000_000_000_000, // 1 token (18 decimals)
        );

        let display = tx.to_display(DecimalPrecision::NativeToken);

        assert_eq!(display.gas_used, "100000");
        assert_eq!(
            display.transferred_amount_usdc, "1",
            "Should format 1 token as '1'"
        );
    }

    #[test]
    fn test_to_display_with_l1_fee() {
        // Test that L1 fee is included in display
        let tx = create_test_tx(
            50000,
            20_000_000_000_u64,            // 20 Gwei
            Some(100_000_000_000_000_u64), // 0.0001 ETH L1 fee
            0,
            1_000_000,
        );

        let display = tx.to_display(DecimalPrecision::Usdc);

        // L1 fee should be Some(String)
        assert!(display.l1_fee_eth.is_some(), "L1 fee should be present");
    }

    #[test]
    fn test_to_display_without_l1_fee() {
        // Test L1-only transaction (no L1 fee field)
        let tx = create_test_tx(
            21000,
            50_000_000_000_u64,
            None, // L1 transaction, no L1 fee
            0,
            1_000_000,
        );

        let display = tx.to_display(DecimalPrecision::Usdc);

        assert!(
            display.l1_fee_eth.is_none(),
            "L1 fee should be None for L1 transactions"
        );
    }

    #[test]
    fn test_clone_and_equality() {
        // Test that GasAndAmountForTx implements Clone and PartialEq correctly
        let tx1 = create_test_tx(21000, 50, None, 0, 1000);
        let tx2 = tx1.clone();

        assert_eq!(tx1, tx2, "Cloned transactions should be equal");
        assert_eq!(
            tx1.total_gas_cost(),
            tx2.total_gas_cost(),
            "Total costs should match"
        );
    }

    #[test]
    fn test_debug_representation() {
        // Test that Debug trait is implemented
        let tx = create_test_tx(21000, 50, Some(1000), 500, 2000);

        let debug_str = format!("{:?}", tx);

        // Should contain key fields
        assert!(
            debug_str.contains("gas_used"),
            "Debug output should include gas_used"
        );
        assert!(
            debug_str.contains("tx_hash"),
            "Debug output should include tx_hash"
        );
    }
}
