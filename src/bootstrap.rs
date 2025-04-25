use alloy_chains::NamedChain;
use alloy_primitives::Address;
use common::{Usdc, create_provider};
use dotenvy::dotenv;
use odos_sdk::OdosChain;
use tokio::net::TcpListener;
use tracing::info;

use crate::{
    CalculatePriceCommand, Command,
    job::{PriceJob, PriceJobHandle},
    price::PriceCalculator,
};

use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

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

/// Query parameters for the `/api/v1/price` endpoint.
#[derive(Debug, Deserialize)]
struct PriceQuery {
    token_address: Address,
    from_block: u64,
    to_block: u64,
}

/// Handler for the `/api/v1/price` endpoint.
async fn get_price(
    State(price_job): State<PriceJobHandle>,
    axum::extract::Query(params): axum::extract::Query<PriceQuery>,
) -> Result<Json<String>, String> {
    info!(params = ?params, "Received price request");

    let token_address = params.token_address;

    let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

    price_job
        .tx
        .send(Command::CalculatePrice(CalculatePriceCommand {
            token_address,
            from_block: params.from_block,
            to_block: params.to_block,
            responder: responder_tx,
        }))
        .await
        .map_err(|_| "Failed to send command".to_string())?;

    match responder_rx.await {
        Ok(Ok(result)) => Ok(Json(format!(
            "Average price: {}",
            result.get_average_price()
        ))),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to receive response".to_string()),
    }
}

/// Starts the API server.
pub async fn serve_api(listener: TcpListener, price_job: PriceJobHandle) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/api/v1/price", get(get_price))
        .with_state(price_job);

    let addr = listener.local_addr()?;

    tracing::info!(address = ?addr, "Starting server");

    axum::serve(listener, app).await?;

    Ok(())
}
