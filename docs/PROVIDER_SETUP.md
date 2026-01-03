# Provider Setup Examples

This guide provides practical examples for setting up Alloy providers with Semioscan, covering common configurations and advanced patterns.

## Table of Contents

- [Basic Provider Setup](#basic-provider-setup)
- [Rate Limiting](#rate-limiting)
- [Logging and Tracing](#logging-and-tracing)
- [Retry with Exponential Backoff](#retry-with-exponential-backoff)
- [Combining Multiple Layers](#combining-multiple-layers)
- [Provider Pooling](#provider-pooling)
- [WebSocket Providers](#websocket-providers)
- [Custom Filler Configuration](#custom-filler-configuration)

---

## Basic Provider Setup

### Simple HTTP Provider

The simplest way to create a provider:

```rust
use alloy_provider::ProviderBuilder;

let provider = ProviderBuilder::new()
    .connect_http("https://eth.llamarpc.com".parse()?);

let block = provider.get_block_number().await?;
```

### Using Semioscan Helpers

Semioscan provides convenience functions:

```rust
use semioscan::{simple_http_provider, ProviderConfig, create_http_provider};

// Quick and simple
let provider = simple_http_provider("https://eth.llamarpc.com")?;

// With configuration
let provider = create_http_provider(
    ProviderConfig::new("https://eth.llamarpc.com")
)?;
```

### Network-Specific Providers

For type-safe network-specific operations:

```rust
use alloy_network::Ethereum;
use op_alloy_network::Optimism;
use alloy_provider::ProviderBuilder;
use semioscan::create_typed_http_provider;

// Ethereum provider (explicit)
let eth_provider = ProviderBuilder::new()
    .network::<Ethereum>()
    .connect_http("https://eth.llamarpc.com".parse()?);

// Optimism provider (for Base, OP, Mode, etc.)
let op_provider = ProviderBuilder::new()
    .network::<Optimism>()
    .connect_http("https://mainnet.base.org".parse()?);

// Using Semioscan's typed helper
use semioscan::ProviderConfig;
let config = ProviderConfig::new("https://eth.llamarpc.com");
let provider = create_typed_http_provider::<Ethereum>(config)?;
```

---

## Rate Limiting

Rate limiting prevents RPC providers from rejecting requests due to rate limits.

### Using RateLimitLayer

```rust
use semioscan::RateLimitLayer;
use alloy_rpc_client::ClientBuilder;
use alloy_provider::ProviderBuilder;
use std::time::Duration;

// 10 requests per second
let layer = RateLimitLayer::per_second(10);

// Apply to client
let client = ClientBuilder::default()
    .layer(layer)
    .http("https://eth.llamarpc.com".parse()?);

let provider = ProviderBuilder::new()
    .connect_client(client);
```

### Custom Rate Limit Configurations

```rust
use semioscan::RateLimitLayer;
use std::time::Duration;

// 100 requests per minute
let layer = RateLimitLayer::new(100, Duration::from_secs(60));

// Minimum 100ms between requests
let layer = RateLimitLayer::with_min_delay(Duration::from_millis(100));

// Chain-specific rate limits
fn rate_limit_for_chain(chain: NamedChain) -> RateLimitLayer {
    match chain {
        NamedChain::Base => RateLimitLayer::per_second(4),      // Strict
        NamedChain::Optimism => RateLimitLayer::per_second(10),
        NamedChain::Mainnet => RateLimitLayer::per_second(25),
        _ => RateLimitLayer::per_second(10),                    // Default
    }
}
```

### Using ProviderConfig

```rust
use semioscan::{create_http_provider, ProviderConfig};

// Rate limiting via config
let provider = create_http_provider(
    ProviderConfig::new("https://eth.llamarpc.com")
        .with_rate_limit(10)  // 10 req/sec
)?;
```

---

## Logging and Tracing

Add observability to RPC calls for debugging and monitoring.

### Using LoggingLayer

```rust
use semioscan::LoggingLayer;
use alloy_rpc_client::ClientBuilder;
use alloy_provider::ProviderBuilder;

// Basic logging (method names and timing)
let layer = LoggingLayer::new();

// With request payload logging
let layer = LoggingLayer::new().with_request_logging();

// Verbose mode (full request/response payloads)
let layer = LoggingLayer::new().verbose();

let client = ClientBuilder::default()
    .layer(layer)
    .http("https://eth.llamarpc.com".parse()?);

let provider = ProviderBuilder::new()
    .connect_client(client);

// Now all RPC calls are logged:
// DEBUG rpc_call{method="eth_blockNumber"}: RPC request
// DEBUG rpc_call{method="eth_blockNumber" duration_ms=42}: RPC response
```

### Using ProviderConfig

```rust
use semioscan::{create_http_provider, ProviderConfig};

let provider = create_http_provider(
    ProviderConfig::new("https://eth.llamarpc.com")
        .with_logging(true)
)?;
```

---

## Retry with Exponential Backoff

Handle transient failures gracefully with automatic retries.

### Using RetryLayer

```rust
use semioscan::{RetryLayer, RetryConfig};
use alloy_rpc_client::ClientBuilder;
use alloy_provider::ProviderBuilder;
use std::time::Duration;

// Default retry (3 attempts, 100ms base delay)
let layer = RetryLayer::new();

// Custom configuration
let layer = RetryLayer::builder()
    .max_retries(5)
    .base_delay(Duration::from_millis(200))
    .max_delay(Duration::from_secs(60))
    .build();

// Preset configurations
let aggressive = RetryLayer::aggressive();   // 5 retries, 50ms base
let conservative = RetryLayer::conservative(); // 3 retries, 500ms base

let client = ClientBuilder::default()
    .layer(layer)
    .http("https://eth.llamarpc.com".parse()?);

let provider = ProviderBuilder::new()
    .connect_client(client);
```

### Retry Behavior

The retry layer uses exponential backoff:

- Delay = min(base_delay * 2^attempt, max_delay)
- Only retries transient errors (rate limits, timeouts, connection errors)
- Does not retry permanent errors (invalid params, contract reverts)

---

## Combining Multiple Layers

Layer order matters! Outer layers wrap inner layers.

### Recommended Stack

```rust
use semioscan::{LoggingLayer, RateLimitLayer, RetryLayer};
use alloy_rpc_client::ClientBuilder;
use alloy_provider::ProviderBuilder;

// Recommended layer order (outer to inner):
// 1. Logging - logs all requests/responses
// 2. Retry - retries failed requests
// 3. Rate Limit - enforces rate limits
let client = ClientBuilder::default()
    .layer(LoggingLayer::new())           // Outer: logs everything
    .layer(RetryLayer::new())             // Middle: retries on failure
    .layer(RateLimitLayer::per_second(10)) // Inner: rate limits
    .http("https://eth.llamarpc.com".parse()?);

let provider = ProviderBuilder::new()
    .connect_client(client);
```

### Layer Execution Order

With the stack above, request flow is:

```
Request → Logging → Retry → RateLimit → HTTP Transport
                                              ↓
Response ← Logging ← Retry ← RateLimit ← HTTP Transport
```

- Logging sees all requests (including retries)
- Retry catches failures and retries with backoff
- Rate limit ensures we don't exceed limits

---

## Provider Pooling

For multi-chain applications, use a provider pool for efficient connection reuse.

### Basic Pool Setup

```rust
use semioscan::{ProviderPool, ProviderPoolBuilder, ChainEndpoint};
use alloy_chains::NamedChain;

// Create a pool with multiple chains
let pool = ProviderPoolBuilder::new()
    .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
    .add_chain(NamedChain::Base, "https://mainnet.base.org")
    .add_chain(NamedChain::Optimism, "https://mainnet.optimism.io")
    .add_chain(NamedChain::Arbitrum, "https://arb1.arbitrum.io/rpc")
    .with_rate_limit(10)
    .build()?;

// Get provider for a specific chain
let provider = pool.get(NamedChain::Base).expect("Chain not configured");
let block = provider.get_block_number().await?;
```

### Static Pool Pattern

For applications that need a global pool:

```rust
use semioscan::{ProviderPool, ProviderPoolBuilder};
use alloy_chains::NamedChain;
use std::sync::LazyLock;

// Static pool initialized once on first access
static PROVIDERS: LazyLock<ProviderPool> = LazyLock::new(|| {
    ProviderPoolBuilder::new()
        .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
        .add_chain(NamedChain::Base, "https://mainnet.base.org")
        .add_chain(NamedChain::Optimism, "https://mainnet.optimism.io")
        .with_rate_limit(10)
        .build()
        .expect("Failed to create provider pool")
});

// Use anywhere in your application
async fn get_block_number(chain: NamedChain) -> Result<u64, Error> {
    let provider = PROVIDERS.get(chain).expect("Chain not configured");
    Ok(provider.get_block_number().await?)
}

async fn multi_chain_operation() -> Result<(), Error> {
    // Concurrent access to multiple chains
    let (eth_block, base_block) = tokio::join!(
        get_block_number(NamedChain::Mainnet),
        get_block_number(NamedChain::Base),
    );

    println!("Ethereum: {}, Base: {}", eth_block?, base_block?);
    Ok(())
}
```

### Lazy Provider Addition

Add chains on-demand:

```rust
use semioscan::{ProviderPool, ProviderPoolBuilder, ProviderConfig};
use alloy_chains::NamedChain;

let pool = ProviderPoolBuilder::new().build()?;

// Add chains lazily when first accessed
async fn get_or_add_provider(
    pool: &ProviderPool,
    chain: NamedChain,
    rpc_url: &str,
) -> Result<&PooledProvider, Error> {
    pool.get_or_add(chain, || {
        ProviderConfig::new(rpc_url).with_rate_limit(10)
    })
}
```

---

## WebSocket Providers

For real-time subscriptions, use WebSocket providers.

### Basic WebSocket Setup

```rust
use alloy_provider::{ProviderBuilder, WsConnect};

let ws = WsConnect::new("wss://eth-mainnet.ws.alchemyapi.io/v2/YOUR_KEY");
let provider = ProviderBuilder::new()
    .connect_ws(ws)
    .await?;
```

### Using Semioscan's WebSocket Helper

```rust
use semioscan::{create_ws_provider, ProviderConfig};

let provider = create_ws_provider(
    ProviderConfig::new("wss://eth-mainnet.ws.alchemyapi.io/v2/YOUR_KEY")
).await?;
```

### Real-Time Event Streaming

```rust
use semioscan::events::realtime::RealtimeEventScanner;
use alloy_provider::{ProviderBuilder, WsConnect};
use alloy_rpc_types::Filter;
use futures::StreamExt;

// Create WebSocket provider
let ws = WsConnect::new("wss://eth.example.com/ws");
let provider = ProviderBuilder::new().connect_ws(ws).await?;

// Create scanner
let scanner = RealtimeEventScanner::new(provider);

// Subscribe to blocks
let mut blocks = scanner.subscribe_blocks().await?;
while let Some(header) = blocks.next().await {
    println!("New block: #{}", header.number);
}

// Subscribe to logs
let filter = Filter::new()
    .address(token_address)
    .event_signature(Transfer::SIGNATURE_HASH);

let mut logs = scanner.subscribe_logs(filter).await?;
while let Some(log) = logs.next().await {
    println!("Transfer event: {:?}", log);
}
```

---

## Custom Filler Configuration

Fillers automatically populate transaction fields.

### Recommended Fillers

```rust
use alloy_provider::ProviderBuilder;

// Default: includes nonce, gas, and chain ID fillers
let provider = ProviderBuilder::new()
    .with_recommended_fillers()
    .connect_http("https://eth.llamarpc.com".parse()?);
```

### With Wallet

```rust
use alloy_provider::ProviderBuilder;
use alloy_signer_local::PrivateKeySigner;

let signer: PrivateKeySigner = "your-private-key".parse()?;

let provider = ProviderBuilder::new()
    .with_recommended_fillers()
    .wallet(signer)
    .connect_http("https://eth.llamarpc.com".parse()?);

// Transactions are now automatically signed
let tx = TransactionRequest::default()
    .with_to(recipient)
    .with_value(U256::from(1_000_000_000_000_000_000u64));

let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
```

### Read-Only Provider (No Fillers)

For read-only operations, skip fillers for better performance:

```rust
use alloy_provider::ProviderBuilder;

// No fillers - for read-only operations
let provider = ProviderBuilder::new()
    .connect_http("https://eth.llamarpc.com".parse()?);

// Can read but not send transactions
let block = provider.get_block_number().await?;
```

---

## Complete Examples

### Production-Ready Ethereum Provider

```rust
use semioscan::{RateLimitLayer, RetryLayer, LoggingLayer};
use alloy_rpc_client::ClientBuilder;
use alloy_provider::ProviderBuilder;
use alloy_network::Ethereum;

fn create_production_provider(rpc_url: &str) -> Result<impl Provider<Ethereum>, Error> {
    let client = ClientBuilder::default()
        .layer(LoggingLayer::new())
        .layer(RetryLayer::builder()
            .max_retries(5)
            .base_delay(Duration::from_millis(100))
            .build())
        .layer(RateLimitLayer::per_second(20))
        .http(rpc_url.parse()?);

    Ok(ProviderBuilder::new()
        .network::<Ethereum>()
        .connect_client(client))
}
```

### Multi-Chain Analytics Application

```rust
use semioscan::{
    GasCostCalculator, network_type_for_chain, NetworkType,
    ProviderPool, ProviderPoolBuilder,
    EthereumReceiptAdapter, OptimismReceiptAdapter,
};
use alloy_chains::NamedChain;
use std::sync::LazyLock;

static POOL: LazyLock<ProviderPool> = LazyLock::new(|| {
    ProviderPoolBuilder::new()
        .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
        .add_chain(NamedChain::Base, "https://mainnet.base.org")
        .add_chain(NamedChain::Optimism, "https://mainnet.optimism.io")
        .with_rate_limit(10)
        .build()
        .expect("Failed to create pool")
});

async fn analyze_gas_costs(
    chain: NamedChain,
    from: Address,
    to: Address,
    start_block: u64,
    end_block: u64,
) -> Result<GasCostResult, Error> {
    let provider = POOL.get(chain).expect("Chain not configured");

    match network_type_for_chain(chain) {
        NetworkType::Ethereum => {
            // Clone to get owned provider for calculator
            let calculator = GasCostCalculator::new(provider.clone());
            calculator.calculate_gas_cost_for_transfers_between_blocks(
                chain, from, to, start_block, end_block
            ).await
        }
        NetworkType::Optimism => {
            let calculator = GasCostCalculator::new(provider.clone());
            calculator.calculate_gas_cost_for_transfers_between_blocks(
                chain, from, to, start_block, end_block
            ).await
        }
    }
}
```

### Batch RPC Operations

```rust
use semioscan::{batch_fetch_balances, batch_fetch_eth_balances, BalanceQuery};
use alloy_provider::ProviderBuilder;
use alloy_provider::layers::CallBatchLayer;
use std::time::Duration;

// Provider with automatic call batching
let provider = ProviderBuilder::new()
    .layer(CallBatchLayer::new().wait(Duration::from_millis(10)))
    .connect_http("https://eth.llamarpc.com".parse()?);

// Batch fetch multiple token balances
let queries: Vec<BalanceQuery> = vec![
    (usdc_address, alice),
    (usdc_address, bob),
    (weth_address, alice),
    (weth_address, bob),
];

let results = batch_fetch_balances(&provider, &queries).await;

for (query, result) in queries.iter().zip(results.iter()) {
    match result {
        Ok(balance) => println!("{:?}: {}", query, balance),
        Err(e) => println!("{:?}: Error - {}", query, e),
    }
}

// Batch fetch ETH balances
let addresses = vec![alice, bob, charlie];
let eth_results = batch_fetch_eth_balances(&provider, &addresses).await;
```

---

## Environment Variables

Common patterns for RPC URL configuration:

```rust
use std::env;

fn get_rpc_url(chain: NamedChain) -> String {
    match chain {
        NamedChain::Mainnet => {
            env::var("ETHEREUM_RPC_URL")
                .unwrap_or_else(|_| "https://eth.llamarpc.com".to_string())
        }
        NamedChain::Base => {
            env::var("BASE_RPC_URL")
                .unwrap_or_else(|_| "https://mainnet.base.org".to_string())
        }
        NamedChain::Optimism => {
            env::var("OPTIMISM_RPC_URL")
                .unwrap_or_else(|_| "https://mainnet.optimism.io".to_string())
        }
        _ => panic!("Unsupported chain: {:?}", chain),
    }
}
```

### Using dotenvy

```rust
use dotenvy::dotenv;

fn main() -> Result<(), Error> {
    // Load .env file (optional - doesn't fail if missing)
    dotenv().ok();

    let rpc_url = std::env::var("ETHEREUM_RPC_URL")?;
    // ...
}
```

---

*Related documentation:*

- [Network Selection Guide](./NETWORK_SELECTION.md)
- [Alloy Base Prompt](./alloy/base-prompt.md)
- [Improvements Tracking](./IMPROVEMENTS.md)
