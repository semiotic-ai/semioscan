/// Example demonstrating router token discovery using extract_transferred_to_tokens
///
/// This example shows:
/// 1. How to discover all tokens transferred to a router contract
/// 2. How to use extract_transferred_to_tokens for token inventory
/// 3. Real-world usage patterns for liquidation systems
/// 4. Performance considerations for large block ranges
///
/// Run with:
/// ```bash
/// # Discover tokens on Arbitrum Odos router
/// ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/ \
/// cargo run --package semioscan --example router_token_discovery -- arbitrum
///
/// # Discover tokens on Base Odos router
/// BASE_RPC_URL=https://mainnet.base.org \
/// cargo run --package semioscan --example router_token_discovery -- base
///
/// # Discover tokens with custom block range
/// ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/ \
/// START_BLOCK=270000000 \
/// END_BLOCK=270010000 \
/// cargo run --package semioscan --example router_token_discovery -- arbitrum --custom-range
/// ```
///
/// # Use Cases
///
/// 1. **Liquidation Systems**: Discover tokens that need to be liquidated from router contracts
/// 2. **Token Inventory**: Track all tokens that have been sent to a contract
/// 3. **Analytics**: Analyze token flow patterns
/// 4. **Monitoring**: Alert when new tokens appear in a contract
///
/// # How It Works
///
/// The `extract_transferred_to_tokens` function:
/// 1. Scans blockchain logs for ERC-20 Transfer events
/// 2. Filters for transfers where the `to` address matches the router
/// 3. Returns a deduplicated set of token addresses
/// 4. Handles rate limiting and chunking automatically
use alloy_chains::NamedChain;
use alloy_primitives::Address;
use alloy_provider::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use semioscan::extract_transferred_to_tokens;
use std::env;
use std::time::Instant;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

/// Discover tokens on Arbitrum Odos V2 Router
async fn discover_arbitrum_tokens(custom_range: bool) -> Result<()> {
    let (rpc_url, api_key) = (
        env::var("ARBITRUM_RPC_URL")
            .unwrap_or_else(|_| "https://arb1.arbitrum.io/rpc/".to_string()),
        env::var("API_KEY").context("API_KEY environment variable not set")?,
    );

    let full_rpc_url = format!("{rpc_url}{api_key}/");

    info!(chain = "Arbitrum", "Connecting to chain");

    let provider = ProviderBuilder::new().connect_http(full_rpc_url.parse()?);

    // Odos V2 Router on Arbitrum
    let router: Address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13".parse()?;

    let (start_block, end_block) = if custom_range {
        let start = env::var("START_BLOCK")
            .context("START_BLOCK required for custom range")?
            .parse()?;
        let end = env::var("END_BLOCK")
            .context("END_BLOCK required for custom range")?
            .parse()?;
        (start, end)
    } else {
        // Use recent blocks
        let latest = provider.get_block_number().await?;
        let start = latest.saturating_sub(10_000); // Last ~10k blocks
        (start, latest)
    };

    println!("\n=== Token Discovery on Arbitrum ===");
    println!("Router: {}", router);
    println!("Block range: [{}, {}]", start_block, end_block);
    println!("Blocks to scan: {}", end_block - start_block + 1);
    println!();

    info!(start_block, end_block, ?router, "Starting token discovery");

    let start_time = Instant::now();
    let tokens = extract_transferred_to_tokens(
        &provider,
        NamedChain::Arbitrum,
        router,
        start_block,
        end_block,
    )
    .await?;
    let elapsed = start_time.elapsed();

    println!("=== Results ===");
    println!("Tokens discovered: {}", tokens.len());
    println!("Scan time: {:?}", elapsed);
    println!(
        "Performance: {:.2} blocks/sec",
        (end_block - start_block + 1) as f64 / elapsed.as_secs_f64()
    );
    println!();

    if !tokens.is_empty() {
        println!("Token addresses (first 10):");
        for (i, token) in tokens.iter().take(10).enumerate() {
            println!("  {}. {}", i + 1, token);
        }
        if tokens.len() > 10 {
            println!("  ... and {} more", tokens.len() - 10);
        }
    } else {
        println!("No tokens found in this range.");
    }

    Ok(())
}

