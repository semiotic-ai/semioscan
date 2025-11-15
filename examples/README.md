# Semioscan Examples

This directory contains examples demonstrating the key features of the semioscan library. Examples are organized by functionality and use case.

## Table of Contents

- [Semioscan Examples](#semioscan-examples)
  - [Table of Contents](#table-of-contents)
  - [Rust Examples](#rust-examples)
    - [Block Window Calculations](#block-window-calculations)
    - [Token Discovery](#token-discovery)
    - [Gas Calculations](#gas-calculations)
    - [Custom DEX Integration](#custom-dex-integration)
  - [Configuration](#configuration)
  - [Prerequisites](#prerequisites)
    - [Environment Setup](#environment-setup)
    - [RPC Endpoints](#rpc-endpoints)
  - [Running Examples](#running-examples)
    - [Basic Pattern](#basic-pattern)
    - [With Environment Variables](#with-environment-variables)
    - [With Logging](#with-logging)
    - [Common Environment Variables](#common-environment-variables)
  - [Performance Tips](#performance-tips)
    - [Rate Limiting](#rate-limiting)
    - [Block Range Chunking](#block-range-chunking)
    - [Caching](#caching)
  - [Troubleshooting](#troubleshooting)
    - [RPC Errors](#rpc-errors)
    - [Missing Data](#missing-data)
    - [Chain ID Issues](#chain-id-issues)
  - [Example Workflow](#example-workflow)
  - [Further Reading](#further-reading)
  - [Support](#support)

## Rust Examples

### Block Window Calculations

**[`daily_block_window.rs`](./daily_block_window.rs)**

Demonstrates how to calculate block ranges for specific UTC days, essential for querying blockchain data by date.

**Features:**

- Create `BlockWindowCalculator` for any chain
- Calculate block range for a specific UTC day
- Cache results for performance
- Integrate with other semioscan tools

**Use Cases:**

- Daily analytics reports
- Historical data queries
- Time-based event analysis

**Run:**

```bash
CHAIN_ID=42161 \
RPC_URL=https://arb1.arbitrum.io/rpc/ \
API_KEY=your_api_key \
DAY=2025-10-10 \
CACHE_PATH=block_windows.json \
cargo run --package semioscan --example daily_block_window
```

**Key Concepts:**

- Block windows map calendar dates to blockchain block ranges
- Different chains have different block production rates
- Caching saves expensive RPC calls

---

### Token Discovery

**[`router_token_discovery.rs`](./router_token_discovery.rs)**

Discovers all tokens transferred to a router contract, useful for liquidation systems and token inventory management.

**Features:**

- Scan blockchain for ERC-20 Transfer events
- Discover tokens sent to specific addresses
- Handle rate limiting and chunking automatically
- Compare token sets across time periods

**Use Cases:**

- **Liquidation Systems**: Discover tokens that need to be liquidated from router contracts
- **Token Inventory**: Track all tokens sent to a contract
- **Analytics**: Analyze token flow patterns
- **Monitoring**: Alert when new tokens appear

**Run:**

```bash
# Arbitrum Odos router
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/ \
cargo run --package semioscan --example router_token_discovery -- arbitrum

# Base Odos router
BASE_RPC_URL=https://mainnet.base.org \
cargo run --package semioscan --example router_token_discovery -- base

# Custom block range
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/ \
START_BLOCK=270000000 \
END_BLOCK=270010000 \
cargo run --package semioscan --example router_token_discovery -- arbitrum --custom-range
```

**Key Concepts:**

- Transfer events reveal token movements
- Deduplication provides unique token set
- Large block ranges require chunking and rate limiting

---

### Gas Calculations

**[`eip4844_blob_gas.rs`](./eip4844_blob_gas.rs)**

Demonstrates EIP-4844 blob gas calculations for L2 rollup transactions on Ethereum.

**Features:**

- Detect EIP-4844 (Type 3) transactions
- Calculate blob gas separately from execution gas
- Understand cost breakdown: execution + blob gas
- Analyze real-world blob transactions

**Background:**

- **Before EIP-4844**: L2 rollups posted data as expensive calldata
- **After EIP-4844**: L2s use cheaper "blobs" for data availability
- **Cost Savings**: 10-100x cheaper for L2 data posting

**Use Cases:**

- L2 rollup cost analysis
- Understanding L2 economics
- Transaction cost optimization
- Blob market analysis

**Run:**

```bash
ETHEREUM_RPC_URL=https://eth.llamarpc.com \
cargo run --package semioscan --example eip4844_blob_gas
```

**Key Concepts:**

- Type 3 transactions carry data blobs
- Blob gas uses separate fee market
- Total cost = execution gas + blob gas
- Primarily used by L2 sequencers

---

### Custom DEX Integration

**[`custom_dex_integration.rs`](./custom_dex_integration.rs)**

Template showing how to implement the `PriceSource` trait for any DEX protocol.

**Features:**

- Implement `PriceSource` for custom DEX
- Decode DEX-specific swap events
- Integrate with `PriceCalculator`
- Custom filtering logic

**Integration Steps:**

1. Define your swap events using `alloy_sol_types::sol!`
2. Implement `PriceSource` trait:
   - `router_address()`: Return DEX contract address
   - `event_topics()`: Return event signature hashes
   - `extract_swap_from_log()`: Parse events into `SwapData`
3. Optional: Implement `should_include_swap()` for filtering
4. Use with `PriceCalculator` to extract prices

**Example Implementations:**

- Uniswap V3 integration (included in template)
- Adaptable to Curve, Balancer, SushiSwap, etc.

**Run:**

```bash
cargo run --package semioscan --example custom_dex_integration
```

**Key Concepts:**

- `PriceSource` trait provides abstraction over DEXes
- Event decoding uses alloy's type-safe sol! macro
- Filtering enables liquidator-specific analytics

---

## Configuration

**[`chains_config.json`](./chains_config.json)**

Configuration file for multi-chain operations. Defines:

- RPC endpoints per chain
- Rate limiting parameters
- Max block ranges
- Chain-specific settings

**Example:**

```json
{
  "chains": {
    "arbitrum": {
      "rpc_url": "https://arb1.arbitrum.io/rpc/",
      "max_block_range": 5000,
      "rate_limit_ms": 100
    }
  }
}
```

---

## Prerequisites

### Environment Setup

1. **Install Rust** (latest stable):

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Set up environment variables**:

   ```bash
   # Create .env file in project root
   echo "ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/" >> .env
   echo "BASE_RPC_URL=https://mainnet.base.org" >> .env
   echo "ETHEREUM_RPC_URL=https://eth.llamarpc.com" >> .env
   ```

### RPC Endpoints

Examples require RPC access to blockchain nodes. Options:

**Free Public RPCs** (rate limited):

- Arbitrum: `https://arb1.arbitrum.io/rpc/`
- Base: `https://mainnet.base.org`
- Ethereum: `https://eth.llamarpc.com`

**Paid RPC Providers** (recommended for production):

- Alchemy: <https://www.alchemy.com/>
- Infura: <https://www.infura.io/>
- QuickNode: <https://www.quicknode.com/>

**Note**: Free RPCs have strict rate limits. For large block ranges, use paid providers.

---

## Running Examples

### Basic Pattern

```bash
cargo run --package semioscan --example <example_name>
```

### With Environment Variables

```bash
RPC_URL=<your_rpc_url> \
cargo run --package semioscan --example <example_name>
```

### With Logging

```bash
RUST_LOG=debug \
RPC_URL=<your_rpc_url> \
cargo run --package semioscan --example <example_name>
```

### Common Environment Variables

| Variable | Description | Required | Example |
|----------|-------------|----------|---------|
| `RPC_URL` | Blockchain RPC endpoint | Yes* | `https://arb1.arbitrum.io/rpc/` |
| `API_KEY` | RPC provider API key | No | `your_alchemy_api_key` |
| `CHAIN_ID` | Chain ID (when RPC doesn't support `eth_chainId`) | Sometimes** | `42161` |
| `RUST_LOG` | Logging level | No | `info`, `debug`, `trace` |

\* Some examples have defaults
** Required for chains without `eth_chainId` support (e.g., Avalanche)

---

## Performance Tips

### Rate Limiting

Most public RPCs have rate limits:

- Free tier: 25-100 requests/second
- Paid tier: 300-1000+ requests/second

**Semioscan automatically handles rate limiting** based on chain configuration.

### Block Range Chunking

Large block ranges are automatically chunked:

- Default: 5,000 blocks per chunk (configurable)
- Prevents RPC timeouts
- Enables progress tracking

### Caching

Examples demonstrate caching strategies:

- **Block windows**: Cache date→block mappings
- **Gas calculations**: Cache by block range
- **Token discovery**: Save results to JSON

**Use caching** to avoid re-fetching expensive blockchain data.

---

## Troubleshooting

### RPC Errors

**Error**: `429 Too Many Requests`

- **Cause**: Rate limit exceeded
- **Solution**: Use paid RPC provider or increase rate limit delay

**Error**: `block range too large`

- **Cause**: RPC doesn't support large ranges
- **Solution**: Reduce `max_block_range` in config (default: 5000)

### Missing Data

**Error**: `no logs found`

- **Possible causes**:
  - Wrong block range
  - Wrong contract address
  - Chain reorganization
- **Solution**: Verify addresses and block range

### Chain ID Issues

**Error**: `chain_id not supported`

- **Cause**: Some chains don't support `eth_chainId`
- **Solution**: Set `CHAIN_ID` environment variable

---

## Example Workflow

Complete workflow for analyzing liquidations on Arbitrum:

```bash
# 1. Calculate block window for specific day
CHAIN_ID=42161 \
RPC_URL=https://arb1.arbitrum.io/rpc/ \
DAY=2025-10-15 \
CACHE_PATH=block_windows.json \
cargo run --package semioscan --example daily_block_window

# 2. Discover tokens in router
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc/ \
START_BLOCK=<from_step_1> \
END_BLOCK=<from_step_1> \
cargo run --package semioscan --example router_token_discovery -- arbitrum --custom-range

# 3. Generate multi-chain report
./examples/multi_chain_daily_report.sh
```

---

## Further Reading

- **Main README**: `../../README.md` - Library overview and API documentation
- **Testing Guide**: `../../TESTING.md` - Testing patterns and examples
- **Contributing**: `../../CONTRIBUTING.md` - How to contribute

---

## Support

For questions or issues:

- Open an issue on GitHub
- Check existing examples for patterns
- Review source code documentation (all examples have inline docs)

---

**Note**: These examples use blockchain-dependent code and require live RPC connections. They demonstrate production patterns from the likwid liquidation system.
