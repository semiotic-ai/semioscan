# Network Selection Guide

This guide explains how to choose the right network type when working with Semioscan and Alloy.

## Overview

Alloy uses the `Network` trait to abstract over different blockchain networks. This enables type-safe handling of network-specific features like transaction types and receipt formats.

Semioscan supports three network approaches:

| Network Type | Use Case | L1 Data Fees | Type Safety |
|-------------|----------|--------------|-------------|
| `Ethereum` | Ethereum L1 and compatible chains | No | Full |
| `Optimism` | OP-stack L2 chains | Yes | Full |
| `AnyNetwork` | Runtime chain selection | Varies | Partial |

---

## Ethereum Network

Use `alloy_network::Ethereum` for Ethereum mainnet and chains that share its transaction/receipt format.

### When to Use

- Ethereum mainnet and testnets (Sepolia, Holesky)
- Arbitrum (despite being L2, uses Ethereum receipt format)
- Polygon (L1 sidechain)
- Avalanche C-Chain
- BNB Chain

### Key Characteristics

- Standard EIP-1559 transaction support
- No L1 data fee field in receipts
- Maximum type safety with compile-time verification

### Example

```rust
use alloy_network::Ethereum;
use alloy_provider::ProviderBuilder;
use semioscan::{GasCostCalculator, EthereumReceiptAdapter};

// Create provider with Ethereum network
let provider = ProviderBuilder::new()
    .connect_http("https://eth.llamarpc.com".parse()?);

// Use with GasCostCalculator - adapter extracts gas data correctly
let calculator = GasCostCalculator::new(provider.root().clone());

// The EthereumReceiptAdapter handles receipt parsing
// - gas_used: Extracted from receipt
// - effective_gas_price: Extracted from receipt
// - l1_data_fee: Always None (no L1 fees on Ethereum)
```

### Chain Mapping

```rust
use alloy_chains::NamedChain;
use semioscan::{network_type_for_chain, NetworkType};

// These chains use Ethereum network type
let chains = [
    NamedChain::Mainnet,      // Ethereum
    NamedChain::Sepolia,      // Ethereum testnet
    NamedChain::Holesky,      // Ethereum testnet
    NamedChain::Arbitrum,     // Arbitrum One
    NamedChain::ArbitrumNova, // Arbitrum Nova
    NamedChain::Polygon,      // Polygon PoS
    NamedChain::PolygonAmoy,  // Polygon testnet
];

for chain in chains {
    assert_eq!(network_type_for_chain(chain), NetworkType::Ethereum);
}
```

---

## Optimism Network

Use `op_alloy_network::Optimism` for Optimism Stack (OP-stack) L2 chains.

### When to Use

- Optimism
- Base
- Mode
- Fraxtal
- Zora
- Any chain built on the OP Stack

### Key Characteristics

- Includes L1 data fee in transaction receipts
- Supports deposit transactions (L1 → L2)
- Uses `OpTransactionReceipt` with additional fields

### Example

```rust
use op_alloy_network::Optimism;
use alloy_provider::ProviderBuilder;
use semioscan::{GasCostCalculator, OptimismReceiptAdapter};

// Create provider with Optimism network
let provider = ProviderBuilder::new()
    .network::<Optimism>()
    .connect_http("https://mainnet.base.org".parse()?);

// Use with GasCostCalculator
let calculator = GasCostCalculator::new(provider.root().clone());

// The OptimismReceiptAdapter extracts all cost components:
// - gas_used: L2 execution gas
// - effective_gas_price: L2 gas price
// - l1_data_fee: Cost of posting data to L1 (Some(U256))
```

### L1 Data Fee Explanation

On OP-stack chains, transactions pay two types of fees:

1. **L2 Execution Fee**: `gas_used * effective_gas_price`
2. **L1 Data Fee**: Cost to post transaction data to Ethereum L1

The `OptimismReceiptAdapter` extracts both, enabling accurate total cost calculation:

```rust
use semioscan::ReceiptAdapter;

fn calculate_total_cost<N: Network>(
    adapter: &impl ReceiptAdapter<N>,
    receipt: &N::ReceiptResponse,
) -> U256 {
    let gas_used = adapter.gas_used(receipt);
    let gas_price = adapter.effective_gas_price(receipt);
    let l2_cost = gas_used.saturating_mul(gas_price);

    // L1 data fee is Some for OP-stack, None for Ethereum
    let l1_fee = adapter.l1_data_fee(receipt).unwrap_or_default();

    l2_cost.saturating_add(l1_fee)
}
```

### Chain Mapping

