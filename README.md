# Semioscan

[![Crates.io](https://img.shields.io/crates/v/semioscan.svg)](https://crates.io/crates/semioscan)
[![Documentation](https://docs.rs/semioscan/badge.svg)](https://docs.rs/semioscan)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

**Semioscan** is a Rust library for blockchain analytics, providing production-grade tools for calculating gas costs, extracting price data from DEX swaps, and working with block ranges across multiple EVM-compatible chains.

## Features

- **Gas Cost Calculation**: Accurately calculate transaction gas costs for both L1 (Ethereum) and L2 (Optimism Stack) chains, including L1 data fees
- **Block Window Calculations**: Map UTC dates to blockchain block ranges with intelligent caching
- **DEX Price Extraction**: Extensible trait-based system for extracting price data from on-chain swap events
- **Multi-Chain Support**: Works with 12+ EVM chains including Ethereum, Arbitrum, Base, Optimism, Polygon, and more
- **Event Scanning**: Extract transfer amounts and events from blockchain transaction logs
- **Production-Ready**: Battle-tested in production for automated liquidation systems processing millions of dollars in swaps

## Installation

Add semioscan to your `Cargo.toml`:

```toml
[dependencies]
# Core library (gas, block windows, events)
semioscan = "0.2"

# With Odos DEX reference implementation
semioscan = { version = "0.2", features = ["odos-example"] }
```

### Feature Flags

- **`odos-example`** (default): Includes `OdosPriceSource` as a reference implementation of the `PriceSource` trait for Odos DEX aggregator

## Quick Start

### 1. Calculate Gas Costs

Calculate total gas costs for transactions between two addresses:

```rust
use semioscan::GasCalculator;
use alloy_provider::ProviderBuilder;
use alloy_chains::NamedChain;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create provider
    let provider = ProviderBuilder::new()
        .connect_http("https://arb1.arbitrum.io/rpc".parse()?);

    // Create gas calculator
    let calculator = GasCalculator::new(provider.clone());

    // Calculate gas costs for a block range
    let from_address = "0x123...".parse()?;
    let to_address = "0x456...".parse()?;
    let chain_id = NamedChain::Arbitrum as u64;

    let result = calculator
        .get_gas_cost(chain_id, from_address, to_address, 200_000_000, 200_001_000)
        .await?;

    println!("Total gas cost: {} wei", result.total_gas_cost);
    println!("Transaction count: {}", result.transaction_count);

    Ok(())
}
```

**L2 chains** (Arbitrum, Base, Optimism) automatically include L1 data fees in the calculation.

### 2. Calculate Daily Block Windows

Map a UTC date to the corresponding blockchain block range:

```rust
use semioscan::BlockWindowCalculator;
use alloy_provider::ProviderBuilder;
use alloy_chains::NamedChain;
use chrono::NaiveDate;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create provider
    let provider = ProviderBuilder::new()
        .connect_http("https://arb1.arbitrum.io/rpc".parse()?);

    // Create calculator with cache file
    let calculator = BlockWindowCalculator::new(
        provider.clone(),
        "block_windows.json".to_string()
    );

    // Get block window for a specific day
    let date = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
    let window = calculator
        .get_daily_window(NamedChain::Arbitrum, date)
        .await?;

    println!("Date: {}", date);
    println!("Block range: [{}, {}]", window.start_block, window.end_block);
    println!("Block count: {}", window.block_count());

    Ok(())
}
```

**Caching**: Block windows are automatically cached to disk for faster subsequent queries.

### 3. Extract DEX Price Data

Use the `PriceSource` trait to extract price data from on-chain swap events:

```rust
use semioscan::price::odos::OdosPriceSource;  // requires "odos-example" feature
use semioscan::PriceCalculator;
use alloy_provider::ProviderBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create provider
    let provider = ProviderBuilder::new()
        .connect_http("https://arb1.arbitrum.io/rpc".parse()?);

    // Create Odos price source for V2 router
    let router_address = "0xa669e7A0d4b3e4Fa48af2dE86BD4CD7126Be4e13".parse()?;
    let price_source = OdosPriceSource::new(router_address);

    // Create price calculator with the price source
    let calculator = PriceCalculator::with_price_source(
        provider.clone(),
        Box::new(price_source)
    );

    // Calculate average price for a token over a block range
    let token_address = "0x789...".parse()?;
    let result = calculator
        .get_price(token_address, 200_000_000, 200_001_000)
        .await?;

    println!("Average price: {}", result.average_price);
    println!("Total volume: {}", result.total_volume_in);

    Ok(())
}
```

## Implementing Custom Price Sources

Semioscan uses a trait-based architecture that allows you to implement price extraction for **any DEX protocol**. The `PriceSource` trait is object-safe and designed for easy extensibility.

### Example: Uniswap V3 Price Source

```rust
use semioscan::price::{PriceSource, SwapData, PriceSourceError};
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::Log;
use alloy_sol_types::sol;

// Define Uniswap V3 Swap event
sol! {
    event SwapV3(
        address indexed sender,
        address indexed recipient,
        int256 amount0,
        int256 amount1,
        uint160 sqrtPriceX96,
        uint128 liquidity,
        int24 tick
    );
}

pub struct UniswapV3PriceSource {
    pool_address: Address,
    token0: Address,
    token1: Address,
}

impl PriceSource for UniswapV3PriceSource {
    fn router_address(&self) -> Address {
        self.pool_address
    }

    fn event_topics(&self) -> Vec<B256> {
        vec![SwapV3::SIGNATURE_HASH]
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapV3::decode_log(&log.into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Determine swap direction based on amount signs
        let (token_in, token_in_amount, token_out, token_out_amount) = if event.amount0.is_negative() {
            (self.token0, event.amount0.unsigned_abs(), self.token1, U256::from(event.amount1))
        } else {
            (self.token1, event.amount1.unsigned_abs(), self.token0, U256::from(event.amount0))
        };

        Ok(Some(SwapData {
            token_in,
            token_in_amount,
            token_out,
            token_out_amount,
            sender: Some(event.sender),
        }))
    }
}
```

See the [PriceSource trait documentation](https://docs.rs/semioscan/latest/semioscan/price/trait.PriceSource.html) for more details and best practices.

## Library Architecture

Semioscan is a **library-only crate** with no binaries, CLI tools, or API servers. You bring your own:

- **Blockchain Providers**: Use [Alloy](https://github.com/alloy-rs/alloy) to create providers for your chains
- **Price Sources**: Implement the `PriceSource` trait for your DEX protocol
- **Configuration**: Configure RPC endpoints and chain settings in your application

This design makes semioscan highly composable and easy to integrate into existing systems.

## Examples

The `examples/` directory contains complete working examples:

- **`daily_block_window.rs`**: Calculate block windows for specific dates
- **Shell scripts**: Multi-chain reporting and data collection workflows

Run an example:

```bash
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
cargo run --package semioscan --example daily_block_window
```

## Multi-Chain Support

Semioscan works with any EVM-compatible chain. Chains with L2-specific features (like L1 data fees) are automatically detected and handled correctly.

Tested chains include:

- **L1**: Ethereum, Avalanche, BNB Chain
- **L2**: Arbitrum, Base, Optimism, Polygon, Scroll, Mode, Sonic, Fraxtal

Chain support is based on [alloy-chains](https://github.com/alloy-rs/alloy/tree/main/crates/alloy-chains) `NamedChain` enum.

## Advanced Configuration

Use `SemioscanConfig` to customize RPC behavior per chain:

```rust
use semioscan::SemioscanConfigBuilder;
use alloy_chains::NamedChain;

let config = SemioscanConfigBuilder::default()
    .with_chain_override(
        NamedChain::Base,
        2000,  // max_block_range
        500    // rate_limit_per_second
    )
    .build()?;

// Pass config to calculators
let calculator = GasCalculator::with_config(provider.clone(), Some(config.clone()));
```

## Development

### Building

```bash
# Build library
cargo build --package semioscan

# Build with Odos example
cargo build --package semioscan --features odos-example

# Run tests
cargo test --package semioscan --lib

# Check code quality
cargo clippy --package semioscan --all-targets --all-features -- -D warnings
```

### Testing

Semioscan has comprehensive unit tests for all business logic:

```bash
# Run all tests
cargo test --package semioscan --all-features

# Run only unit tests (no integration tests)
cargo test --package semioscan --lib

# Run specific test file
cargo test --package semioscan --test gas_calculator_tests
```

**Testing Strategy**:

- **Unit Tests** (`tests/`): Test business logic, edge cases, and error handling without external dependencies
- **Examples** (`examples/`): Validate integration with real blockchain networks (requires RPC access)
- **Mock Infrastructure**: `tests/helpers/` provides `MockPriceSource` for testing price extraction logic

See [TESTING_GUIDE.md](docs/TESTING_GUIDE.md) for detailed testing principles and best practices.

## Production Usage

Semioscan is battle-tested in production for:

- **Automated liquidation systems** processing millions of dollars in swaps across 12+ chains
- **Financial reporting** for blockchain transaction accounting
- **Token analytics** for discovering and tracking token transfers

## Contributing

Contributions are welcome! Areas of interest:

- Additional DEX protocol implementations (Uniswap, SushiSwap, Curve, etc.)
- Performance optimizations for large block ranges
- Additional caching strategies
- Documentation improvements

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Acknowledgments

Built by [Semiotic AI](https://semiotic.ai) as part of the Likwid liquidation infrastructure. Extracted and open-sourced to benefit the Rust + Ethereum ecosystem.
