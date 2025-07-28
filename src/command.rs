use alloy_chains::NamedChain;
use alloy_primitives::Address;
use common::{create_l1_read_provider, create_op_stack_read_provider, L2};
use odos_sdk::OdosChain;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};
use usdshe::Usdc;

use crate::{
    bootstrap::SupportedEvent,
    price::{PriceCalculator, TokenPriceResult},
    AmountCalculator, AmountResult, CombinedCalculator, CombinedDataResult, GasCostCalculator,
    GasCostResult, RouterType,
};

type Responder<T> = oneshot::Sender<Result<T, String>>;

pub struct CommandHandler {
    calculators: HashMap<u64, PriceCalculator>,
}

impl CommandHandler {
    /// Initializes the `PriceJob` and returns a `PriceJobHandle`.
    pub fn init() -> SemioscanHandle {
        let (tx, mut rx) = mpsc::channel(10);

        let job = CommandHandler {
            calculators: HashMap::new(),
        };

        tokio::spawn(async move {
            let mut job = job;
            while let Some(command) = rx.recv().await {
                match command {
                    Command::CalculatePrice(cmd) => {
                        let result = job
                            .handle_calculate_price(
                                cmd.chain_id,
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
                    Command::CalculateGas(cmd) => {
                        let result = job
                            .handle_calculate_gas(
                                cmd.chain_id,
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
                    Command::CalculateTransferAmount(cmd) => {
                        let result = job
                            .handle_calculate_transfer_amount(
                                cmd.chain_id,
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
                    Command::CalculateCombinedData(cmd) => {
                        let result = job
                            .handle_calculate_combined_data(
                                cmd.chain_id,
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
    async fn get_or_create_calculator(
        &mut self,
        chain_id: u64,
        router_type: RouterType,
    ) -> anyhow::Result<&mut PriceCalculator> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.calculators.entry(chain_id) {
            // Create a new calculator for this chain
            info!(chain_id = chain_id, "Creating new PriceCalculator");

            let chain = NamedChain::try_from(chain_id)
                .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

            // Create provider for this chain
            let provider = create_l1_read_provider(chain)?;

            // Get chain-specific addresses
            let router_address = chain.v2_router_address()?;
            let usdc_address = chain.usdc_address()?;

            // Create and insert calculator
            let calculator = PriceCalculator::new(
                router_address,
                usdc_address,
                router_type.address(),
                provider,
            );
            e.insert(calculator);
        }

        Ok(self.calculators.get_mut(&chain_id).unwrap())
    }

    /// Handle the `CalculatePrice` command by invoking the `PriceCalculator`.
    async fn handle_calculate_price(
        &mut self,
        chain_id: u64,
        router_type: RouterType,
        token_address: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<TokenPriceResult> {
        let calculator = self.get_or_create_calculator(chain_id, router_type).await?;

        calculator
            .calculate_price_between_blocks(token_address, from_block, to_block)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_calculate_gas(
        &mut self,
        chain_id: u64,
        event: SupportedEvent,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let chain = NamedChain::try_from(chain_id)
            .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

        if chain.has_l1_fees() {
            let provider = create_op_stack_read_provider(chain)?;
            let calculator = GasCostCalculator::new(provider);

            match event {
                SupportedEvent::Transfer => {
                    calculator
                        .calculate_gas_cost_for_transfers_between_blocks(
                            chain_id, from, to, token, from_block, to_block,
                        )
                        .await
                }
                SupportedEvent::Approval => {
                    calculator
                        .calculate_gas_cost_for_approvals_between_blocks(
                            chain_id, from, to, token, from_block, to_block,
                        )
                        .await
                }
            }
        } else {
            let provider = create_l1_read_provider(chain)?;
            let calculator = GasCostCalculator::new(provider);

            match event {
                SupportedEvent::Transfer => {
                    calculator
                        .calculate_gas_cost_for_transfers_between_blocks(
                            chain_id, from, to, token, from_block, to_block,
                        )
                        .await
                }
                SupportedEvent::Approval => {
                    calculator
                        .calculate_gas_cost_for_approvals_between_blocks(
                            chain_id, from, to, token, from_block, to_block,
                        )
                        .await
                }
            }
        }
    }

    async fn handle_calculate_transfer_amount(
        &mut self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<AmountResult> {
        let chain = NamedChain::try_from(chain_id)
            .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

        let provider = create_l1_read_provider(chain)?;

        let calculator = AmountCalculator::new(provider);

        calculator
            .calculate_transfer_amount_between_blocks(
                chain_id, from, to, token, from_block, to_block,
            )
            .await
    }

    async fn handle_calculate_combined_data(
        &mut self,
        chain_id: u64,
        from: Address,
        to: Address,
        token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<CombinedDataResult> {
        let chain = NamedChain::try_from(chain_id)
            .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

        if chain.has_l1_fees() {
            let provider = create_op_stack_read_provider(chain)?;
            let calculator = CombinedCalculator::new(provider);

            calculator
                .calculate_combined_data_optimism(chain_id, from, to, token, from_block, to_block)
                .await
        } else {
            let provider = create_l1_read_provider(chain)?;
            let calculator = CombinedCalculator::new(provider);

            calculator
                .calculate_combined_data_ethereum(chain_id, from, to, token, from_block, to_block)
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
    CalculatePrice(CalculatePriceCommand),
    CalculateGas(CalculateGasCommand),
    CalculateTransferAmount(CalculateTransferAmountCommand),
    CalculateCombinedData(CalculateCombinedDataCommand),
}

pub struct CalculatePriceCommand {
    pub chain_id: u64,
    pub router_type: RouterType,
    pub token_address: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<TokenPriceResult>,
}

pub struct CalculateGasCommand {
    pub chain_id: u64,
    pub event: SupportedEvent,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<GasCostResult>,
}

pub struct CalculateTransferAmountCommand {
    pub chain_id: u64,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<AmountResult>,
}

pub struct CalculateCombinedDataCommand {
    pub chain_id: u64,
    pub from: Address,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<CombinedDataResult>,
}
