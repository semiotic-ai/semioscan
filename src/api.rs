use alloy_primitives::Address;
use tokio::net::TcpListener;
use tracing::info;

use crate::{job::PriceJobHandle, CalculatePriceCommand, Command};

use axum::{extract::State, routing::get, Json, Router};
use serde::Deserialize;

const V2_LIQUIDATOR_ADDRESS: &str = "0x498020622CA0d5De103b7E78E3eFe5819D0d28AB";
// TODO: support pre-v2 routers
const LO_LIQUIDATOR_ADDRESS: &str = "0x9aA30b2289020f9de59D39fBd7Bd5f3BE661a2a6";

/// Router type enum
#[derive(Debug, Clone, Copy)]
pub enum RouterType {
    V2,
    LimitOrder,
}

impl RouterType {
    pub fn address(&self) -> Address {
        match self {
            Self::V2 => V2_LIQUIDATOR_ADDRESS.parse().unwrap(),
            Self::LimitOrder => LO_LIQUIDATOR_ADDRESS.parse().unwrap(),
        }
    }
}

/// Query parameters for the price endpoints.
#[derive(Debug, Deserialize)]
struct PriceQuery {
    chain_id: u64,
    token_address: Address,
    from_block: u64,
    to_block: u64,
}

/// Handler for the v2 price endpoint.
async fn get_v2_price(
    State(price_job): State<PriceJobHandle>,
    axum::extract::Query(params): axum::extract::Query<PriceQuery>,
) -> Result<Json<String>, String> {
    info!(router_type = "v2", params = ?params, "Received price request");

    let token_address = params.token_address;
    let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

    price_job
        .tx
        .send(Command::CalculatePrice(CalculatePriceCommand {
            token_address,
            from_block: params.from_block,
            to_block: params.to_block,
            chain_id: params.chain_id,
            router_type: RouterType::V2,
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

/// Handler for the limit order price endpoint.
async fn get_lo_price(
    State(_price_job): State<PriceJobHandle>,
    axum::extract::Query(params): axum::extract::Query<PriceQuery>,
) -> Result<Json<String>, String> {
    info!(router_type = "limit_order", params = ?params, "Received price request");

    // For now, return an informative error since limit order is not implemented
    Err("Limit order price calculation is not yet implemented".to_string())
}

/// Starts the API server.
pub async fn serve_api(listener: TcpListener, price_job: PriceJobHandle) -> anyhow::Result<()> {
    let app = Router::new()
        // Original path for backward compatibility
        .route("/api/v1/price", get(get_v2_price))
        // New explicit paths for different router types
        .route("/api/v1/price/v2", get(get_v2_price))
        .route("/api/v1/price/lo", get(get_lo_price))
        .with_state(price_job);

    let addr = listener.local_addr()?;

    tracing::info!(address = ?addr, "Starting server");

    axum::serve(listener, app).await?;

    Ok(())
}
