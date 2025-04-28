use alloy_chains::NamedChain;
use common::{Usdc, create_provider};
use dotenvy::dotenv;
use odos_sdk::OdosChain;
use tokio::net::TcpListener;

use crate::{PriceCalculator, PriceJob, serve_api};

/// Main entry point for the application.
pub async fn run() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    // Configure API port from environment variables
    let port = dotenvy::var("API_PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = TcpListener::bind(&format!("0.0.0.0:{port}")).await?;

    // Initialize the blockchain chain
    let chain = NamedChain::try_from(
        dotenvy::var("CHAIN")
            .unwrap_or_else(|_| "8453".to_string()) // Default to Base chain
            .parse::<u64>()?,
    )
    .expect("Invalid or missing 'CHAIN' environment variable");

    // Create the blockchain provider
    let provider = create_provider(chain, common::Signer::V2)?;

    // Fetch router and USDC addresses
    let router_address = chain.v2_router_address();
    let usdc_address = chain.usdc_address();

    // Initialize the PriceCalculator
    let calculator = PriceCalculator::new(router_address, usdc_address, provider);

    // Initialize the PriceJob
    let price_job_handle = PriceJob::init(calculator);

    // Start the API server
    serve_api(listener, price_job_handle).await?;

    Ok(())
}
