use alloy_chains::NamedChain;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::Filter;
use erc20_rs::Erc20;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{error, info};

use crate::price::{PriceSource, PriceSourceError};
use crate::PriceCache;

#[cfg(feature = "api-server")]
use axum::{extract::State, Json};

#[cfg(all(feature = "api-server", feature = "odos-example"))]
use crate::{CalculatePriceCommand, Command, SemioscanHandle};

#[cfg(all(feature = "api-server", feature = "odos-example"))]
use odos_sdk::RouterType;

// Price calculation result
#[derive(Default, Debug, Clone, Serialize)]
pub struct TokenPriceResult {
    token_address: Address,
    total_token_amount: f64,
    total_usdc_amount: f64,
    transaction_count: usize,
}

impl TokenPriceResult {
    pub fn new(token_address: Address) -> Self {
        Self {
            token_address,
            ..Default::default()
        }
    }

    fn add_swap(&mut self, token_amount: f64, usdc_amount: f64) {
        self.total_token_amount += token_amount;
        self.total_usdc_amount += usdc_amount;
        self.transaction_count += 1;
    }

    /// Get the average price of the token
    pub fn get_average_price(&self) -> f64 {
        if self.total_token_amount == 0.0 {
            return 0.0;
        }
        self.total_usdc_amount / self.total_token_amount
    }

    /// Merge two price results together
    pub fn merge(&mut self, other: &Self) {
        self.total_token_amount += other.total_token_amount;
        self.total_usdc_amount += other.total_usdc_amount;
        self.transaction_count += other.transaction_count;
    }

    /// Get the total token amount
    pub fn total_token_amount(&self) -> f64 {
        self.total_token_amount
    }

    /// Get the total USDC amount
    pub fn total_usdc_amount(&self) -> f64 {
        self.total_usdc_amount
    }

    /// Get the transaction count
    pub fn transaction_count(&self) -> usize {
        self.transaction_count
    }
}

pub struct PriceCalculator {
    provider: RootProvider,
    price_source: Box<dyn PriceSource>,
    usdc_address: Address,
    chain: alloy_chains::NamedChain,
    token_decimals_cache: HashMap<Address, u8>,
    price_cache: Mutex<PriceCache>,
    config: crate::SemioscanConfig,
}

impl PriceCalculator {
    /// Create a new PriceCalculator with a custom price source
    ///
    /// # Arguments
    ///
    /// * `provider` - Blockchain provider for querying logs and token data
    /// * `usdc_address` - Address of the stablecoin to calculate prices against
    /// * `price_source` - Implementation of PriceSource trait for extracting swap data
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use semioscan::price::odos::OdosPriceSource;
    /// use semioscan::price_calculator::PriceCalculator;
    ///
    /// let price_source = OdosPriceSource::new(router_address)
    ///     .with_liquidator_filter(liquidator_address);
    /// let calculator = PriceCalculator::new(provider, usdc_address, Box::new(price_source));
    /// ```
    pub fn new(
        provider: RootProvider,
        chain: alloy_chains::NamedChain,
        usdc_address: Address,
        price_source: Box<dyn PriceSource>,
    ) -> Self {
        Self::with_config(
            provider,
            chain,
            usdc_address,
            price_source,
            crate::SemioscanConfig::default(),
        )
    }

    /// Create a new PriceCalculator with custom configuration
    ///
    /// # Arguments
    ///
    /// * `provider` - Blockchain provider for querying logs and token data
    /// * `chain` - The blockchain network (used for config lookups)
    /// * `usdc_address` - Address of the stablecoin to calculate prices against
    /// * `price_source` - Implementation of PriceSource trait for extracting swap data
    /// * `config` - Configuration for RPC behavior (block ranges, rate limiting)
    pub fn with_config(
        provider: RootProvider,
        chain: alloy_chains::NamedChain,
        usdc_address: Address,
        price_source: Box<dyn PriceSource>,
        config: crate::SemioscanConfig,
    ) -> Self {
        Self {
            provider,
            price_source,
            usdc_address,
            chain,
            token_decimals_cache: HashMap::new(),
            price_cache: Default::default(),
            config,
        }
    }

