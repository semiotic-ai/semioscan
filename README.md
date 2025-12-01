# Semioscan

[![Crates.io](https://img.shields.io/crates/v/semioscan.svg)](https://crates.io/crates/semioscan)
[![Documentation](https://docs.rs/semioscan/badge.svg)](https://docs.rs/semioscan)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

**Semioscan** is a Rust library for blockchain analytics, providing production-grade tools for calculating gas costs, extracting price data from DEX swaps, and working with block ranges across multiple EVM-compatible chains.

**Key differentiator**: Semioscan is a **library-only crate** with no CLI, API server, or database dependencies. You bring your own infrastructure and integrate semioscan into your existing systems.

Built on [Alloy](https://github.com/alloy-rs/alloy), the modern Ethereum library for Rust, semioscan provides type-safe blockchain interactions with zero-copy parsing and excellent performance.

## Table of Contents

- [Semioscan](#semioscan)
  - [Table of Contents](#table-of-contents)
  - [Features](#features)
  - [Use Cases](#use-cases)
  - [Installation](#installation)
    - [Feature Flags](#feature-flags)
  - [Quick Start](#quick-start)
    - [1. Calculate Gas Costs](#1-calculate-gas-costs)
    - [2. Calculate Daily Block Windows](#2-calculate-daily-block-windows)
    - [3. Extract DEX Price Data](#3-extract-dex-price-data)
  - [Examples and Tutorials](#examples-and-tutorials)
    - [Quick Reference](#quick-reference)
    - [Shell Script Workflows](#shell-script-workflows)
    - [Running Examples](#running-examples)
  - [Core Concepts](#core-concepts)
    - [Block Windows](#block-windows)
    - [L1 Data Fees (L2 Chains)](#l1-data-fees-l2-chains)
    - [Caching](#caching)
      - [Cache Backends](#cache-backends)
      - [Basic Usage](#basic-usage)
      - [Advanced Configuration](#advanced-configuration)
      - [Cache Statistics](#cache-statistics)
      - [Cache Best Practices](#cache-best-practices)
      - [Multi-Process Safety](#multi-process-safety)
      - [Custom Cache Backends](#custom-cache-backends)
      - [What's Cached](#whats-cached)
  - [Implementing Custom Price Sources](#implementing-custom-price-sources)
    - [Example: Uniswap V3 Price Source](#example-uniswap-v3-price-source)
  - [Library Architecture](#library-architecture)
  - [Multi-Chain Support](#multi-chain-support)
  - [Advanced Configuration](#advanced-configuration-1)
  - [Performance Considerations](#performance-considerations)
    - [Block Range Chunking](#block-range-chunking)
    - [Rate Limiting](#rate-limiting)
    - [Memory Usage](#memory-usage)
    - [Query Performance](#query-performance)
  - [Running Tests and Examples](#running-tests-and-examples)
    - [Running Tests](#running-tests)
    - [Running Examples](#running-examples-1)
  - [Troubleshooting](#troubleshooting)
    - [Common Issues](#common-issues)
  - [When NOT to Use Semioscan](#when-not-to-use-semioscan)
  - [Production Usage](#production-usage)
  - [Contributing](#contributing)
  - [License](#license)
  - [Acknowledgments](#acknowledgments)

## Features

- **Gas Cost Calculation**: Accurately calculate transaction gas costs for both L1 (Ethereum) and L2 (Optimism Stack) chains, including L1 data fees
- **Block Window Calculations**: Map UTC dates to blockchain block ranges with intelligent caching
- **DEX Price Extraction**: Extensible trait-based system for extracting price data from on-chain swap events
- **Multi-Chain Support**: Works with 12+ EVM chains including Ethereum, Arbitrum, Base, Optimism, Polygon, and more
- **Event Scanning**: Extract transfer amounts and events from blockchain transaction logs
- **Production-Ready**: Battle-tested in production for automated trading and DeFi applications processing millions of dollars in swaps

## Use Cases

Semioscan is ideal for:

- **DeFi Liquidation Bots**: Calculate profitability accounting for accurate gas costs across L1/L2 chains
- **Trading Automation**: Extract real-time price data from DEX swaps for arbitrage detection
- **Blockchain Analytics**: Map calendar dates to block ranges for historical analysis and reporting
- **Token Discovery**: Scan chains for tokens transferred to specific addresses (e.g., router contracts)
- **Financial Reporting**: Calculate transaction costs for accounting and tax purposes
- **MEV Research**: Analyze gas costs and swap prices for MEV opportunity detection
- **Multi-Chain Operations**: Consistent API across 12+ EVM chains with automatic L2 fee handling

## Installation

Add semioscan to your `Cargo.toml`:

```toml
[dependencies]
# Core library (gas, block windows, events)
semioscan = "0.3"

# With Odos DEX reference implementation (optional)
semioscan = { version = "0.3", features = ["odos-example"] }
```

### Feature Flags

- **`odos-example`**: Includes `OdosPriceSource` as a reference implementation of the `PriceSource` trait for Odos DEX aggregator (optional, not included by default)

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

    // Create calculator with disk cache
    let calculator = BlockWindowCalculator::with_disk_cache(
        provider.clone(),
        "block_windows.json"
    )?;

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

## Examples and Tutorials

The [`examples/`](examples/) directory contains complete, production-ready examples demonstrating semioscan's capabilities. **See [examples/README.md](examples/README.md) for comprehensive documentation, setup instructions, and troubleshooting.**

### Quick Reference

| Example | Use Case | Difficulty |
|---------|----------|------------|
| [`daily_block_window.rs`](examples/daily_block_window.rs) | Map UTC dates to block ranges | Beginner |
| [`router_token_discovery.rs`](examples/router_token_discovery.rs) | Discover tokens sent to router contracts | Intermediate |
| [`eip4844_blob_gas.rs`](examples/eip4844_blob_gas.rs) | Calculate EIP-4844 blob gas for L2 rollups | Advanced |
| [`custom_dex_integration.rs`](examples/custom_dex_integration.rs) | Implement `PriceSource` for any DEX | Advanced |

### Shell Script Workflows

Production-ready scripts for multi-chain operations:

- **`multi_chain_daily_report.sh`** - Generate liquidation reports across all chains
- **`generate_daily_report.sh`** - Single-chain financial reporting
- **`collect_data_only.sh`** - Data collection without report generation
- **`generate_report_from_json.sh`** - Report from cached data

### Running Examples

```bash
# Basic usage
RPC_URL=https://arb1.arbitrum.io/rpc cargo run --example daily_block_window

# With chain-specific environment variables
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc cargo run --example router_token_discovery -- arbitrum

# With logging for debugging
RUST_LOG=debug cargo run --example eip4844_blob_gas
```

**For detailed setup, configuration, performance tips, and troubleshooting, see [examples/README.md](examples/README.md).**

## Core Concepts

### Block Windows

A block window maps a calendar date (in UTC) to the range of blocks produced during that day. Different chains have different block production rates:

- **Arbitrum**: ~4 blocks/second (~345,600 blocks/day)
- **Ethereum**: ~12 seconds/block (~7,200 blocks/day)
- **Base**: ~2 seconds/block (~43,200 blocks/day)

Block windows enable date-based queries for analytics, reporting, and historical analysis.

### L1 Data Fees (L2 Chains)

L2 chains like Arbitrum, Base, and Optimism post transaction data to Ethereum for security. This creates two separate gas costs:

- **Execution gas**: Cost of running the transaction on L2 (cheap, uses L2 gas price)
- **L1 data fee**: Cost of posting transaction data to Ethereum (expensive, varies by calldata size and L1 gas price)

Semioscan automatically detects L2 chains and calculates both components for accurate total costs. This is critical for profitability calculations in liquidation bots and trading systems.

### Caching

Semioscan provides flexible caching for block window calculations using a trait-based backend system. You can choose the caching strategy that best fits your needs.

#### Cache Backends

**DiskCache** (recommended for production)

- Persistent JSON-based cache with file locking
- Survives process restarts
- Multi-process safe (advisory file locks)
- Configurable TTL and size limits
- Automatic path validation
- ~1-2ms cache hit latency

**MemoryCache**

- In-memory HashMap cache
- Fastest performance (<0.1ms cache hits)
- Data lost when process exits
- Configurable size limits with LRU eviction
- Ideal for short-lived processes

**NoOpCache**

- Disables caching entirely
- Zero overhead
- Always performs RPC queries
- Useful for testing or one-time queries

#### Basic Usage

```rust
use semioscan::{BlockWindowCalculator, DiskCache, MemoryCache};
use std::time::Duration;

// Disk cache (simplest, recommended)
let calculator = BlockWindowCalculator::with_disk_cache(provider, "cache.json")?;

// Memory cache
let calculator = BlockWindowCalculator::with_memory_cache(provider);

// No cache
let calculator = BlockWindowCalculator::without_cache(provider);
```

#### Advanced Configuration

```rust
use semioscan::{BlockWindowCalculator, DiskCache};
use std::time::Duration;

// Disk cache with TTL and size limit
let cache = DiskCache::new("cache.json")
    .with_ttl(Duration::from_secs(86400 * 7))  // 7 days
    .with_max_entries(1000)                     // Max 1000 entries
    .validate()?;                               // Validate path

let calculator = BlockWindowCalculator::new(provider, Box::new(cache));

// Memory cache with size limit
let cache = MemoryCache::new()
    .with_max_entries(500)
    .with_ttl(Duration::from_secs(3600));

let calculator = BlockWindowCalculator::new(provider, Box::new(cache));
```

#### Cache Statistics

All cache backends track performance metrics:

```rust
let stats = calculator.cache_stats().await;
println!("Hit rate: {:.1}%", stats.hit_rate());
println!("Hits: {}, Misses: {}", stats.hits, stats.misses);
println!("Evictions: {}, Entries: {}", stats.evictions, stats.entries);
```

#### Cache Best Practices

1. **Production**: Use `DiskCache` with TTL for persistent caching
2. **Development**: Use `MemoryCache` for faster iteration without disk I/O
3. **Testing**: Use `NoOpCache` or `MemoryCache` to avoid file system dependencies
4. **Path validation**: Always call `.validate()` on `DiskCache` to catch path issues early
5. **TTL**: Set TTL based on your use case (block windows are immutable for past dates)
6. **Size limits**: Set reasonable limits to prevent unbounded cache growth

#### Multi-Process Safety

`DiskCache` uses advisory file locking to prevent corruption when multiple processes share the same cache file. However, for high-concurrency scenarios, consider:

- Using separate cache files per process
- Using a centralized cache service (Redis, etc.) via custom `BlockWindowCache` trait implementation

#### Custom Cache Backends

Implement the `BlockWindowCache` trait to create custom cache backends (Redis, S3, etc.):

```rust
use semioscan::cache::{BlockWindowCache, CacheKey, CacheStats};
use semioscan::DailyBlockWindow;
use async_trait::async_trait;

struct RedisCacheBackend {
    client: redis::Client,
}

#[async_trait]
impl BlockWindowCache for RedisCacheBackend {
    async fn get(&self, key: &CacheKey) -> Option<DailyBlockWindow> {
        // Implement Redis get logic
        todo!()
    }

    async fn insert(&self, key: CacheKey, window: DailyBlockWindow)
        -> Result<(), BlockWindowError>
    {
        // Implement Redis insert logic
        todo!()
    }

    async fn clear(&self) -> Result<(), BlockWindowError> {
        todo!()
    }

    async fn stats(&self) -> CacheStats {
        todo!()
    }

    fn name(&self) -> &'static str {
        "RedisCacheBackend"
    }
}
```

#### What's Cached

- **Block windows**: Mappings from (chain, date) to block ranges
  - Immutable for past dates (perfect for caching)
  - ~200 bytes per cached entry
  - Dramatically reduces RPC usage (5-15s query → <1ms)
- **Gas calculations**: In-memory cache only (not persisted)
- **Price calculations**: In-memory cache only (not persisted)

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

## Performance Considerations

### Block Range Chunking

Large block ranges are automatically chunked to prevent RPC timeouts:

- **Default**: 5,000 blocks per chunk (configurable per chain)
- **Benefits**: Prevents timeouts, enables progress tracking, reduces memory usage

### Rate Limiting

Automatic rate limiting protects against RPC provider limits:

- **Default**: 100 requests/second (configurable per chain)
- **Recommendation**: Use paid RPC providers for production (300-1000+ req/s)

### Memory Usage

- **Minimal**: Caches are written to disk, not held in memory
- **Typical cache size**: 1-10 MB per chain
- **Concurrency**: Safe to run multiple queries concurrently

### Query Performance

Typical performance characteristics (depends on RPC provider):

- **Block window calculation**: 5-15 seconds (first query), <1ms (cached)
- **Gas calculation** (1,000 blocks): 10-30 seconds
- **Token discovery** (10,000 blocks): 2-5 minutes

**See [examples/README.md#performance-tips](examples/README.md#performance-tips) for optimization strategies.**

## Running Tests and Examples

### Running Tests

Semioscan has comprehensive unit tests for all business logic:

```bash
# Run all tests
cargo test --package semioscan --all-features

# Run only unit tests (no integration tests)
cargo test --package semioscan --lib

# Run specific test file
cargo test --package semioscan --test gas_calculator_tests

# Run with logging
RUST_LOG=debug cargo test --package semioscan
```

### Running Examples

Examples demonstrate real-world usage with live blockchain connections:

```bash
# Run example with environment variables
RPC_URL=https://arb1.arbitrum.io/rpc cargo run --package semioscan --example daily_block_window

# Run with logging
RUST_LOG=info RPC_URL=https://arb1.arbitrum.io/rpc cargo run --package semioscan --example router_token_discovery

# Run with chain-specific configuration
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc \
API_KEY=your_api_key \
cargo run --package semioscan --example router_token_discovery -- arbitrum
```

**For detailed example documentation, see [examples/README.md](examples/README.md).**

## Troubleshooting

### Common Issues

**Rate Limiting (`429 Too Many Requests`)**

- **Solution**: Use a paid RPC provider or increase rate limit delay in config
- **See**: [examples/README.md#rpc-errors](examples/README.md#rpc-errors)

**Block Range Too Large**

- **Solution**: Reduce `max_block_range` in config (default: 5,000)
- **Cause**: Some RPC providers have stricter limits

**Missing Data / No Logs Found**

- **Possible causes**: Wrong contract address, invalid block range, chain reorganization
- **Solution**: Verify addresses and block range using a block explorer

**Chain ID Issues**

- **Solution**: Set `CHAIN_ID` environment variable for chains without `eth_chainId` support
- **Affected chains**: Some Avalanche RPC endpoints

**For comprehensive troubleshooting, see [examples/README.md#troubleshooting](examples/README.md#troubleshooting).**

## When NOT to Use Semioscan

Semioscan may not be the best choice for:

- **Real-time price feeds**: Use WebSocket-based oracles (Chainlink, Pyth, etc.) for sub-second price updates
- **Non-EVM chains**: Semioscan is EVM-specific (Solana, Cosmos, etc. are not supported)
- **Simple balance queries**: Use lighter libraries like `ethers-rs` for basic token balances
- **Indexing entire chains**: Use The Graph or custom indexers for comprehensive blockchain indexing
- **High-frequency trading**: RPC-based queries have latency; use WebSocket streams or MEV infrastructure

Semioscan excels at **batch analytics**, **historical queries**, and **multi-chain operations** where accurate gas cost calculation and flexible price extraction are required.

## Production Usage

Semioscan is battle-tested in production for:

- **Automated trading and DeFi applications** processing millions of dollars in swaps across 12+ chains
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
