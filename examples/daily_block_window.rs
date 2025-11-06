use alloy_chains::NamedChain;
/// Example demonstrating how to calculate daily block windows for blockchain queries
///
/// This example shows how to:
/// 1. Create a BlockWindowCalculator for a specific chain
/// 2. Calculate the block range for a specific UTC day
/// 3. Use the cached results for repeated queries
/// 4. Integrate with existing semioscan tools like CombinedCalculator
///
/// Run with:
/// ```bash
/// CHAIN_ID=42161 \
/// RPC_URL=https://arb1.arbitrum.io/rpc/ \
/// API_KEY=your_api_key \
/// DAY=2025-10-10 \
/// CACHE_PATH=block_windows.json \
/// cargo run --package semioscan --example daily_block_window
/// ```
///
/// Note: CHAIN_ID must be provided as config because some chains (e.g., Avalanche)
/// don't support the get_chain_id() RPC method.
use alloy_provider::ProviderBuilder;
use anyhow::{Context, Result};
use chrono::NaiveDate;
use semioscan::BlockWindowCalculator;
use std::env;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    dotenvy::dotenv().ok();

    // Read configuration from environment
    let rpc_url = env::var("RPC_URL").context("RPC_URL environment variable not set")?;
    let api_key = env::var("API_KEY").context("API_KEY environment variable not set")?;
    let day_str = env::var("DAY").unwrap_or_else(|_| "2025-10-16".to_string());
    let cache_path = env::var("CACHE_PATH").unwrap_or_else(|_| "block_windows.json".to_string());

    // Combine RPC URL with API key (trailing slash is important for Pinax endpoint)
    let full_rpc_url = format!("{rpc_url}{api_key}/");

    info!(day_str, cache_path, "Starting daily block window example");

    // Parse the date
    let date = NaiveDate::parse_from_str(&day_str, "%Y-%m-%d")
        .context("Failed to parse DAY (expected format: YYYY-MM-DD)")?;

    // Create provider
    let provider = ProviderBuilder::new().connect_http(full_rpc_url.parse()?);

    // Create calculator
    let calculator = BlockWindowCalculator::new(provider.clone(), cache_path.clone());

    // Calculate daily window
    // Note: Chain ID is injected from config rather than queried from provider
    // because some chains (e.g., Avalanche) don't support get_chain_id()
    info!(chain = ?NamedChain::Arbitrum, date = %date, "Calculating daily block window");
    let window = calculator
        .get_daily_window(NamedChain::Arbitrum, date)
        .await?;

    info!(
        date = %date,
        start_block = window.start_block,
        end_block = window.end_block,
        block_count = window.block_count(),
        start_ts = %window.start_ts,
        end_ts_exclusive = %window.end_ts_exclusive,
        "Daily block window calculated"
    );

    println!("\n=== Daily Block Window ===");
    println!("Date: {date}");
    println!(
        "Block range: [{}, {}] (inclusive)",
        window.start_block, window.end_block
    );
    println!("Block count: {}", window.block_count());
    println!(
        "UTC start: {} ({})",
        window.start_ts,
        chrono::DateTime::from_timestamp(window.start_ts.0, 0).unwrap()
    );
    println!(
        "UTC end (exclusive): {} ({})",
        window.end_ts_exclusive,
        chrono::DateTime::from_timestamp(window.end_ts_exclusive.0, 0).unwrap()
    );

    Ok(())
}