    async fn get_token_decimals(&mut self, token_address: Address) -> anyhow::Result<u8> {
        if let Some(&decimals) = self.token_decimals_cache.get(&token_address) {
            return Ok(decimals);
        }

        let token_contract = Erc20::new(token_address, self.provider.clone());
        let decimals = token_contract.decimals().await?;
        self.token_decimals_cache.insert(token_address, decimals);

        Ok(decimals)
    }

    fn normalize_amount(&self, amount: U256, decimals: u8) -> f64 {
        let divisor = U256::from(10).pow(U256::from(decimals));
        f64::from(amount) / f64::from(divisor)
    }

    /// Process a SwapData extracted by the PriceSource trait
    ///
    /// Checks if the swap involves our target token and USDC, then extracts
    /// normalized amounts for price calculation.
    ///
    /// Returns Some((token_amount, usdc_amount)) if relevant, None otherwise.
    async fn process_swap_data(
        &mut self,
        swap: &crate::price::SwapData,
        token_address: Address,
    ) -> anyhow::Result<Option<(f64, f64)>> {
        // Check if this swap involves our target token being sold for USDC
        if swap.token_in == token_address && swap.token_out == self.usdc_address {
            let token_decimals = self.get_token_decimals(token_address).await?;
            let usdc_decimals = self.get_token_decimals(self.usdc_address).await?;

            let token_amount = self.normalize_amount(swap.token_in_amount, token_decimals);
            let usdc_amount = self.normalize_amount(swap.token_out_amount, usdc_decimals);

            return Ok(Some((token_amount, usdc_amount)));
        }

        // Check if this swap involves USDC being sold for our target token (reverse direction)
        // This provides price information too: if someone buys our token with USDC
        if swap.token_in == self.usdc_address && swap.token_out == token_address {
            let token_decimals = self.get_token_decimals(token_address).await?;
            let usdc_decimals = self.get_token_decimals(self.usdc_address).await?;

            let token_amount = self.normalize_amount(swap.token_out_amount, token_decimals);
            let usdc_amount = self.normalize_amount(swap.token_in_amount, usdc_decimals);

            return Ok(Some((token_amount, usdc_amount)));
        }

        Ok(None)
    }

