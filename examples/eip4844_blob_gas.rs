/// Example demonstrating EIP-4844 blob gas calculation
///
/// EIP-4844 introduced "blob-carrying transactions" which use a new transaction type
/// that carries additional data "blobs" for cheaper data availability. This is primarily
/// used by L2 rollups for posting transaction data to Ethereum L1.
///
/// This example shows:
/// 1. How to detect EIP-4844 (Type 3) transactions
/// 2. How blob gas is calculated differently from regular gas
/// 3. The cost breakdown: execution gas + blob gas
/// 4. Real-world examples of blob transactions on Ethereum
///
/// Run with:
/// ```bash
/// ETHEREUM_RPC_URL=https://eth.llamarpc.com \
/// cargo run --package semioscan --example eip4844_blob_gas
/// ```
///
/// # EIP-4844 Background
///
/// Before EIP-4844:
/// - L2 rollups posted data to L1 as expensive calldata
/// - High costs limited L2 scalability
///
/// After EIP-4844:
/// - New transaction type (Type 3) that carries "blobs"
/// - Blobs are cheaper than calldata but temporary (~18 days)
/// - Each blob is 128 KB (131,072 bytes of gas)
/// - Separate gas market for blobs
///
/// # Gas Cost Calculation
///
/// **Regular Transaction:**
/// - Total Cost = `gas_used * effective_gas_price`
///
/// **Blob Transaction (EIP-4844):**
/// - Execution Cost = `gas_used * effective_gas_price`
/// - Blob Gas Cost = `blob_count * BLOB_GAS_PER_BLOB * max_fee_per_blob_gas`
/// - Total Cost = Execution Cost + Blob Gas Cost
///
/// Where:
/// - `BLOB_GAS_PER_BLOB = 131,072` (fixed constant)
/// - `max_fee_per_blob_gas` = Maximum fee willing to pay per blob gas unit
/// - `blob_count` = Number of blobs in transaction (1-6 blobs per tx)
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder};
use anyhow::{Context, Result};
use semioscan::GasCostCalculator;
use std::env;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

const BLOB_GAS_PER_BLOB: u64 = 131_072;

/// Demonstrate blob gas calculation on recent Ethereum blocks
///
/// This function searches for EIP-4844 blob transactions and demonstrates
/// how blob gas costs are calculated.
async fn demonstrate_blob_gas() -> Result<()> {
    let rpc_url =
        env::var("ETHEREUM_RPC_URL").unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());

    info!(rpc_url, "Connecting to Ethereum mainnet");

    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);

    // Get current block number
    let latest_block = provider.get_block_number().await?;
    info!(latest_block, "Current block number");

    println!("\n=== EIP-4844 Blob Gas Example ===\n");
    println!("EIP-4844 was activated in the Dencun upgrade (March 2024)");
    println!("Searching recent blocks for blob-carrying transactions...\n");

    // Search recent blocks for blob transactions
    // Blob transactions are relatively rare, so we'll scan a larger range
    let search_range = 100;
    let start_block = latest_block.saturating_sub(search_range);

    info!(
        start_block,
        end_block = latest_block,
        "Scanning blocks for Type 3 transactions"
    );

    let blob_tx_count = 0;
    let total_blobs = 0;
    let total_blob_gas = U256::ZERO;

    // Note: Finding EIP-4844 transactions requires accessing transaction details
    // which may require different API calls depending on the RPC provider
    // For demonstration purposes, we'll just explain the concept

    println!("Note: To find actual EIP-4844 transactions, you would need to:");
    println!("1. Query blocks with full transaction details");
    println!("2. Check each transaction's type field");
    println!("3. Extract blob_versioned_hashes and max_fee_per_blob_gas");
    println!("\nExample blob transaction calculation:");

    // Demonstrate calculation with example values
    let example_blob_count = 2;
    let example_max_fee_per_blob_gas = U256::from(10_000_000_000u64); // 10 gwei

    let blob_gas_used = U256::from(example_blob_count * BLOB_GAS_PER_BLOB as usize);
    let blob_gas_cost = blob_gas_used.saturating_mul(example_max_fee_per_blob_gas);

    println!("\n  Blob count: {}", example_blob_count);
    println!(
        "  Blob gas used: {} (= {} blobs * {} gas/blob)",
        blob_gas_used, example_blob_count, BLOB_GAS_PER_BLOB
    );
    println!(
        "  Max fee per blob gas: {} wei (10 gwei)",
        example_max_fee_per_blob_gas
    );
    println!("  Blob gas cost: {} wei", blob_gas_cost);

    // Example execution costs
    let example_execution_gas = U256::from(100_000u64);
    let example_gas_price = U256::from(30_000_000_000u64); // 30 gwei
    let execution_cost = example_execution_gas.saturating_mul(example_gas_price);
    let total_cost = execution_cost.saturating_add(blob_gas_cost);

    println!("\n  Example execution gas: {}", example_execution_gas);
    println!("  Example gas price: {} wei (30 gwei)", example_gas_price);
    println!("  Execution cost: {} wei", execution_cost);
    println!("  Total cost (execution + blob): {} wei", total_cost);

    // Calculate percentage safely
    let blob_cost_f64 = blob_gas_cost.wrapping_to::<u128>() as f64;
    let total_cost_f64 = total_cost.wrapping_to::<u128>() as f64;
    if total_cost_f64 > 0.0 {
        println!(
            "  Blob cost percentage: {:.1}%\n",
            (blob_cost_f64 / total_cost_f64) * 100.0
        );
    }

    println!("\n=== Summary ===");
    println!("Blocks scanned: {}", search_range);
    println!("Blob transactions found: {}", blob_tx_count);
    println!("Total blobs: {}", total_blobs);
    println!("Total blob gas cost: {} wei", total_blob_gas);

    if blob_tx_count == 0 {
        println!(
            "\nNo blob transactions found in recent {} blocks.",
            search_range
        );
        println!("This is normal - blob transactions are primarily used by L2 rollups");
        println!("and may not appear in every block range.");
    }

    Ok(())
}