```rust
use alloy_chains::NamedChain;
use semioscan::{network_type_for_chain, NetworkType};

// These chains use Optimism network type
let chains = [
    NamedChain::Optimism,       // Optimism mainnet
    NamedChain::OptimismSepolia,// Optimism testnet
    NamedChain::Base,           // Base mainnet
    NamedChain::BaseSepolia,    // Base testnet
    NamedChain::Mode,           // Mode mainnet
    NamedChain::ModeSepolia,    // Mode testnet
    NamedChain::Fraxtal,        // Fraxtal mainnet
    NamedChain::Zora,           // Zora mainnet
    NamedChain::ZoraSepolia,    // Zora testnet
];

for chain in chains {
    assert_eq!(network_type_for_chain(chain), NetworkType::Optimism);
}
```

---

## AnyNetwork

Use `alloy_network::AnyNetwork` when the chain is determined at runtime or you need to support multiple chains with a single provider type.

### When to Use

- Multi-chain applications with runtime chain selection
- CLI tools where user specifies the chain
- Prototyping or quick scripts
- When simplicity matters more than maximum type safety

### Key Characteristics

- Type-erased network that works with any EVM chain
- Loses some compile-time type information
- Network-specific receipt fields require manual extraction
- Good for read-only operations

### Example

```rust
use alloy_network::AnyNetwork;
use alloy_provider::ProviderBuilder;
use semioscan::{create_http_provider, ProviderConfig};

// Create provider that works with any chain
let provider = create_http_provider(
    ProviderConfig::new("https://eth.llamarpc.com")
)?;

// Or explicitly specify AnyNetwork
let provider = ProviderBuilder::new()
    .network::<AnyNetwork>()
    .connect_http("https://eth.llamarpc.com".parse()?);

// Works for basic operations
let block_number = provider.get_block_number().await?;
let balance = provider.get_balance(address).await?;
```

### Limitations

1. **Network-specific fields require manual extraction**:

```rust
// With AnyNetwork, OP-stack L1 fees are in the `other` field
let receipt = provider.get_transaction_receipt(tx_hash).await?;
if let Some(l1_fee) = receipt.other.get("l1Fee") {
    // Manual deserialization needed
}
```

1. **Less type safety** - mismatched network usage won't cause compile errors

2. **May fail on network-specific operations** - some RPC calls have network-specific responses

---

## Choosing the Right Network

### Decision Tree

```
Is the chain known at compile time?
├── Yes → Do you need maximum type safety?
│   ├── Yes → Is it an OP-stack chain?
│   │   ├── Yes → Use Optimism
│   │   └── No → Use Ethereum
│   └── No → AnyNetwork is acceptable
└── No → Use AnyNetwork
```

### Comparison Matrix

| Feature | Ethereum | Optimism | AnyNetwork |
|---------|----------|----------|------------|
| Compile-time type checking | Full | Full | Partial |
| L1 data fee extraction | N/A | Automatic | Manual |
| Performance | Best | Best | Good |
| Multi-chain support | Single | Single | All |
| Network-specific methods | Full | Full | Limited |

---

## Multi-Chain Application Patterns

### Pattern 1: Compile-Time Network Selection

When you know the chain at compile time, use specific network types:

```rust
use alloy_network::Ethereum;
use op_alloy_network::Optimism;
use semioscan::{GasCostCalculator, EthereumReceiptAdapter, OptimismReceiptAdapter};

// Ethereum mainnet calculator
async fn ethereum_gas_costs(rpc_url: &str) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .connect_http(rpc_url.parse()?);

    let calculator = GasCostCalculator::new(provider.root().clone());
    // Uses EthereumReceiptAdapter internally
    Ok(())
}

// Base L2 calculator
async fn base_gas_costs(rpc_url: &str) -> Result<(), Error> {
    let provider = ProviderBuilder::new()
        .network::<Optimism>()
        .connect_http(rpc_url.parse()?);

    let calculator = GasCostCalculator::new(provider.root().clone());
    // Uses OptimismReceiptAdapter internally, includes L1 fees
    Ok(())
}
```

### Pattern 2: Runtime Network Selection

When the chain is determined at runtime:

```rust
use alloy_chains::NamedChain;
use semioscan::{network_type_for_chain, NetworkType, ProviderConfig, create_http_provider};

async fn get_gas_costs(chain: NamedChain, rpc_url: &str) -> Result<GasCostResult, Error> {
    let config = ProviderConfig::new(rpc_url).with_rate_limit(10);

    match network_type_for_chain(chain) {
        NetworkType::Ethereum => {
            let provider = create_typed_http_provider::<Ethereum>(config)?;
            let calc = GasCostCalculator::new(provider);
            calc.calculate_gas_cost_for_transfers_between_blocks(/*...*/).await
        }
        NetworkType::Optimism => {
            let provider = create_typed_http_provider::<Optimism>(config)?;
            let calc = GasCostCalculator::new(provider);
            calc.calculate_gas_cost_for_transfers_between_blocks(/*...*/).await
        }
    }
}
```

