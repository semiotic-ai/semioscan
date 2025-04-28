use dotenvy::dotenv;
use tokio::net::TcpListener;

use crate::{PriceJob, serve_api};

/// Main entry point for the application.
pub async fn run() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    // Configure API port from environment variables
    let port = dotenvy::var("API_PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = TcpListener::bind(&format!("0.0.0.0:{port}")).await?;

    // Initialize the PriceJob for handling price queries
    let price_job_handle = PriceJob::init();

    // Start the API server
    serve_api(listener, price_job_handle).await?;

    Ok(())
}
