use alloy_chains::NamedChain;
use alloy_primitives::Address;
use common::{create_read_provider, Usdc};
use odos_sdk::OdosChain;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::{
    price::{PriceCalculator, TokenPriceResult},
    RouterType,
};

type Responder = oneshot::Sender<Result<TokenPriceResult, String>>;

pub struct PriceJob {
    calculators: HashMap<u64, PriceCalculator>,
}

impl PriceJob {
    /// Initializes the `PriceJob` and returns a `PriceJobHandle`.
    pub fn init() -> PriceJobHandle {
        let (tx, mut rx) = mpsc::channel(10);

        let job = PriceJob {
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
                            error!("Failed to send response");
                        }
                    }
                }
            }
        });

        PriceJobHandle { tx }
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
            let provider = create_read_provider(chain)?;

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
}

#[derive(Clone)]
pub struct PriceJobHandle {
    pub tx: mpsc::Sender<Command>,
}

pub enum Command {
    CalculatePrice(CalculatePriceCommand),
}

pub struct CalculatePriceCommand {
    pub chain_id: u64,
    pub router_type: RouterType,
    pub token_address: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder,
}
