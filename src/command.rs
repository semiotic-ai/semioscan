use alloy_chains::NamedChain;
use alloy_primitives::Address;
use common::{create_l1_read_provider, Usdc};
use odos_sdk::OdosChain;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::{
    price::{PriceCalculator, TokenPriceResult},
    AmountCalculator, AmountResult, GasCostCalculator, GasCostResult, RouterType,
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
                                cmd.signer_address,
                                cmd.output_token,
                                cmd.from_block,
                                cmd.to_block,
                            )
                            .await
                            .map_err(|e| e.to_string());

                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send gas cost response");
                        }
                    }
                    Command::CalculateAmount(cmd) => {
                        let result = job
                            .handle_calculate_amount(
                                cmd.chain_id,
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
            let router_address = chain.v2_router_address();
            let usdc_address = chain.usdc_address();

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

    async fn handle_calculate_gas(
        &mut self,
        chain_id: u64,
        signer_address: Address,
        output_token: Address,
        from_block: u64,
        to_block: u64,
    ) -> anyhow::Result<GasCostResult> {
        let chain = NamedChain::try_from(chain_id)
            .map_err(|_| anyhow::anyhow!("Invalid chain ID: {chain_id}"))?;

        let provider = create_l1_read_provider(chain)?;

        let calculator = GasCostCalculator::new(provider);

        calculator
            .calculate_gas_cost_between_blocks(
                chain_id,
                signer_address,
                output_token,
                from_block,
                to_block,
            )
            .await
    }

    async fn handle_calculate_amount(
        &mut self,
        chain_id: u64,
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
            .calculate_amount_between_blocks(chain_id, to, token, from_block, to_block)
            .await
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
    CalculateAmount(CalculateAmountCommand),
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
    pub signer_address: Address,
    pub output_token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub router_type: RouterType,
    pub responder: Responder<GasCostResult>,
}

pub struct CalculateAmountCommand {
    pub chain_id: u64,
    pub to: Address,
    pub token: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder<AmountResult>,
}