/// Discover tokens on Base Odos V2 Router
async fn discover_base_tokens() -> Result<()> {
    let (rpc_url, api_key) = (
        env::var("BASE_RPC_URL").unwrap_or_else(|_| "https://mainnet.base.org".to_string()),
        env::var("API_KEY").context("API_KEY environment variable not set")?,
    );

    let full_rpc_url = format!("{rpc_url}{api_key}/");

    info!(chain = "Base", "Connecting to chain");

    let provider = ProviderBuilder::new().connect_http(full_rpc_url.parse()?);

    // Odos V2 Router on Base
    let router: Address = "0x19cEeAd7105607Cd444F5ad10dd51356436095a1".parse()?;

    let latest = provider.get_block_number().await?;
    let start_block = latest.saturating_sub(10_000);
    let end_block = latest;

    println!("\n=== Token Discovery on Base ===");
    println!("Router: {}", router);
    println!("Block range: [{}, {}]", start_block, end_block);
    println!("Blocks to scan: {}", end_block - start_block + 1);
    println!();

    info!(start_block, end_block, ?router, "Starting token discovery");

    let start_time = Instant::now();
    let tokens =
        extract_transferred_to_tokens(&provider, NamedChain::Base, router, start_block, end_block)
            .await?;
    let elapsed = start_time.elapsed();

    println!("=== Results ===");
    println!("Tokens discovered: {}", tokens.len());
    println!("Scan time: {:?}", elapsed);
    println!(
        "Performance: {:.2} blocks/sec",
        (end_block - start_block + 1) as f64 / elapsed.as_secs_f64()
    );
    println!();

    if !tokens.is_empty() {
        println!("Token addresses (first 10):");
        for (i, token) in tokens.iter().take(10).enumerate() {
            println!("  {}. {}", i + 1, token);
        }
        if tokens.len() > 10 {
            println!("  ... and {} more", tokens.len() - 10);
        }
    } else {
        println!("No tokens found in this range.");
    }

    Ok(())
}

/// Demonstrate comparing token sets across different time periods
async fn demonstrate_temporal_analysis() -> Result<()> {
    println!("\n=== Temporal Token Analysis Pattern ===\n");

    println!("Use Case: Track new tokens appearing in router");
    println!("1. Scan last 24 hours worth of blocks");
    println!("2. Scan previous 24 hours");
    println!("3. Compare sets to find new tokens");
    println!();

    println!("Example Code:");
    println!("```rust");
    println!("// Get current block");
    println!("let latest = provider.get_block_number().await?;");
    println!();
    println!("// Assuming ~2 sec/block on L2");
    println!("let blocks_per_day = 43_200; // (24 * 60 * 60) / 2");
    println!();
    println!("// Scan last 24 hours");
    println!("let recent_tokens = extract_transferred_to_tokens(");
    println!("    &provider,");
    println!("    chain,");
    println!("    router,");
    println!("    latest - blocks_per_day,");
    println!("    latest,");
    println!(").await?;");
    println!();
    println!("// Scan previous 24 hours");
    println!("let previous_tokens = extract_transferred_to_tokens(");
    println!("    &provider,");
    println!("    chain,");
    println!("    router,");
    println!("    latest - (2 * blocks_per_day),");
    println!("    latest - blocks_per_day,");
    println!(").await?;");
    println!();
    println!("// Find new tokens");
    println!("let new_tokens: BTreeSet<_> = recent_tokens");
    println!("    .difference(&previous_tokens)");
    println!("    .copied()");
    println!("    .collect();");
    println!();
    println!("println!(\"New tokens: {{}}\", new_tokens.len());");
    println!("```");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let chain = args.get(1).map(|s| s.as_str()).unwrap_or("arbitrum");
    let custom_range = args.get(2).map(|s| s.as_str()) == Some("--custom-range");

    match chain {
        "arbitrum" | "arb" => discover_arbitrum_tokens(custom_range).await?,
        "base" => discover_base_tokens().await?,
        "temporal" => demonstrate_temporal_analysis().await?,
        _ => {
            eprintln!("Unknown chain: {}", chain);
            eprintln!(
                "Usage: {} [arbitrum|base|temporal] [--custom-range]",
                args[0]
            );
            std::process::exit(1);
        }
    }

    println!("\n=== Performance Notes ===");
    println!("- Function uses automatic chunking based on chain");
    println!("- Respects rate limits for RPC endpoints");
    println!("- Returns deduplicated set of token addresses");
    println!("- Suitable for large block ranges (tested up to 1M+ blocks)");
    println!();
    println!("=== Configuration ===");
    println!("- Block ranges per request: Chain-specific (see SemioscanConfig)");
    println!("- Rate limiting: Automatic per chain");
    println!("- Deduplication: Built-in using BTreeSet");
    println!();
    println!("=== Integration Tips ===");
    println!("1. For liquidation: Run periodically to discover new tokens");
    println!("2. For analytics: Store results in database with timestamps");
    println!("3. For monitoring: Alert when high-value tokens appear");
    println!("4. For history: Scan from deployment block to current");

    Ok(())
}
