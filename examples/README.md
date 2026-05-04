# Semioscan Examples

This directory contains examples demonstrating the key features of the semioscan library. Examples are organized by functionality and use case.

## Table of Contents

- [Semioscan Examples](#semioscan-examples)
  - [Table of Contents](#table-of-contents)
  - [Rust Examples](#rust-examples)
    - [Block Window Calculations](#block-window-calculations)
    - [Gas Calculations](#gas-calculations)
    - [Diagnostics](#diagnostics)
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

### Diagnostics

**[`zksync_combined_probe.rs`](./zksync_combined_probe.rs)**

Diagnostic probe for zkSync combined retrieval incidents. It compares the
typed `eth_getTransactionByHash` behavior from semioscan's
`Ethereum`-typed provider path with a permissive raw transaction decode and
the full combined retrieval path over the same transfer window.

**Features:**

- Repeated historical tx and receipt lookups for one target tx hash
- Repeated raw `eth_getTransactionByHash` lookups into `AnyRpcTransaction`
- semioscan-backed chunked transfer scan for the matching ERC-20 window
- Full `CombinedCalculator` execution with surfaced partial metadata
- Optional alternate-provider comparison via `ZKSYNC_PROBE_ALT_RPC_URL`

**Use Cases:**

- Investigating provider-specific zkSync transaction-shape mismatches
- Confirming whether typed tx lookups fail while raw permissive decoding succeeds
- Verifying that semioscan's combined fallback matches scanned transfer totals
- Comparing two zkSync RPC providers against the same historical tx

**Run:**

```bash
ZKSYNC_RPC_URL=https://your-zksync-rpc \
cargo run --package semioscan --example zksync_combined_probe
```

**Key Environment Variables:**

- `ZKSYNC_PROBE_TX_HASH` (defaults to the March 11, 2026 incident tx)
- `ZKSYNC_PROBE_START_BLOCK`
- `ZKSYNC_PROBE_END_BLOCK`
- `ZKSYNC_PROBE_FROM_ADDRESS`
- `ZKSYNC_PROBE_TO_ADDRESS`
- `ZKSYNC_PROBE_TOKEN`
- `ZKSYNC_PROBE_ATTEMPTS` (default: `3`)
- `ZKSYNC_PROBE_DELAY_MS` (default: `250`)
- `ZKSYNC_PROBE_ALT_RPC_URL` for side-by-side provider comparison

---

### Custom DEX Integration

**[`custom_dex_integration.rs`](./custom_dex_integration.rs)**

Template/tutorial showing how to implement the `PriceSource` trait for any DEX protocol.
It compiles as an example, but it is not a turnkey production integration.

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

Reference example data for multi-chain workflows and local experimentation.
Semioscan does not automatically load this file. Defines:

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
| `RPC_URL` | Blockchain RPC endpoint base URL | Yes* | `https://arb1.arbitrum.io/rpc/` |
| `API_KEY` | RPC provider API key (concatenated with RPC_URL) | No | `your_alchemy_api_key` |
| `CHAIN_ID` | Chain ID (when RPC doesn't support `eth_chainId`) | Sometimes** | `42161` |
| `RUST_LOG` | Logging level | No | `info`, `debug`, `trace` |

\* Some examples have defaults
** Required for chains without `eth_chainId` support (e.g., Avalanche)

**RPC URL Construction:**

When both `RPC_URL` and `API_KEY` are provided, they are concatenated using the web3-standard pattern:

```rust
// Examples concatenate base URL + API key + trailing slash
let full_url = format!("{RPC_URL}{API_KEY}/");
// e.g., "https://arb-mainnet.g.alchemy.com/v2/" + "your_key" + "/"
```

**Provider-Specific Examples:**

```bash
# Alchemy (recommended pattern)
RPC_URL=https://arb-mainnet.g.alchemy.com/v2/
API_KEY=your_alchemy_api_key

# Infura
RPC_URL=https://arbitrum-mainnet.infura.io/v3/
API_KEY=your_infura_project_id

# QuickNode
RPC_URL=https://xxx-xxx-xxx.arbitrum-mainnet.quiknode.pro/
API_KEY=your_quicknode_token

# Public RPC (no API key)
RPC_URL=https://arb1.arbitrum.io/rpc/
# API_KEY not needed
```

This pattern matches standard web3 tooling (Hardhat, Foundry) and provider documentation.

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

Complete workflow for date-driven blockchain analytics on Arbitrum:

```bash
# 1. Calculate block window for specific day
CHAIN_ID=42161 \
RPC_URL=https://arb1.arbitrum.io/rpc/ \
DAY=2025-10-15 \
CACHE_PATH=block_windows.json \
cargo run --package semioscan --example daily_block_window

# 2. Feed the resulting block range into your own application
#    (gas calculation, custom PriceSource scans, etc.)
```

---

## Further Reading

- **Main README**: `../../README.md` - Library overview and API documentation
- **Contributing**: `../../CONTRIBUTING.md` - How to contribute and testing guidelines
- **PriceSource Guide**: `../docs/PRICESOURCE_GUIDE.md` - Detailed guide for implementing DEX integrations

---

## Support

For questions or issues:

- Open an issue on GitHub
- Check existing examples for patterns
- Review source code documentation (all examples have inline docs)

---

**Note**: These examples use blockchain-dependent code and require live RPC connections. They demonstrate production patterns from the likwid liquidation system.
