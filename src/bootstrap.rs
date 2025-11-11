use alloy_chains::NamedChain;
use alloy_primitives::Address;
use alloy_provider::ProviderBuilder;
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use dotenvy::dotenv;
use std::env;
use tokio::net::TcpListener;

use crate::{
    serve_api, BlockWindowCalculator, CalculateCombinedDataCommand, CalculateGasCommand,
    CalculatePriceCommand, CalculateTransferAmountCommand, Command, CommandHandler, RouterType,
    SemioscanConfig, SemioscanConfigBuilder, SupportedEvent,
};
use std::time::Duration;

/// Configuration preset options
#[derive(Debug, Clone, Copy)]
enum ConfigPreset {
    /// Default configuration optimized for Alchemy/Infura
    Default,
    /// Minimal configuration with no rate limiting
    Minimal,
}

/// Parse configuration preset from string
fn parse_config_preset(s: &str) -> Result<ConfigPreset, String> {
    match s.to_lowercase().as_str() {
        "default" => Ok(ConfigPreset::Default),
        "minimal" => Ok(ConfigPreset::Minimal),
        _ => Err(format!(
            "Invalid config preset '{s}'. Valid options: 'default', 'minimal'"
        )),
    }
}

/// Parse chain:blocks override from string (e.g., "base:1000")
fn parse_chain_max_blocks(s: &str) -> Result<(NamedChain, u64), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid format '{s}'. Expected 'chain_name:blocks' (e.g., 'base:1000')"
        ));
    }

    let chain = parts[0]
        .parse::<NamedChain>()
        .map_err(|_| format!("Invalid chain name: '{}'", parts[0]))?;

    let blocks = parts[1]
        .parse::<u64>()
        .map_err(|_| format!("Invalid block count: '{}'", parts[1]))?;

    Ok((chain, blocks))
}

/// Parse chain:millis rate limit override from string (e.g., "base:250")
fn parse_chain_rate_limit(s: &str) -> Result<(NamedChain, u64), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid format '{s}'. Expected 'chain_name:millis' (e.g., 'base:250')"
        ));
    }

    let chain = parts[0]
        .parse::<NamedChain>()
        .map_err(|_| format!("Invalid chain name: '{}'", parts[0]))?;

    let millis = parts[1]
        .parse::<u64>()
        .map_err(|_| format!("Invalid milliseconds: '{}'", parts[1]))?;

    Ok((chain, millis))
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Maximum number of blocks to query in a single RPC call (default: 500)
    #[arg(long, global = true)]
    max_block_range: Option<u64>,

    /// Rate limit delay in milliseconds between RPC requests
    #[arg(long, global = true)]
    rate_limit_delay: Option<u64>,

    /// Configuration preset: "default" (Alchemy/Infura optimized) or "minimal" (no rate limiting)
    #[arg(long, global = true, value_parser = parse_config_preset)]
    config_preset: Option<ConfigPreset>,

    /// Per-chain max block range override (format: chain_name:blocks, e.g., "base:1000")
    #[arg(long, global = true, value_parser = parse_chain_max_blocks)]
    chain_max_blocks: Vec<(NamedChain, u64)>,

    /// Per-chain rate limit override in milliseconds (format: chain_name:millis, e.g., "base:250")
    #[arg(long, global = true, value_parser = parse_chain_rate_limit)]
    chain_rate_limit: Vec<(NamedChain, u64)>,
}