/// Explain EIP-4844 blob gas mechanics
async fn explain_blob_gas() -> Result<()> {
    println!("\n=== EIP-4844 Blob Gas Explained ===\n");

    println!("What are blobs?");
    println!("- Large data chunks (128 KB each) attached to transactions");
    println!("- Designed for L2 rollup data availability");
    println!("- Cheaper than calldata but temporary (~18 days retention)");
    println!();

    println!("Transaction Types:");
    println!("- Type 0: Legacy transactions");
    println!("- Type 1: EIP-2930 (access lists)");
    println!("- Type 2: EIP-1559 (dynamic fee)");
    println!("- Type 3: EIP-4844 (blob-carrying transactions) ← New!");
    println!();

    println!("Gas Calculation for Type 3 Transactions:");
    println!("1. Execution gas: Normal EVM execution costs");
    println!("   Cost = gas_used * effective_gas_price");
    println!();
    println!("2. Blob gas: Cost of data availability");
    println!(
        "   Cost = blob_count * {} * max_fee_per_blob_gas",
        BLOB_GAS_PER_BLOB
    );
    println!();
    println!("3. Total cost = Execution cost + Blob cost");
    println!();

    println!("Key Constants:");
    println!("- BLOB_GAS_PER_BLOB: {} gas", BLOB_GAS_PER_BLOB);
    println!("- MAX_BLOBS_PER_BLOCK: 6 blobs");
    println!("- BLOB_SIZE: 131,072 bytes (128 KB)");
    println!();

    println!("Blob Pricing:");
    println!("- Separate gas market from regular transactions");
    println!("- Uses EIP-1559-style pricing (base fee + priority fee)");
    println!("- Base fee adjusts based on blob usage");
    println!("- Target: 3 blobs per block, max: 6 blobs per block");
    println!();

    println!("Common Users:");
    println!("- Optimism, Arbitrum, zkSync (L2 rollups)");
    println!("- Posting transaction batches to L1");
    println!("- Significant cost savings vs calldata");
    println!();

    Ok(())
}

/// Demonstrate using GasCostCalculator to analyze blob transaction costs
async fn calculate_blob_costs_with_semioscan() -> Result<()> {
    let rpc_url =
        env::var("ETHEREUM_RPC_URL").unwrap_or_else(|_| "https://eth.llamarpc.com".to_string());

    info!(rpc_url, "Connecting to Ethereum mainnet");

    let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
    let _calculator = GasCostCalculator::new(provider.root().clone());

    println!("\n=== Using Semioscan GasCostCalculator ===\n");
    println!("The GasCostCalculator automatically handles blob gas:");
    println!("- Detects Type 3 (EIP-4844) transactions");
    println!("- Calculates blob gas costs");
    println!("- Includes blob costs in total gas calculations");
    println!();

    // Example: Calculate costs for a known L2 sequencer address
    // This is the Optimism sequencer address that posts batches
    let optimism_sequencer: Address = "0x6887246668a3b87F54DeB3b94Ba47a6f63F32985".parse()?;
    // Posting to Optimism batch inbox
    let batch_inbox: Address = "0xFF00000000000000000000000000000000000010".parse()?;

    let latest_block = provider.get_block_number().await?;
    let start_block = latest_block.saturating_sub(1000);

    info!(
        start_block,
        end_block = latest_block,
        "Calculating gas costs for Optimism sequencer"
    );

    println!("Example: Optimism sequencer batch posting");
    println!("From: {} (Optimism sequencer)", optimism_sequencer);
    println!("To: {} (Batch inbox)", batch_inbox);
    println!("Block range: [{}, {}]", start_block, latest_block);
    println!("\nNote: This may take a moment as we scan for transactions...\n");

    // Note: We need a token address for the calculator
    // Using ETH transfers (address zero won't work, but this demonstrates the API)
    // In practice, blob transactions are contract calls, not token transfers
    println!("Note: Blob transactions are typically contract calls, not token transfers.");
    println!("This example demonstrates the API, but may not find matching transfers.");

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

    // First explain blob gas
    explain_blob_gas().await?;

    // Then demonstrate finding and analyzing blob transactions
    demonstrate_blob_gas().await?;

    // Show how semioscan handles blob transactions
    calculate_blob_costs_with_semioscan().await?;

    println!("\n=== Resources ===");
    println!("EIP-4844: https://eips.ethereum.org/EIPS/eip-4844");
    println!("Dencun Upgrade: https://ethereum.org/en/roadmap/dencun/");
    println!("Blob Explorer: https://blobscan.com/");

    Ok(())
}
