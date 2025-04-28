use alloy_primitives::Address;
use tokio::net::TcpListener;
use tracing::info;

use crate::{CalculatePriceCommand, Command, job::PriceJobHandle};

use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

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
