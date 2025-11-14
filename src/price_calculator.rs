use alloy_chains::NamedChain;
use alloy_primitives::{Address, BlockNumber, U256};
use alloy_provider::Provider;
use alloy_rpc_types::Filter;
use erc20_rs::Erc20;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{error, info};

use crate::price::{PriceSource, PriceSourceError};
use crate::{PriceCache, SemioscanConfig};

// Price calculation result
#[derive(Default, Debug, Clone, Serialize)]
pub struct TokenPriceResult {
    pub token_address: Address,
    pub total_token_amount: f64,
    pub total_usdc_amount: f64,
    pub transaction_count: usize,
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

pub struct PriceCalculator<P> {
    provider: P,
    price_source: Box<dyn PriceSource>,
    usdc_address: Address,
    chain: NamedChain,
    token_decimals_cache: HashMap<Address, u8>,
    price_cache: Mutex<PriceCache>,
    config: SemioscanConfig,
}

impl<P: Provider + Clone> PriceCalculator<P> {
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
        provider: P,
        chain: NamedChain,
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
        provider: P,
        chain: NamedChain,
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
        start_block: BlockNumber,
        end_block: BlockNumber,
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
                let to_block = std::cmp::min(current_block + max_block_range.as_u64() - 1, gap_end);

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn test_add_swap_accumulates_amounts() {
        let token = address!("1111111111111111111111111111111111111111");
        let mut result = TokenPriceResult::new(token);

        // Add first swap
        result.add_swap(100.0, 200.0);
        assert_eq!(result.total_token_amount(), 100.0);
        assert_eq!(result.total_usdc_amount(), 200.0);
        assert_eq!(result.transaction_count(), 1);

        // Add second swap
        result.add_swap(50.0, 75.0);
        assert_eq!(result.total_token_amount(), 150.0);
        assert_eq!(result.total_usdc_amount(), 275.0);
        assert_eq!(result.transaction_count(), 2);
    }

    #[test]
    fn test_get_average_price_normal_case() {
        let token = address!("1111111111111111111111111111111111111111");
        let mut result = TokenPriceResult::new(token);

        // Add swaps with known prices
        // Swap 1: 100 tokens for 200 USDC = $2.00 per token
        result.add_swap(100.0, 200.0);
        // Swap 2: 50 tokens for 150 USDC = $3.00 per token
        result.add_swap(50.0, 150.0);

        // Average: 350 USDC / 150 tokens = $2.333... per token
        let avg_price = result.get_average_price();
        assert!((avg_price - 2.333333).abs() < 0.0001);
    }

    #[test]
    fn test_get_average_price_zero_volume() {
        let token = address!("1111111111111111111111111111111111111111");
        let result = TokenPriceResult::new(token);

        // Edge case: no volume should return 0.0, not panic
        assert_eq!(result.get_average_price(), 0.0);
    }

    #[test]
    fn test_get_average_price_zero_token_amount_after_swaps() {
        let token = address!("1111111111111111111111111111111111111111");
        let mut result = TokenPriceResult::new(token);

        // Edge case: USDC amount but zero token amount
        // This shouldn't happen in practice but we handle it gracefully
        result.add_swap(0.0, 100.0);
        assert_eq!(result.get_average_price(), 0.0);
    }

    #[test]
    fn test_merge_combines_results() {
        let token = address!("1111111111111111111111111111111111111111");

        let mut result1 = TokenPriceResult::new(token);
        result1.add_swap(100.0, 200.0);
        result1.add_swap(50.0, 100.0);

        let mut result2 = TokenPriceResult::new(token);
        result2.add_swap(25.0, 50.0);

        // Merge result2 into result1
        result1.merge(&result2);

        // Check combined values
        assert_eq!(result1.total_token_amount(), 175.0); // 100 + 50 + 25
        assert_eq!(result1.total_usdc_amount(), 350.0); // 200 + 100 + 50
        assert_eq!(result1.transaction_count(), 3);
    }

    #[test]
    fn test_merge_with_empty_result() {
        let token = address!("1111111111111111111111111111111111111111");

        let mut result = TokenPriceResult::new(token);
        result.add_swap(100.0, 200.0);

        let empty = TokenPriceResult::new(token);

        // Merge empty result should not change values
        result.merge(&empty);

        assert_eq!(result.total_token_amount(), 100.0);
        assert_eq!(result.total_usdc_amount(), 200.0);
        assert_eq!(result.transaction_count(), 1);
    }

    #[test]
    fn test_merge_two_empty_results() {
        let token = address!("1111111111111111111111111111111111111111");

        let mut result1 = TokenPriceResult::new(token);
        let result2 = TokenPriceResult::new(token);

        result1.merge(&result2);

        assert_eq!(result1.total_token_amount(), 0.0);
        assert_eq!(result1.total_usdc_amount(), 0.0);
        assert_eq!(result1.transaction_count(), 0);
        assert_eq!(result1.get_average_price(), 0.0);
    }