/// Commands for the Semioscan CLI
#[derive(Subcommand)]
enum Commands {
    /// Start the API server
    Api {
        #[arg(short, long, default_value = "3000")]
        port: String,
    },
    /// Port to run the API server on
    /// Calculate token price for a given block range
    Price {
        /// Chain ID to query
        #[arg(long)]
        chain: NamedChain,
        /// Token address to query
        #[arg(long)]
        token_address: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
        /// Router type (v2 or lo)
        #[arg(long, value_parser = parse_router_type)]
        router_type: RouterType,
    },
    /// Calculate gas cost for a given block range
    Gas {
        /// Chain ID to query
        #[arg(long)]
        chain: NamedChain,
        /// From address to query. (The alias '--router' is deprecated, please use '--from')
        #[arg(long, alias = "router")]
        from: Address,
        /// To address to query
        #[arg(long)]
        to: Address,
        /// Token address to query
        #[arg(long)]
        token: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
        /// Event type to filter for (e.g., transfer, approval)
        #[arg(long, value_enum)]
        event: SupportedEvent,
    },
    /// Calculate the amount of a token transferred to a recipient
    /// for a given block range
    TransferAmount {
        /// Chain ID to query
        #[arg(long)]
        chain: NamedChain,
        /// From address to query. (The alias '--router' is deprecated, please use '--from')
        #[arg(long, alias = "router")]
        from: Address,
        /// Recipient address
        #[arg(long)]
        to: Address,
        /// Token address
        #[arg(long)]
        token: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
    },
    /// Calculate the combined gas and transfer amount data for a given block range
    Combined {
        /// Chain ID to query
        #[arg(long)]
        chain: NamedChain,
        /// From address
        #[arg(long)]
        from: Address,
        /// To address
        #[arg(long)]
        to: Address,
        /// Token address
        #[arg(long)]
        token: Address,
        /// Starting block number
        #[arg(long)]
        from_block: u64,
        /// Ending block number
        #[arg(long)]
        to_block: u64,
        /// Output format: json or debug (default)
        #[arg(long, default_value = "debug")]
        format: String,
    },
    /// Calculate the block range for a given date (UTC)
    BlockWindow {
        /// Chain ID to query
        #[arg(long)]
        chain: NamedChain,
        /// Date to query (format: YYYY-MM-DD)
        #[arg(long)]
        date: String,
        /// Optional cache file path (defaults to block_windows.json)
        #[arg(long)]
        cache_path: Option<String>,
        /// Output format: plain (just block numbers), json, or human-readable (default)
        #[arg(long, default_value = "human")]
        format: String,
    },
}

/// Build SemioscanConfig from CLI arguments and environment variables
///
/// Priority order (highest to lowest):
/// 1. CLI arguments
/// 2. Environment variables
/// 3. Preset (if specified via CLI or env)
/// 4. Default preset
fn build_semioscan_config(cli: &Cli) -> SemioscanConfig {
    // Determine the base preset from CLI or environment
    let preset = cli
        .config_preset
        .or_else(|| {
            env::var("SEMIOSCAN_CONFIG_PRESET")
                .ok()
                .and_then(|s| parse_config_preset(&s).ok())
        })
        .unwrap_or(ConfigPreset::Default);

    // Start with the preset
    let mut builder = match preset {
        ConfigPreset::Default => SemioscanConfigBuilder::with_defaults(),
        ConfigPreset::Minimal => SemioscanConfigBuilder::new(),
    };

    // Apply environment variable overrides
    if let Ok(max_blocks) = env::var("SEMIOSCAN_MAX_BLOCK_RANGE") {
        if let Ok(max_blocks) = max_blocks.parse::<u64>() {
            builder = builder.max_block_range(max_blocks);
        }
    }

    if let Ok(delay_ms) = env::var("SEMIOSCAN_RATE_LIMIT_DELAY") {
        if let Ok(delay_ms) = delay_ms.parse::<u64>() {
            builder = builder.rate_limit_delay(Duration::from_millis(delay_ms));
        }
    }

    // Apply CLI argument overrides (highest priority)
    if let Some(max_blocks) = cli.max_block_range {
        builder = builder.max_block_range(max_blocks);
    }

    if let Some(delay_ms) = cli.rate_limit_delay {
        builder = builder.rate_limit_delay(Duration::from_millis(delay_ms));
    }

    // Apply per-chain overrides
    for (chain, blocks) in &cli.chain_max_blocks {
        builder = builder.chain_max_blocks(*chain, *blocks);
    }

    for (chain, millis) in &cli.chain_rate_limit {
        builder = builder.chain_rate_limit(*chain, Duration::from_millis(*millis));
    }

    builder.build()
}