    pub async fn calculate_price_between_blocks(
        &mut self,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> anyhow::Result<TokenPriceResult> {
        info!(
            token_address = ?token_address,
            start_block = start_block,
            end_block = end_block,
            "Starting price calculation"
        );

        // Check cache and calculate gaps that need to be filled
        let (cached_result, gaps) = {
            let cache = self.price_cache.lock().unwrap();
            cache.calculate_gaps(token_address, start_block, end_block)
        };

        // If there are no gaps, we can return the cached result
        if let Some(result) = cached_result.clone() {
            if gaps.is_empty() {
                info!(
                    token_address = ?token_address,
                    "Using complete cached result for block range"
                );
                return Ok(result);
            }
        }

        // Initialize with any cached data or create new result
        let mut price_data = cached_result.unwrap_or_else(|| TokenPriceResult::new(token_address));

        // Process each gap
        for (gap_start, gap_end) in gaps {
            info!(
                token_address = ?token_address,
                gap_start = gap_start,
                gap_end = gap_end,
                "Processing uncached block range"
            );

            // Process the gap using PriceSource trait
            let mut current_block = gap_start;
            let max_block_range = self.config.get_max_block_range(self.chain);
            let rate_limit = self.config.get_rate_limit_delay(self.chain);

            // Get event topics from price source
            let event_topics = self.price_source.event_topics();

            let mut gap_result = TokenPriceResult::new(token_address);

            while current_block <= gap_end {
                let to_block = std::cmp::min(current_block + max_block_range - 1, gap_end);

                // Create a filter for swap events from the price source
                let filter = Filter::new()
                    .from_block(current_block)
                    .to_block(to_block)
                    .address(self.price_source.router_address())
                    .event_signature(event_topics.clone());

                match self.provider.get_logs(&filter).await {
                    Ok(logs) => {
                        info!(
                            logs_count = logs.len(),
                            current_block = current_block,
                            to_block = to_block,
                            "Fetched logs for block range"
                        );

                        for log in logs {
                            // Extract swap data using the price source
                            match self.price_source.extract_swap_from_log(&log) {
                                Ok(Some(swap_data)) => {
                                    // Apply price source filtering
                                    if !self.price_source.should_include_swap(&swap_data) {
                                        continue;
                                    }

                                    // Process the swap data
                                    match self.process_swap_data(&swap_data, token_address).await {
                                        Ok(Some((token_amount, usdc_amount))) => {
                                            gap_result.add_swap(token_amount, usdc_amount);
                                        }
                                        Ok(None) => {
                                            // Not relevant for our token
                                        }
                                        Err(e) => {
                                            error!(error = ?e, "Error processing swap data");
                                        }
                                    }
                                }
                                Ok(None) => {
                                    // Log is not a relevant swap event
                                }
                                Err(PriceSourceError::DecodeError(e)) => {
                                    error!(error = ?e, "Failed to decode log");
                                }
                                Err(PriceSourceError::InvalidSwapData(e)) => {
                                    error!(error = ?e, "Invalid swap data in log");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            error = ?e,
                            current_block = current_block,
                            to_block = to_block,
                            "Error fetching logs for block range"
                        );
                    }
                }

                current_block = to_block + 1;

                // Apply rate limiting if configured for this chain
                if let Some(delay) = rate_limit {
                    if current_block <= gap_end {
                        tokio::time::sleep(delay).await;
                    }
                }
            }

            // Cache the gap result
            {
                let mut cache = self.price_cache.lock().unwrap();
                cache.insert(token_address, gap_start, gap_end, gap_result.clone());
            }

            // Merge the gap result with our main result
            price_data.merge(&gap_result);
        }

        // Cache the complete result
        {
            let mut cache = self.price_cache.lock().unwrap();
            cache.insert(token_address, start_block, end_block, price_data.clone());
        }

        info!(
            token_address = ?token_address,
            total_token_amount = price_data.total_token_amount,
            total_usdc_amount = price_data.total_usdc_amount,
            transaction_count = price_data.transaction_count,
            "Finished price calculation"
        );

        Ok(price_data)
    }
}

/// Query parameters for the price endpoints.
#[cfg(feature = "api-server")]
#[derive(Debug, Deserialize)]
pub struct PriceQuery {
    chain: NamedChain,
    token_address: Address,
    from_block: u64,
    to_block: u64,
}

/// Handler for the v2 price endpoint.
#[cfg(feature = "api-server")]
pub async fn get_v2_price(
    State(price_job): State<SemioscanHandle>,
    axum::extract::Query(params): axum::extract::Query<PriceQuery>,
) -> Result<Json<String>, String> {
    info!(router_type = "v2", params = ?params, "Received price request");

    let token_address = params.token_address;
    let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

    price_job
        .tx
        .send(Command::CalculatePrice(CalculatePriceCommand {
            token_address,
            from_block: params.from_block,
            to_block: params.to_block,
            chain: params.chain,
            router_type: RouterType::V2,
            responder: responder_tx,
        }))
        .await
        .map_err(|_| "Failed to send command".to_string())?;

    match responder_rx.await {
        Ok(Ok(result)) => Ok(Json(format!(
            "Average price: {}",
            result.get_average_price()
        ))),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to receive response".to_string()),
    }
}

/// Handler for the limit order price endpoint.
#[cfg(feature = "api-server")]
pub async fn get_lo_price(
    State(_price_job): State<SemioscanHandle>,
    axum::extract::Query(params): axum::extract::Query<PriceQuery>,
) -> Result<Json<String>, String> {
    info!(router_type = "limit_order", params = ?params, "Received price request");

    // For now, return an informative error since limit order is not implemented
    Err("Limit order price calculation is not yet implemented".to_string())
}