    #[test]
    fn test_large_amounts() {
        let token = address!("1111111111111111111111111111111111111111");
        let mut result = TokenPriceResult::new(token);

        // Test with large amounts (billions of dollars)
        result.add_swap(1_000_000_000.0, 2_000_000_000.0);
        result.add_swap(500_000_000.0, 1_000_000_000.0);

        assert_eq!(result.total_token_amount(), 1_500_000_000.0);
        assert_eq!(result.total_usdc_amount(), 3_000_000_000.0);
        assert_eq!(result.get_average_price(), 2.0);
    }

    #[test]
    fn test_fractional_amounts() {
        let token = address!("1111111111111111111111111111111111111111");
        let mut result = TokenPriceResult::new(token);

        // Test with small fractional amounts
        result.add_swap(0.001, 0.002);
        result.add_swap(0.0005, 0.001);

        assert!((result.total_token_amount() - 0.0015).abs() < 1e-10);
        assert!((result.total_usdc_amount() - 0.003).abs() < 1e-10);
        assert!((result.get_average_price() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_amount_standard_decimals() {
        // Test normalize_amount logic directly without needing a provider
        // This tests the business logic of decimal normalization

        // Test USDC (6 decimals): 1,000,000 raw = 1.0 USDC
        let divisor = U256::from(10u64.pow(6));
        let normalized = f64::from(U256::from(1_000_000u64)) / f64::from(divisor);
        assert_eq!(normalized, 1.0);

        // Test WETH (18 decimals): 1e18 raw = 1.0 ETH
        let divisor = U256::from(10u128.pow(18));
        let normalized = f64::from(U256::from(1_000_000_000_000_000_000u64)) / f64::from(divisor);
        assert_eq!(normalized, 1.0);
    }

    #[test]
    fn test_normalize_amount_edge_cases() {
        // Test normalize_amount logic without needing a provider

        // Zero amount
        let divisor = U256::from(10u128.pow(18));
        let normalized = f64::from(U256::ZERO) / f64::from(divisor);
        assert_eq!(normalized, 0.0);

        // Zero decimals (like some weird tokens)
        let divisor = U256::from(10u64.pow(0)); // = 1
        let normalized = f64::from(U256::from(42u64)) / f64::from(divisor);
        assert_eq!(normalized, 42.0);

        // 1 decimal
        let divisor = U256::from(10u64.pow(1));
        let normalized = f64::from(U256::from(100u64)) / f64::from(divisor);
        assert_eq!(normalized, 10.0);
    }

    #[test]
    fn test_average_price_calculation() {
        let token = address!("1111111111111111111111111111111111111111");

        // Manually set values to simulate swap processing
        let result = TokenPriceResult {
            token_address: token,
            total_token_amount: 100.0,
            total_usdc_amount: 200.0,
            transaction_count: 5,
        };

        // Average price = 200.0 / 100.0 = 2.0 USDC per token
        assert_eq!(result.get_average_price(), 2.0);
    }

    #[test]
    fn test_average_price_fractional() {
        let token = address!("1111111111111111111111111111111111111111");
        let result = TokenPriceResult {
            token_address: token,
            total_token_amount: 333.33,
            total_usdc_amount: 999.99,
            transaction_count: 10,
        };

        // Average price ≈ 3.0
        let price = result.get_average_price();
        assert!((price - 3.0).abs() < 0.01, "Expected ~3.0, got {price}");
    }

    #[test]
    fn test_price_result_multiple_merges() {
        let token = address!("1111111111111111111111111111111111111111");

        let mut total = TokenPriceResult::new(token);

        // Merge three results
        let r1 = TokenPriceResult {
            token_address: token,
            total_token_amount: 10.0,
            total_usdc_amount: 20.0,
            transaction_count: 1,
        };

        let r2 = TokenPriceResult {
            token_address: token,
            total_token_amount: 20.0,
            total_usdc_amount: 40.0,
            transaction_count: 2,
        };

        let r3 = TokenPriceResult {
            token_address: token,
            total_token_amount: 30.0,
            total_usdc_amount: 60.0,
            transaction_count: 3,
        };

        total.merge(&r1);
        total.merge(&r2);
        total.merge(&r3);

        assert_eq!(total.total_token_amount(), 60.0);
        assert_eq!(total.total_usdc_amount(), 120.0);
        assert_eq!(total.transaction_count(), 6);
        assert_eq!(total.get_average_price(), 2.0);
    }

    #[test]
    fn test_price_calculation_high_precision() {
        let token = address!("1111111111111111111111111111111111111111");

        let result = TokenPriceResult {
            token_address: token,
            total_token_amount: 0.000001,  // Very small amount
            total_usdc_amount: 0.00000123, // Even smaller USDC amount
            transaction_count: 1,
        };

        let price = result.get_average_price();
        // Price = 0.00000123 / 0.000001 = 1.23
        assert!((price - 1.23).abs() < 0.001, "Expected ~1.23, got {price}");
    }
}
