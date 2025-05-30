use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use tokio::net::TcpListener;

use crate::{
    serve_api, CalculateGasCommand, CalculatePriceCommand, CalculateTransferAmountCommand, Command,
    CommandHandler, RouterType,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Commands for the Semioscan CLI
#[derive(Subcommand)]
enum Commands {
    /// Start the API server
    Api {
        #[arg(short, long, default_value = "3000")]
        port: String,
    },
    /// Port to run the API server on
    /// Calculate token price for a given block range
    Price {
        /// Chain ID to query
        #[arg(long)]
        chain_id: u64,
        /// Token address to query
        #[arg(long)]
        token_address: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
        /// Router type (v2 or lo)
        #[arg(long, value_parser = parse_router_type)]
        router_type: RouterType,
    },
    /// Calculate gas cost for a given block range
    Gas {
        /// Chain ID to query
        #[arg(long)]
        chain_id: u64,
        /// Signer address to query
        #[arg(long)]
        signer_address: Address,
        /// Output token address to query
        #[arg(long)]
        output_token: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
        /// Router type (v2 or lo)
        #[arg(long, value_parser = parse_router_type)]
        router_type: RouterType,
    },
    /// Calculate the amount of a token transferred to a recipient
    /// for a given block range
    TransferAmount {
        /// Chain ID to query
        #[arg(long)]
        chain_id: u64,
        /// Router address
        #[arg(long)]
        router: Address,
        /// Recipient address
        #[arg(long)]
        to: Address,
        /// Token address
        #[arg(long)]
        token: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
    },
}

fn parse_router_type(s: &str) -> Result<RouterType, String> {
    match s.to_lowercase().as_str() {
        "v2" => Ok(RouterType::V2),
        "lo" => Ok(RouterType::LimitOrder),
        _ => Err("Router type must be either 'v2' or 'lo'".to_string()),
    }
}

/// Main entry point for the application.
pub async fn run() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Api { port } => {
            // Start the API server
            let listener = TcpListener::bind(&format!("0.0.0.0:{port}")).await?;
            let price_job_handle = CommandHandler::init();
            serve_api(listener, price_job_handle).await?;
        }
        Commands::Price {
            chain_id,
            token_address,
            from_block,
            to_block,
            router_type,
        } => {
            // Initialize price job for CLI usage
            let price_job_handle = CommandHandler::init();

            // Create a oneshot channel for the response
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            // Send the price calculation command
            price_job_handle
                .tx
                .send(Command::CalculatePrice(CalculatePriceCommand {
                    chain_id,
                    router_type,
                    token_address,
                    from_block,
                    to_block,
                    responder: responder_tx,
                }))
                .await?;

            // Wait for and print the result
            match responder_rx.await? {
                Ok(result) => {
                    println!("Average price: {}", result.get_average_price());
                    println!("Total token amount: {}", result.total_token_amount());
                    println!("Total USDC amount: {}", result.total_usdc_amount());
                    println!("Transaction count: {}", result.transaction_count());
                }
                Err(e) => {
                    eprintln!("Error calculating price: {}", e);
                    return Err(anyhow::anyhow!(e));
                }
            }
        }
        Commands::Gas {
            chain_id,
            signer_address,
            output_token,
            from_block,
            to_block,
            router_type,
        } => {
            let price_job_handle = CommandHandler::init();
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            price_job_handle
                .tx
                .send(Command::CalculateGas(CalculateGasCommand {
                    chain_id,
                    signer_address,
                    output_token,
                    from_block,
                    to_block,
                    router_type,
                    responder: responder_tx,
                }))
                .await?;

            match responder_rx.await? {
                Ok(result) => {
                    println!("Gas cost: {}", result.formatted_gas_cost());
                    println!("Transaction count: {}", result.transaction_count);
                }
                Err(e) => {
                    eprintln!("Error calculating gas cost: {}", e);
                }
            }
        }
        Commands::TransferAmount {
            chain_id,
            router,
            to,
            token,
            from_block,
            to_block,
        } => {
            let price_job_handle = CommandHandler::init();
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            price_job_handle
                .tx
                .send(Command::CalculateTransferAmount(
                    CalculateTransferAmountCommand {
                        chain_id,
                        router,
                        to,
                        token,
                        from_block,
                        to_block,
                        responder: responder_tx,
                    },
                ))
                .await?;

            match responder_rx.await? {
                Ok(result) => {
                    println!("Amount: {}", result.amount);
                }
                Err(e) => {
                    eprintln!("Error calculating amount: {}", e);
                }
            }
        }
    }

    Ok(())
}
