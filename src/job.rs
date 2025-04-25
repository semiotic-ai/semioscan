use alloy_primitives::Address;
use tokio::sync::{mpsc, oneshot};
use tracing::error;

use crate::price::{PriceCalculator, TokenPriceResult};

type Responder = oneshot::Sender<Result<TokenPriceResult, String>>;

pub struct PriceJob;

impl PriceJob {
    /// Initializes the `PriceJob` and returns a `PriceJobHandle`.
    pub fn init(mut calculator: PriceCalculator) -> PriceJobHandle {
        let (tx, mut rx) = mpsc::channel(10);

        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Command::CalculatePrice(cmd) => {
                        let result = PriceJob::handle_calculate_price(
                            &mut calculator,
                            cmd.token_address,
                            cmd.from_block,
                            cmd.to_block,
                        )
                        .await;

                        if cmd.responder.send(result).is_err() {
                            error!("Failed to send response");
                        }
                    }
                }
            }
        });

        PriceJobHandle { tx }
    }

    /// Handles the `CalculatePrice` command by invoking the `PriceCalculator`.
    async fn handle_calculate_price(
        calculator: &mut PriceCalculator,
        token_address: Address,
        from_block: u64,
        to_block: u64,
    ) -> Result<TokenPriceResult, String> {
        calculator
            .calculate_price_between_blocks(token_address, from_block, to_block)
            .await
            .map_err(|e| format!("Failed to calculate token price: {}", e))
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
    pub token_address: Address,
    pub from_block: u64,
    pub to_block: u64,
    pub responder: Responder,
}
