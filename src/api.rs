#[cfg(all(feature = "api-server", feature = "odos-example"))]
use axum::{routing::get, Router};
#[cfg(all(feature = "api-server", feature = "odos-example"))]
use tokio::net::TcpListener;

#[cfg(all(feature = "api-server", feature = "odos-example"))]
use crate::{command::SemioscanHandle, get_lo_price, get_v2_price};

/// Starts the API server.
/// Requires both api-server and odos-example features as it serves Odos price endpoints.
#[cfg(all(feature = "api-server", feature = "odos-example"))]
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
