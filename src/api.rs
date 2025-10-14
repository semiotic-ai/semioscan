use alloy_primitives::{address, Address};
use axum::{routing::get, Router};
use tokio::net::TcpListener;

use crate::{command::SemioscanHandle, get_lo_price, get_v2_price};

const V2_LIQUIDATOR_ADDRESS: Address = address!("498020622CA0d5De103b7E78E3eFe5819D0d28AB");
// TODO: support pre-v2 routers
const LO_LIQUIDATOR_ADDRESS: Address = address!("9aA30b2289020f9de59D39fBd7Bd5f3BE661a2a6");

/// Router type enum
#[derive(Debug, Clone, Copy)]
pub enum RouterType {
    V2,
    LimitOrder,
}

impl RouterType {
    pub fn address(&self) -> Address {
        match self {
            Self::V2 => V2_LIQUIDATOR_ADDRESS,
            Self::LimitOrder => LO_LIQUIDATOR_ADDRESS,
        }
    }
}

/// Starts the API server.
pub async fn serve_api(listener: TcpListener, price_job: SemioscanHandle) -> anyhow::Result<()> {
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