/// Parse router type from string using type-safe mapping
///
/// Note: This requires the `odos-example` feature (automatically enabled with `cli`)
fn parse_router_type(s: &str) -> Result<RouterType, String> {
    // Define all valid router type mappings
    const ROUTER_TYPE_MAPPINGS: &[(RouterType, &[&str])] = &[
        (RouterType::V2, &["v2"]),
        (RouterType::LimitOrder, &["lo", "limitorder", "limit-order"]),
        (RouterType::V3, &["v3"]),
    ];

    let input = s.to_lowercase();

    // Find matching router type
    for (router_type, aliases) in ROUTER_TYPE_MAPPINGS {
        if aliases.iter().any(|&alias| alias == input) {
            return Ok(*router_type);
        }
    }

    // Generate error message listing all valid options
    let valid_options: Vec<String> = ROUTER_TYPE_MAPPINGS
        .iter()
        .flat_map(|(_, aliases)| aliases.iter().map(|s| format!("'{s}'")))
        .collect();

    Err(format!(
        "Invalid router type '{s}'. Valid options: {}",
        valid_options.join(", ")
    ))
}

/// Main entry point for the application.
pub async fn run() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    let cli = Cli::parse();

    // Build configuration from CLI args and environment variables
    let config = build_semioscan_config(&cli);

    match cli.command {
        Commands::Api { port } => {
            // Start the API server
            let listener = TcpListener::bind(&format!("0.0.0.0:{port}")).await?;
            let price_job_handle = CommandHandler::init(config);
            serve_api(listener, price_job_handle).await?;
        }
        Commands::Price {
            chain,
            token_address,
            from_block,
            to_block,
            router_type,
        } => {
            // Initialize price job for CLI usage
            let price_job_handle = CommandHandler::init(config.clone());

            // Create a oneshot channel for the response
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            // Send the price calculation command
            price_job_handle
                .tx
                .send(Command::CalculatePrice(CalculatePriceCommand {
                    chain,
                    router_type,
                    token_address,
                    from_block,
                    to_block,
                    responder: responder_tx,
                }))
                .await?;

            // Wait for and print the result
            match responder_rx.await? {
                Ok(result) => {
                    println!("Average price: {}", result.get_average_price());
                    println!("Total token amount: {}", result.total_token_amount());
                    println!("Total USDC amount: {}", result.total_usdc_amount());
                    println!("Transaction count: {}", result.transaction_count());
                }
                Err(e) => {
                    eprintln!("Error calculating price: {e}");
                    return Err(anyhow::anyhow!(e));
                }
            }
        }
        Commands::Gas {
            chain,
            from,
            to,
            token,
            from_block,
            to_block,
            event,
        } => {
            let price_job_handle = CommandHandler::init(config.clone());
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            price_job_handle
                .tx
                .send(Command::CalculateGas(CalculateGasCommand {
                    chain,
                    from,
                    to,
                    token,
                    from_block,
                    to_block,
                    event,
                    responder: responder_tx,
                }))
                .await?;

            match responder_rx.await? {
                Ok(result) => {
                    println!("Gas cost: {}", result.formatted_gas_cost());
                    println!("Transaction count: {}", result.transaction_count);
                }
                Err(e) => {
                    eprintln!("Error calculating gas cost: {e}");
                }
            }
        }
        Commands::TransferAmount {
            chain,
            from,
            to,
            token,
            from_block,
            to_block,
        } => {
            let price_job_handle = CommandHandler::init(config.clone());
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            price_job_handle
                .tx
                .send(Command::CalculateTransferAmount(
                    CalculateTransferAmountCommand {
                        chain,
                        from,
                        to,
                        token,
                        from_block,
                        to_block,
                        responder: responder_tx,
                    },
                ))
                .await?;

            match responder_rx.await? {
                Ok(result) => {
                    println!("Amount: {}", result.amount);
                }
                Err(e) => {
                    eprintln!("Error calculating amount: {e}");
                }
            }
        }
        Commands::Combined {
            chain,
            from,
            to,
            token,
            from_block,
            to_block,
            format,
        } => {
            let price_job_handle = CommandHandler::init(config.clone());
            let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();

            price_job_handle
                .tx
                .send(Command::CalculateCombinedData(
                    CalculateCombinedDataCommand {
                        chain,
                        from,
                        to,
                        token,
                        from_block,
                        to_block,
                        responder: responder_tx,
                    },
                ))
                .await?;

            match responder_rx.await? {
                Ok(result) => {
                    match format.to_lowercase().as_str() {
                        "json" => {
                            // Output as JSON with human-readable values
                            // Query token decimals on-chain
                            use crate::provider::create_ethereum_provider;
                            use erc20_rs::Erc20;

                            let provider = create_ethereum_provider(chain)?;
                            let token_contract = Erc20::new(token, provider);
                            let decimals = token_contract.decimals().await?;

                            let display = result.to_display(decimals);
                            println!("{}", serde_json::to_string_pretty(&display)?);
                        }
                        "debug" => {
                            // Debug format (default)
                            println!("Combined data: {result:?}");
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Invalid format: {format}. Use 'json' or 'debug'"
                            ));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error calculating combined data: {e}");
                }
            }
        }
        Commands::BlockWindow {
            chain,
            date,
            cache_path,
            format,
        } => {
            // Parse the date
            let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| {
                anyhow::anyhow!("Failed to parse date (expected format: YYYY-MM-DD): {e}")
            })?;

            // Get RPC URL and API key from environment
            let rpc_url = env::var("RPC_URL")
                .map_err(|_| anyhow::anyhow!("RPC_URL environment variable not set"))?;
            let api_key = env::var("API_KEY")
                .map_err(|_| anyhow::anyhow!("API_KEY environment variable not set"))?;

            // Combine RPC URL with API key (trailing slash is important for Pinax endpoint)
            let full_rpc_url = format!("{rpc_url}{api_key}/");

            // Create provider
            let provider = ProviderBuilder::new().connect_http(
                full_rpc_url
                    .parse()
                    .map_err(|e| anyhow::anyhow!("Failed to parse RPC URL: {e}"))?,
            );

            // Use provided cache path or default
            let cache_path = cache_path.unwrap_or_else(|| "block_windows.json".to_string());

            // Create calculator and get daily window
            let calculator = BlockWindowCalculator::new(provider, cache_path);
            let window = calculator.get_daily_window(chain, date).await?;

            // Output based on format
            match format.to_lowercase().as_str() {
                "plain" => {
                    // Just output the block range for easy piping
                    println!("{} {}", window.start_block, window.end_block);
                }
                "json" => {
                    // Output as JSON
                    println!("{}", serde_json::to_string_pretty(&window)?);
                }
                "human" => {
                    // Human-readable output
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
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid format: {format}. Use 'plain', 'json', or 'human'"
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_preset() {
        assert!(matches!(
            parse_config_preset("default"),
            Ok(ConfigPreset::Default)
        ));
        assert!(matches!(
            parse_config_preset("minimal"),
            Ok(ConfigPreset::Minimal)
        ));
        assert!(parse_config_preset("invalid").is_err());
    }

    #[test]
    fn test_parse_chain_max_blocks() {
        let result = parse_chain_max_blocks("base:1000");
        assert!(result.is_ok());
        let (chain, blocks) = result.unwrap();
        assert_eq!(chain, NamedChain::Base);
        assert_eq!(blocks, 1000);

        // Invalid formats
        assert!(parse_chain_max_blocks("invalid").is_err());
        assert!(parse_chain_max_blocks("base:abc").is_err());
        assert!(parse_chain_max_blocks("invalidchain:1000").is_err());
    }

    #[test]
    fn test_parse_chain_rate_limit() {
        let result = parse_chain_rate_limit("arbitrum:250");
        assert!(result.is_ok());
        let (chain, millis) = result.unwrap();
        assert_eq!(chain, NamedChain::Arbitrum);
        assert_eq!(millis, 250);

        // Invalid formats
        assert!(parse_chain_rate_limit("invalid").is_err());
        assert!(parse_chain_rate_limit("arbitrum:abc").is_err());
    }
}
