use alloy_chains::NamedChain;
use alloy_network::{eip2718::Typed2718, Ethereum, Network};
use alloy_primitives::{address, keccak256, Address, TxHash, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, Log as RpcLog, TransactionTrait};
use alloy_sol_types::SolEvent;
use bigdecimal::BigDecimal;
use op_alloy_network::Optimism;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, trace, warn};

use crate::{
    spans, EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter, Transfer,
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
    pub tx_hash: TxHash,
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

    /// Convert to display format with custom token decimals
    pub fn to_display(&self, token_decimals: u8) -> GasAndAmountDisplay {
        let l2_execution_cost = self.gas_used.saturating_mul(self.effective_gas_price);
        let total_cost = l2_execution_cost
            .saturating_add(self.blob_gas_cost)
            .saturating_add(self.l1_fee.unwrap_or_default());

        GasAndAmountDisplay {
            tx_hash: self.tx_hash.to_string(),
            gas_used: self.gas_used.to_string(),
            effective_gas_price_gwei: format_wei_to_gwei(self.effective_gas_price),
            l1_fee_eth: self.l1_fee.map(format_wei_to_eth),
            blob_gas_cost_eth: format_wei_to_eth(self.blob_gas_cost),
            total_gas_cost_eth: format_wei_to_eth(total_cost),
            transferred_amount_usdc: format_token_amount(self.transferred_amount, token_decimals),
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
    pub transaction_count: usize,
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
    pub transaction_count: usize,
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

impl From<&CombinedDataResult> for CombinedDataDisplay {
    fn from(result: &CombinedDataResult) -> Self {
        CombinedDataDisplay {
            chain: result.chain,
            from_address: format!("{:#x}", result.from_address),
            to_address: format!("{:#x}", result.to_address),
            token_address: format!("{:#x}", result.token_address),
            total_l2_execution_cost_eth: format_wei_to_eth(result.total_l2_execution_cost),
            total_blob_gas_cost_eth: format_wei_to_eth(result.total_blob_gas_cost),
            total_l1_fee_eth: format_wei_to_eth(result.total_l1_fee),
            overall_total_gas_cost_eth: format_wei_to_eth(result.overall_total_gas_cost),
            total_amount_transferred_usdc: format_usdc(result.total_amount_transferred),
            transaction_count: result.transaction_count,
            transactions_data: result
                .transactions_data
                .iter()
                .map(GasAndAmountDisplay::from)
                .collect(),
        }
    }
}

impl From<&GasAndAmountForTx> for GasAndAmountDisplay {
    fn from(tx: &GasAndAmountForTx) -> Self {
        let l2_execution_cost = tx.gas_used.saturating_mul(tx.effective_gas_price);
        let total_cost = l2_execution_cost
            .saturating_add(tx.blob_gas_cost)
            .saturating_add(tx.l1_fee.unwrap_or_default());

        GasAndAmountDisplay {
            tx_hash: format!("{:#x}", tx.tx_hash),
            gas_used: tx.gas_used.to_string(),
            effective_gas_price_gwei: format_wei_to_gwei(tx.effective_gas_price),
            l1_fee_eth: tx.l1_fee.map(format_wei_to_eth),
            blob_gas_cost_eth: format_wei_to_eth(tx.blob_gas_cost),
            total_gas_cost_eth: format_wei_to_eth(total_cost),
            transferred_amount_usdc: format_usdc(tx.transferred_amount),
        }
    }
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

/// Convert token raw amount (U256) to string with specified decimals
/// Most chains use 6 decimals for USDC, but BNB Chain uses 18 decimals
fn format_token_amount(raw_amount: U256, decimals: u8) -> String {
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

/// Convert USDC raw amount (U256) to USDC string with 6 decimals (default)
fn format_usdc(raw_amount: U256) -> String {
    format_token_amount(raw_amount, 6)
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
    const BSC_BINANCE_PEG_USDC: Address = address!("8ac76a51cc950d9822d68b83fe1ad97b32cd580d");

    if matches!(chain, NamedChain::BinanceSmartChain)
        && matches!(token_address, BSC_BINANCE_PEG_USDC)
    {
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
            transaction_count: 0,
            transactions_data: Vec::new(),
        }
    }

    /// Convert to display format with custom token decimals
    pub fn to_display(&self, token_decimals: u8) -> CombinedDataDisplay {
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
                token_decimals,
            ),
            transaction_count: self.transaction_count,
            transactions_data: self
                .transactions_data
                .iter()
                .map(|tx| tx.to_display(token_decimals))
                .collect(),
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

        Ok(Some(GasAndAmountForTx {
            tx_hash,
            gas_used,
            effective_gas_price,
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
        from_block: u64,
        to_block: u64,
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

            // Delay for specific chains to avoid rate limiting
            if chain.eq(&NamedChain::Base) && current_block <= to_block {
                trace!(?chain, "Applying delay for chain Base");
                sleep(Duration::from_millis(250)).await;
            }
        }
        info!(?chain, %from_address, %to_address, %token_address, from_block, to_block, transactions_found = result.transaction_count, "Finished processing block range");
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
        from_block: u64,
        to_block: u64,
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
        from_block: u64,
        to_block: u64,
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
        from_block: u64,
        to_block: u64,
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
