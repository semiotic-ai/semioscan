// use std::sync::Arc;

// use alloy_chains::NamedChain;
// use alloy_primitives::Address;
// use axum::{Router, routing::get};
// use common::{Usdc, create_provider};
// use semioswap::{get_token_price, health_check, AppState, PriceCalculator};
// use tokio::sync::Mutex;

// #[tokio::main]
// async fn main() {
//     // Load environment variables
//     dotenvy::dotenv().ok();

//     // Initialize tracing
//     tracing_subscriber::fmt()
//         .with_max_level(tracing::Level::DEBUG)
//         .init();

//     let chain = NamedChain::Base;

//     // Get configuration from environment
//     let rpc_url = std::env::var("RPC_URL").expect("RPC_URL must be set");
//     let router_address_str = std::env::var("ROUTER_ADDRESS").expect("ROUTER_ADDRESS must be set");
//     let usdc_address = chain.usdc_address();

//     // Parse addresses
//     let router_address = router_address_str
//         .parse::<Address>()
//         .expect("Invalid ROUTER_ADDRESS");

//     // Initialize price calculator
//     let calculator = PriceCalculator::new(
//         router_address,
//         usdc_address,
//         create_provider(chain, common::Signer::V2).expect("Failed to create provider"),
//     )
//     .await
//     .expect("Failed to create price calculator");

//     // Create application state
//     let state = Arc::new(AppState {
//         calculator: Arc::new(Mutex::new(calculator)),
//         router_address,
//         usdc_address,
//     });

//     // Build router
//     let app = Router::new()
//         .route("/api/v1/token/price", get(get_token_price))
//         .route("/health", get(health_check))
//         .with_state(state);

//     // Get port
//     let port = std::env::var("PORT")
//         .unwrap_or_else(|_| "3000".to_string())
//         .parse()
//         .unwrap_or(3000);

//     let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
//         .await
//         .unwrap();

//     let addr = listener.local_addr().unwrap();

//     tracing::info!("Starting server on {}", addr);

//     // Start server
//     axum::serve(listener, app)
//         .await
//         .expect("Failed to start server");
// }

use std::process::ExitCode;

use semioscan::bootstrap::run;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    if let Err(e) = run().await {
        tracing::error!("Clearing Job error: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
