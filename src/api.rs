#[cfg(feature = "api-server")]
use axum::{routing::get, Router};
#[cfg(feature = "api-server")]
use tokio::net::TcpListener;

#[cfg(feature = "api-server")]
use crate::{command::SemioscanHandle, get_lo_price, get_v2_price};

/// Starts the API server.
#[cfg(feature = "api-server")]
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