### Pattern 3: Provider Pool for Multi-Chain

For applications that frequently access multiple chains:

```rust
use semioscan::{ProviderPool, ProviderPoolBuilder, ChainEndpoint};
use alloy_chains::NamedChain;
use std::sync::LazyLock;

// Static pool initialized once
static PROVIDERS: LazyLock<ProviderPool> = LazyLock::new(|| {
    ProviderPoolBuilder::new()
        .add_chain(NamedChain::Mainnet, "https://eth.llamarpc.com")
        .add_chain(NamedChain::Base, "https://mainnet.base.org")
        .add_chain(NamedChain::Optimism, "https://mainnet.optimism.io")
        .with_rate_limit(10)
        .build()
        .expect("Failed to create provider pool")
});

async fn get_block_number(chain: NamedChain) -> Result<u64, Error> {
    let provider = PROVIDERS.get(chain).expect("Chain not configured");
    Ok(provider.get_block_number().await?)
}
```

---

## Receipt Adapter Pattern

Semioscan uses the `ReceiptAdapter` trait to abstract network-specific receipt handling:

```rust
use semioscan::{ReceiptAdapter, EthereumReceiptAdapter, OptimismReceiptAdapter};

// For Ethereum-compatible chains
let eth_adapter = EthereumReceiptAdapter;
let gas_used = eth_adapter.gas_used(&receipt);
let l1_fee = eth_adapter.l1_data_fee(&receipt); // Always None

// For OP-stack chains
let op_adapter = OptimismReceiptAdapter;
let gas_used = op_adapter.gas_used(&receipt);
let l1_fee = op_adapter.l1_data_fee(&receipt); // Some(U256)
```

This pattern enables Semioscan's `GasCostCalculator` to work correctly across different networks:

```rust
// Ethereum implementation uses EthereumReceiptAdapter
impl<P: Provider<Ethereum>> GasCostCalculator<Ethereum, P> {
    // L1 fees not included (always None)
}

// Optimism implementation uses OptimismReceiptAdapter
impl<P: Provider<Optimism>> GasCostCalculator<Optimism, P> {
    // L1 fees automatically included
}
```

---

## Common Mistakes

### Mistake 1: Using Ethereum Network for OP-stack Chains

```rust
// Wrong: Using Ethereum network for Base
let provider = ProviderBuilder::new()
    .network::<Ethereum>()  // Incorrect!
    .connect_http("https://mainnet.base.org".parse()?);

// May cause deserialization errors when fetching blocks with deposit transactions
// L1 fees will not be extracted correctly
```

### Mistake 2: Ignoring L1 Data Fees

```rust
// Wrong: Only calculating L2 execution cost
let total_cost = gas_used * effective_gas_price;

// Correct: Include L1 data fee for OP-stack chains
let l1_fee = adapter.l1_data_fee(&receipt).unwrap_or_default();
let total_cost = (gas_used * effective_gas_price) + l1_fee;
```

### Mistake 3: Hardcoding Network Types

```rust
// Wrong: Hardcoding network type
fn calculate_costs(rpc_url: &str, chain: NamedChain) {
    let provider = ProviderBuilder::new()
        .network::<Ethereum>()  // What if chain is Base?
        .connect_http(rpc_url.parse()?);
}

// Correct: Use network_type_for_chain()
fn calculate_costs(rpc_url: &str, chain: NamedChain) {
    match network_type_for_chain(chain) {
        NetworkType::Ethereum => { /* Ethereum provider */ }
        NetworkType::Optimism => { /* Optimism provider */ }
    }
}
```

---

## Summary

| Chain | Network Type | Receipt Adapter | L1 Fees |
|-------|--------------|-----------------|---------|
| Ethereum | `Ethereum` | `EthereumReceiptAdapter` | No |
| Sepolia | `Ethereum` | `EthereumReceiptAdapter` | No |
| Arbitrum | `Ethereum` | `EthereumReceiptAdapter` | No |
| Polygon | `Ethereum` | `EthereumReceiptAdapter` | No |
| Optimism | `Optimism` | `OptimismReceiptAdapter` | Yes |
| Base | `Optimism` | `OptimismReceiptAdapter` | Yes |
| Mode | `Optimism` | `OptimismReceiptAdapter` | Yes |
| Fraxtal | `Optimism` | `OptimismReceiptAdapter` | Yes |
| Zora | `Optimism` | `OptimismReceiptAdapter` | Yes |

---

*Related documentation:*

- [Provider Setup Examples](./PROVIDER_SETUP.md)
- [Alloy Base Prompt](./alloy/base-prompt.md)
- [Improvements Tracking](./IMPROVEMENTS.md)
