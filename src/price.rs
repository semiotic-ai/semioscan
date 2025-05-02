use alloy_primitives::{Address, B256, U256, keccak256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::Filter;
use alloy_sol_types::SolEvent;
use odos_sdk::{
    Erc20,
    OdosV2Router::{Swap, SwapMulti},
};
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, error, info};

// Price calculation result
#[derive(Default, Debug, Clone, Serialize)]
pub struct TokenPriceResult {
    token_address: Address,
    total_token_amount: f64,
    total_usdc_amount: f64,
    transaction_count: usize,
}

impl TokenPriceResult {
    fn new(token_address: Address) -> Self {
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

    pub fn get_average_price(&self) -> f64 {
        if self.total_token_amount == 0.0 {
            return 0.0;
        }
        self.total_usdc_amount / self.total_token_amount
    }
}

pub struct PriceCalculator {
    provider: RootProvider,
    router_address: Address,
    usdc_address: Address,
    liquidator_address: Address,
    token_decimals_cache: HashMap<Address, u8>,
}

impl PriceCalculator {
    pub fn new(
        router_address: Address,
        usdc_address: Address,
        liquidator_address: Address,
        provider: RootProvider,
    ) -> Self {
        Self {
            provider,
            router_address,
            usdc_address,
            liquidator_address,
            token_decimals_cache: HashMap::new(),
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

    async fn process_swap_event(
        &mut self,
        event: &SwapMulti,
        token_address: Address,
        token_decimals: u8,
    ) -> anyhow::Result<Option<(f64, f64)>> {
        if event.sender != self.liquidator_address {
            debug!("Skipping swap not initiated by our liquidator address");
            return Ok(None);
        }

        info!(
            event = ?event,
            "Processing swap event"
        );

        let token_in_indices: Vec<usize> = event
            .tokensIn
            .iter()
            .enumerate()
            .filter_map(|(i, &addr)| if addr == token_address { Some(i) } else { None })
            .collect();

        if token_in_indices.is_empty() {
            return Ok(None);
        }

        let mut stable_output = None;
        for (i, &token_out) in event.tokensOut.iter().enumerate() {
            if self.usdc_address == token_out {
                let stablecoin_decimals = self.get_token_decimals(token_out).await?;
                let amount_out = self.normalize_amount(event.amountsOut[i], stablecoin_decimals);
                stable_output = Some(amount_out);
                break;
            }
        }

        if let Some(stable_amount) = stable_output {
            let mut total_token_in = 0.0;
            for &idx in &token_in_indices {
                total_token_in += self.normalize_amount(event.amountsIn[idx], token_decimals);
            }

            return Ok(Some((total_token_in, stable_amount)));
        }

        Ok(None)
    }

    // Process the single Swap event
    async fn process_single_swap_event(
        &mut self,
        event: &Swap,
        token_address: Address,
        token_decimals: u8,
    ) -> anyhow::Result<Option<(f64, f64)>> {
        if event.sender != self.liquidator_address {
            debug!("Skipping swap not initiated by our liquidator address");
            return Ok(None);
        }

        info!(
            event = ?event,
            "Processing single swap event"
        );

        // Check if the input token matches the token we're analyzing
        if event.inputToken != token_address {
            return Ok(None);
        }

        // Check if the output token is USDC
        if event.outputToken != self.usdc_address {
            return Ok(None);
        }

        let stablecoin_decimals = self.get_token_decimals(event.outputToken).await?;
        let token_amount = self.normalize_amount(event.inputAmount, token_decimals);
        let usdc_amount = self.normalize_amount(event.amountOut, stablecoin_decimals);

        Ok(Some((token_amount, usdc_amount)))
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

        let token_decimals = self.get_token_decimals(token_address).await?;
        debug!(
            token_address = ?token_address,
            token_decimals = token_decimals,
            "Token decimals"
        );

        let mut price_data = TokenPriceResult::new(token_address);

        const MAX_BLOCK_RANGE: u64 = 2_000;
        let mut current_block = start_block;

        // Event signatures
        let multi_swap_signature =
            "SwapMulti(address,uint256[],address[],uint256[],address[],uint32)";
        let multi_swap_topic = B256::from_slice(&*keccak256(multi_swap_signature.as_bytes()));

        let single_swap_signature = "Swap(address,uint256,address,uint256,address,int256,uint32)";
        let single_swap_topic = B256::from_slice(&*keccak256(single_swap_signature.as_bytes()));

        while current_block <= end_block {
            let to_block = std::cmp::min(current_block + MAX_BLOCK_RANGE - 1, end_block);

            info!(
                current_block = current_block,
                to_block = to_block,
                "Fetching logs for block range"
            );

            // Create a filter that includes both event signatures
            let filter = Filter::new()
                .from_block(current_block)
                .to_block(to_block)
                .address(self.router_address);

            debug!(
                filter = ?filter,
                "Filter for all swap events"
            );

            match self.provider.get_logs(&filter).await {
                Ok(logs) => {
                    info!(
                        logs_count = logs.len(),
                        current_block = current_block,
                        to_block = to_block,
                        "Fetched logs for block range"
                    );

                    for log in logs {
                        // Process based on the event signature in the first topic
                        if !log.topics().is_empty() {
                            let topic = log.topics()[0];

                            if topic == multi_swap_topic {
                                // Process as SwapMulti event
                                match SwapMulti::decode_log(&log.into()) {
                                    Ok(event) => {
                                        debug!(event = ?event, "Decoded SwapMulti event");

                                        match self
                                            .process_swap_event(
                                                &event,
                                                token_address,
                                                token_decimals,
                                            )
                                            .await
                                        {
                                            Ok(Some((token_amount, usdc_amount))) => {
                                                debug!(
                                                    token_address = ?token_address,
                                                    token_amount = token_amount,
                                                    usdc_amount = usdc_amount,
                                                    "Processed SwapMulti"
                                                );
                                                price_data.add_swap(token_amount, usdc_amount);
                                            }
                                            Ok(None) => {
                                                debug!(
                                                    token_address = ?token_address,
                                                    "SwapMulti event did not match token address"
                                                );
                                            }
                                            Err(e) => {
                                                error!(error = ?e, "Error processing SwapMulti event");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(error = ?e, "Failed to decode SwapMulti log");
                                    }
                                }
                            } else if topic == single_swap_topic {
                                // Process as Swap event
                                match Swap::decode_log(&log.into()) {
                                    Ok(event) => {
                                        debug!(event = ?event, "Decoded Swap event");

                                        match self
                                            .process_single_swap_event(
                                                &event,
                                                token_address,
                                                token_decimals,
                                            )
                                            .await
                                        {
                                            Ok(Some((token_amount, usdc_amount))) => {
                                                debug!(
                                                    token_address = ?token_address,
                                                    token_amount = token_amount,
                                                    usdc_amount = usdc_amount,
                                                    "Processed Swap"
                                                );
                                                price_data.add_swap(token_amount, usdc_amount);
                                            }
                                            Ok(None) => {
                                                debug!(
                                                    token_address = ?token_address,
                                                    "Swap event did not match token address"
                                                );
                                            }
                                            Err(e) => {
                                                error!(error = ?e, "Error processing Swap event");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!(error = ?e, "Failed to decode Swap log");
                                    }
                                }
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
