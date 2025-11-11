use crate::provider::{create_ethereum_provider, create_optimism_provider, ChainFeatures};
use alloy_chains::NamedChain;
use alloy_primitives::Address;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

#[cfg(feature = "odos-example")]
use odos_sdk::{LimitOrderV2, OdosChain, RouterType, V2Router, V3Router};
#[cfg(feature = "odos-example")]
use usdshe::Usdc;

#[cfg(feature = "cli")]
use crate::SupportedEvent;

#[cfg(feature = "odos-example")]
use crate::price::odos::OdosPriceSource;
#[cfg(feature = "odos-example")]
use crate::price_calculator::{PriceCalculator, TokenPriceResult};

use crate::{
    AmountCalculator, AmountResult, CombinedCalculator, CombinedDataResult, GasCostCalculator,
    GasCostResult,
};

type Responder<T> = oneshot::Sender<Result<T, String>>;

pub struct CommandHandler {
    #[cfg(feature = "odos-example")]
    calculators: HashMap<u64, PriceCalculator>,
}

impl CommandHandler {
    /// Initializes the `PriceJob` and returns a `PriceJobHandle`.
    pub fn init() -> SemioscanHandle {
        let (tx, mut rx) = mpsc::channel(10);

        let job = CommandHandler {
            #[cfg(feature = "odos-example")]
            calculators: HashMap::new(),
        };

        tokio::spawn(async move {
            let mut job = job;
            while let Some(command) = rx.recv().await {
                match command {
                    #[cfg(feature = "odos-example")]
                    Command::CalculatePrice(cmd) => {
                        let result = job
                            .handle_calculate_price(
                                cmd.chain,
                                cmd.router_type,
                                cmd.token_address,
                                cmd.from_block,
                                cmd.to_block,
                            )
                            .await
                            .map_err(|e| e.to_string());

                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send price calculation response");
                        }
                    }
                    #[cfg(feature = "cli")]
                    Command::CalculateGas(cmd) => {
                        let result = job
                            .handle_calculate_gas(
                                cmd.chain,
                                cmd.event,
                                cmd.from,
                                cmd.to,
                                cmd.token,
                                cmd.from_block,
                                cmd.to_block,
                            )
                            .await
                            .map_err(|e| e.to_string());

                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send gas cost response");
                        }
                    }
                    #[cfg(feature = "cli")]
                    Command::CalculateTransferAmount(cmd) => {
                        let result = job
                            .handle_calculate_transfer_amount(
                                cmd.chain,
                                cmd.from,
                                cmd.to,
                                cmd.token,
                                cmd.from_block,
                                cmd.to_block,
                            )
                            .await
                            .map_err(|e| e.to_string());
                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send amount response");
                        }
                    }
                    #[cfg(feature = "cli")]
                    Command::CalculateCombinedData(cmd) => {
                        let result = job
                            .handle_calculate_combined_data(
                                cmd.chain,
                                cmd.from,
                                cmd.to,
                                cmd.token,
                                cmd.from_block,
                                cmd.to_block,
                            )
                            .await
                            .map_err(|e| e.to_string());
                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send combined data response");
                        }
                    }
                }
            }
        });

        SemioscanHandle { tx }
    }

    /// Get or create a PriceCalculator for the specified chain
    #[cfg(feature = "odos-example")]
    async fn get_or_create_calculator(
        &mut self,
        chain: NamedChain,
        router_type: RouterType,
    ) -> anyhow::Result<&mut PriceCalculator> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.calculators.entry(chain as u64) {
            // Create a new calculator for this chain
            info!(chain = ?chain, router_type = ?router_type, "Creating new PriceCalculator");

            // Create provider for this chain
            let provider = create_ethereum_provider(chain)?;

            // Get router address based on router type using SDK's type-safe methods
            let router_address = match router_type {
                RouterType::LimitOrder => chain.lo_router_address()?,
                RouterType::V2 => chain.v2_router_address()?,
                RouterType::V3 => chain.v3_router_address()?,
            };

            // Get chain-specific USDC address
            let usdc_address = chain.usdc_address()?;

            // Get liquidator/owner address from router contract
            // V2 routers only have owner(), LO and V3 have liquidator_address()
            let liquidator_address = match router_type {
                RouterType::V2 => {
                    let router = V2Router::new(router_address, &provider);
                    router.owner().await?
                }
                RouterType::LimitOrder => {
                    let router = LimitOrderV2::new(router_address, &provider);
                    router.liquidator_address().await?
                }
                RouterType::V3 => {
                    let router = V3Router::new(router_address, &provider);
                    router.liquidator_address().await?
                }
            };

            info!(liquidator_address = ?liquidator_address, router_type = ?router_type, "Retrieved liquidator/owner address from router contract");

            // Create price source with liquidator filter
            let price_source =
                OdosPriceSource::new(router_address).with_liquidator_filter(liquidator_address);

            // Create calculator with price source
            let calculator = PriceCalculator::new(provider, usdc_address, Box::new(price_source));
            e.insert(calculator);
        }

        Ok(self
            .calculators
            .get_mut(&(chain as u64))
            .unwrap_or_else(|| {
                panic!("PriceCalculator not found for chain: {}", chain);
            }))
    }

    /// Handle the `CalculatePrice` command by invoking the `PriceCalculator`.
    #[cfg(feature = "odos-example")]
    async fn handle_calculate_price(
        &mut self,
        chain: NamedChain,
        router_type: RouterType,
        token_address: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<TokenPriceResult> {
        let calculator = self.get_or_create_calculator(chain, router_type).await?;

        calculator
            .calculate_price_between_blocks(token_address, from_block, to_block)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "cli")]
    async fn handle_calculate_gas(
        &mut self,
        chain: NamedChain,
        event: SupportedEvent,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        if chain.has_l1_fees() {
            let provider = create_optimism_provider(chain)?;
            let calculator = GasCostCalculator::new(provider);

            match event {
                SupportedEvent::Transfer => {
                    calculator
                        .calculate_gas_cost_for_transfers_between_blocks(
                            chain as u64,
                            from,
                            to,
                            token,
                            from_block,
                            to_block,
                        )
                        .await
                }
                SupportedEvent::Approval => {
                    calculator
                        .calculate_gas_cost_for_approvals_between_blocks(
                            chain as u64,
                            from,
                            to,
                            token,
                            from_block,
                            to_block,
                        )
                        .await
                }
            }
        } else {
            let provider = create_ethereum_provider(chain)?;
            let calculator = GasCostCalculator::new(provider);

            match event {
                SupportedEvent::Transfer => {
                    calculator
                        .calculate_gas_cost_for_transfers_between_blocks(
                            chain as u64,
                            from,
                            to,
                            token,
                            from_block,
                            to_block,
                        )
                        .await
                }
                SupportedEvent::Approval => {
                    calculator
                        .calculate_gas_cost_for_approvals_between_blocks(
                            chain as u64,
                            from,
                            to,
                            token,
                            from_block,
                            to_block,
                        )
                        .await
                }
            }
        }
    }

    #[cfg(feature = "cli")]
    async fn handle_calculate_transfer_amount(
        &mut self,
        chain: NamedChain,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<AmountResult> {
        let provider = create_ethereum_provider(chain)?;

        let calculator = AmountCalculator::new(provider);

        calculator
            .calculate_transfer_amount_between_blocks(
                chain as u64,
                from,
                to,
                token,
                from_block,
                to_block,
            )
            .await
    }

    #[cfg(feature = "cli")]
    async fn handle_calculate_combined_data(
        &mut self,
        chain: NamedChain,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<CombinedDataResult> {
        if chain.has_l1_fees() {
            let provider = create_optimism_provider(chain)?;
            let calculator = CombinedCalculator::new(provider);

            calculator
                .calculate_combined_data_optimism(chain, from, to, token, from_block, to_block)
                .await
        } else {
            let provider = create_ethereum_provider(chain)?;
            let calculator = CombinedCalculator::new(provider);

            calculator
                .calculate_combined_data_ethereum(chain, from, to, token, from_block, to_block)
                .await
        }
    }
}

#[derive(Clone)]
pub struct SemioscanHandle {
    pub tx: mpsc::Sender<Command>,
}

/// Commands for the Semioscan `CommandHandler`
pub enum Command {
    #[cfg(feature = "odos-example")]
    CalculatePrice(CalculatePriceCommand),
    #[cfg(feature = "cli")]
    CalculateGas(CalculateGasCommand),
    #[cfg(feature = "cli")]
    CalculateTransferAmount(CalculateTransferAmountCommand),
    #[cfg(feature = "cli")]
    CalculateCombinedData(CalculateCombinedDataCommand),
}

#[cfg(feature = "odos-example")]
pub struct CalculatePriceCommand {
    pub chain: NamedChain,
    pub router_type: RouterType,
    pub token_address: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<TokenPriceResult>,
}

#[cfg(feature = "cli")]
pub struct CalculateGasCommand {
    pub chain: NamedChain,
    pub event: SupportedEvent,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<GasCostResult>,
}

#[cfg(feature = "cli")]
pub struct CalculateTransferAmountCommand {
    pub chain: NamedChain,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<AmountResult>,
}

#[cfg(feature = "cli")]
pub struct CalculateCombinedDataCommand {
    pub chain: NamedChain,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<CombinedDataResult>,
}
