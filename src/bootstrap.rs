use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use tokio::net::TcpListener;

use crate::{serve_api, PriceJob, RouterType};

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
        /// Port to run the API server on
        #[arg(short, long, default_value = "3000")]
        port: String,
    },
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
            let price_job_handle = PriceJob::init();
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
            let price_job_handle = PriceJob::init();

            // Create a oneshot channel for the response
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            // Send the price calculation command
            price_job_handle
                .tx
                .send(crate::Command::CalculatePrice(
                    crate::CalculatePriceCommand {
                        chain_id,
                        router_type,
                        token_address,
                        from_block,
                        to_block,
                        responder: responder_tx,
                    },
                ))
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
    }

    Ok(())
}
