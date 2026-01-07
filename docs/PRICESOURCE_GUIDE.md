# PriceSource Trait Implementation Guide

This guide teaches you how to implement the `PriceSource` trait to add support for any DEX protocol in semioscan.

## Table of Contents

1. [Overview](#overview)
2. [Understanding the PriceSource Trait](#understanding-the-pricesource-trait)
3. [Step-by-Step Implementation](#step-by-step-implementation)
4. [Complete Examples](#complete-examples)
5. [Best Practices](#best-practices)
6. [Testing Your Implementation](#testing-your-implementation)
7. [Common Pitfalls](#common-pitfalls)

## Overview

The `PriceSource` trait is the core extensibility mechanism in semioscan for extracting price data from DEX swap events. By implementing this trait, you can add support for:

- **DEX Aggregators** (Odos, 1inch, Cowswap)
- **AMM Protocols** (Uniswap V2/V3, Curve, Balancer)
- **Limit Order DEXes** (Odos LO routers)
- **Custom Trading Protocols**

### Why Trait-Based?

The trait-based design provides:

- **Type Safety**: Compiler-enforced implementation requirements
- **Runtime Flexibility**: Object-safe trait allows `Box<dyn PriceSource>`
- **Testability**: Easy to mock and test in isolation
- **Composability**: Mix multiple price sources in the same application

## Understanding the PriceSource Trait

### Trait Definition

```rust
pub trait PriceSource: Send + Sync {
    fn router_address(&self) -> Address;
    fn event_topics(&self) -> Vec<B256>;
    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError>;
    fn should_include_swap(&self, _swap: &SwapData) -> bool {
        true  // Default: accept all swaps
    }
}
```

### Required Methods

1. **`router_address()`**: Returns the contract address to scan for events
2. **`event_topics()`**: Returns the event signature hashes to filter for
3. **`extract_swap_from_log()`**: Parses a log entry into `SwapData` format

### Optional Methods

- **`should_include_swap()`**: Filter swaps after extraction (e.g., by sender address)

### SwapData Structure

All `PriceSource` implementations must produce `SwapData`:

```rust
pub struct SwapData {
    /// Token that was sold (input token)
    pub token_in: Address,
    /// Amount of input token (raw U256, not normalized)
    pub token_in_amount: U256,
    /// Token that was bought (output token)
    pub token_out: Address,
    /// Amount of output token (raw U256, not normalized)
    pub token_out_amount: U256,
    /// Optional: sender address (for filtering)
    pub sender: Option<Address>,
    /// Optional: transaction hash (populated from log metadata)
    pub tx_hash: Option<B256>,
    /// Optional: block number (populated from log metadata)
    pub block_number: Option<BlockNumber>,
}
```

**Important**: Token amounts are raw `U256` values. Semioscan handles decimal normalization automatically.

## Step-by-Step Implementation

### Step 1: Define the Swap Event

First, use Alloy's `sol!` macro to define the on-chain event:

```rust
use alloy_sol_types::sol;

sol! {
    // Example: Uniswap V2 Swap event
    event SwapV2(
        address indexed sender,
        uint256 amount0In,
        uint256 amount1In,
        uint256 amount0Out,
        uint256 amount1Out,
        address indexed to
    );
}
```

**Tips**:

- Find the event definition in the protocol's smart contract source
- Use Etherscan to view verified contract code
- The event name must match exactly (case-sensitive)

### Step 2: Create the Struct

Define a struct to hold configuration needed for event extraction:

```rust
pub struct UniswapV2PriceSource {
    pool_address: Address,
    token0: Address,
    token1: Address,
}

impl UniswapV2PriceSource {
    pub fn new(pool_address: Address, token0: Address, token1: Address) -> Self {
        Self {
            pool_address,
            token0,
            token1,
        }
    }
}
```

**What to store**:

- Contract address(es) to scan
- Token addresses (for multi-token pools)
- Any filtering criteria (e.g., allowed senders)

### Step 3: Implement Required Methods

#### 3a. Implement `router_address()`

```rust
impl PriceSource for UniswapV2PriceSource {
    fn router_address(&self) -> Address {
        self.pool_address
    }
}
```

**For aggregators**: Return the router address
**For AMMs**: Return the pool address
**For protocols with multiple contracts**: Return the primary swap contract

#### 3b. Implement `event_topics()`

```rust
impl PriceSource for UniswapV2PriceSource {
    fn event_topics(&self) -> Vec<B256> {
        vec![SwapV2::SIGNATURE_HASH]
    }
}
```

**Multiple events**: Return all relevant event signatures:

```rust
fn event_topics(&self) -> Vec<B256> {
    vec![
        SwapEvent::SIGNATURE_HASH,
        SwapMultiEvent::SIGNATURE_HASH,
    ]
}
```

#### 3c. Implement `extract_swap_from_log()`

This is the core parsing logic. Here's the pattern:

```rust
impl PriceSource for UniswapV2PriceSource {
    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        // 1. Decode the event
        let event = SwapV2::decode_log(&log.into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // 2. Determine swap direction
        let (token_in, amount_in, token_out, amount_out) = if event.amount0In > U256::ZERO {
            // Swapping token0 for token1
            (self.token0, event.amount0In, self.token1, event.amount1Out)
        } else {
            // Swapping token1 for token0
            (self.token1, event.amount1In, self.token0, event.amount0Out)
        };

        // 3. Validate the swap
        if amount_in.is_zero() || amount_out.is_zero() {
            return Err(PriceSourceError::InvalidSwapData(
                "Zero swap amounts".to_string()
            ));
        }

        // 4. Return SwapData
        Ok(Some(SwapData {
            token_in,
            token_in_amount: amount_in,
            token_out,
            token_out_amount: amount_out,
            sender: Some(event.sender),
            tx_hash: log.transaction_hash,
            block_number: log.block_number,
        }))
    }
}
```

**Key steps**:

1. Decode the log using the event type
2. Determine which token was input vs output
3. Validate the data (non-zero amounts, expected structure)
4. Map to `SwapData` format

### Step 4: Optional Filtering

If you want to filter swaps (e.g., only from a specific liquidator address):

```rust
impl PriceSource for UniswapV2PriceSource {
    fn should_include_swap(&self, swap: &SwapData) -> bool {
        // Example: only swaps from a specific address
        swap.sender.map_or(false, |s| s == self.allowed_sender)
    }
}
```

**Common filters**:

- Sender address matching
- Minimum swap size thresholds
- Specific token pair combinations

## Complete Examples

### Example 1: Uniswap V3 (Signed Amounts)

Uniswap V3 uses signed integers to indicate swap direction:

```rust
use semioscan::price::{PriceSource, SwapData, PriceSourceError};
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::Log;
use alloy_sol_types::sol;

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

impl UniswapV3PriceSource {
    pub fn new(pool_address: Address, token0: Address, token1: Address) -> Self {
        Self {
            pool_address,
            token0,
            token1,
        }
    }
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

        // Negative amount means token was sent out (input)
        // Positive amount means token was received (output)
        let (token_in, amount_in, token_out, amount_out) = if event.amount0.is_negative() {
            // Sent token0, received token1
            (
                self.token0,
                event.amount0.unsigned_abs(),
                self.token1,
                U256::try_from(event.amount1)
                    .map_err(|_| PriceSourceError::InvalidSwapData("amount1 negative".into()))?
            )
        } else {
            // Sent token1, received token0
            (
                self.token1,
                event.amount1.unsigned_abs(),
                self.token0,
                U256::try_from(event.amount0)
                    .map_err(|_| PriceSourceError::InvalidSwapData("amount0 negative".into()))?
            )
        };

        Ok(Some(SwapData {
            token_in,
            token_in_amount: amount_in,
            token_out,
            token_out_amount: amount_out,
            sender: Some(event.sender),
            tx_hash: log.transaction_hash,
            block_number: log.block_number,
        }))
    }
}
```

### Example 2: Multi-Token Swaps (Odos Pattern)

For DEX aggregators that support multi-token swaps:

```rust
use semioscan::price::{PriceSource, SwapData, PriceSourceError};
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::Log;
use alloy_sol_types::sol;

sol! {
    event SwapMulti(
        address sender,
        uint256[] amountsIn,
        address[] tokensIn,
        uint256[] amountsOut,
        address[] tokensOut,
        uint32 referralCode
    );
}

pub struct AggregatorPriceSource {
    router_address: Address,
}

impl PriceSource for AggregatorPriceSource {
    fn router_address(&self) -> Address {
        self.router_address
    }

    fn event_topics(&self) -> Vec<B256> {
        vec![SwapMulti::SIGNATURE_HASH]
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapMulti::decode_log(&log.into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Validate array lengths
        if event.tokensIn.is_empty() || event.tokensOut.is_empty() {
            return Err(PriceSourceError::InvalidSwapData(
                "Empty token arrays".to_string()
            ));
        }

        if event.amountsIn.len() != event.tokensIn.len() {
            return Err(PriceSourceError::InvalidSwapData(
                "Mismatched input arrays".to_string()
            ));
        }

        // For multi-token swaps, extract first input and output
        // (You can modify this logic to handle multiple tokens differently)
        Ok(Some(SwapData {
            token_in: event.tokensIn[0],
            token_in_amount: event.amountsIn[0],
            token_out: event.tokensOut[0],
            token_out_amount: event.amountsOut[0],
            sender: Some(event.sender),
            tx_hash: log.transaction_hash,
            block_number: log.block_number,
        }))
    }
}
```

### Example 3: Curve StableSwap

Curve uses indexed token parameters:

```rust
sol! {
    event TokenExchange(
        address indexed buyer,
        int128 sold_id,
        uint256 tokens_sold,
        int128 bought_id,
        uint256 tokens_bought
    );
}

pub struct CurvePriceSource {
    pool_address: Address,
    tokens: Vec<Address>,  // Ordered list of tokens in pool
}

impl PriceSource for CurvePriceSource {
    fn router_address(&self) -> Address {
        self.pool_address
    }

    fn event_topics(&self) -> Vec<B256> {
        vec![TokenExchange::SIGNATURE_HASH]
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = TokenExchange::decode_log(&log.into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Convert token IDs to addresses
        let sold_id = event.sold_id as usize;
        let bought_id = event.bought_id as usize;

        if sold_id >= self.tokens.len() || bought_id >= self.tokens.len() {
            return Err(PriceSourceError::InvalidSwapData(
                format!("Token ID out of bounds: sold={}, bought={}", sold_id, bought_id)
            ));
        }

        Ok(Some(SwapData {
            token_in: self.tokens[sold_id],
            token_in_amount: event.tokens_sold,
            token_out: self.tokens[bought_id],
            token_out_amount: event.tokens_bought,
            sender: Some(event.buyer),
            tx_hash: log.transaction_hash,
            block_number: log.block_number,
        }))
    }
}
```

## Best Practices

### 1. Error Handling

**DO**: Provide clear error messages

```rust
if event.tokensIn.is_empty() {
    return Err(PriceSourceError::InvalidSwapData(
        "tokensIn array is empty - expected at least one token".to_string()
    ));
}
```

**DON'T**: Use generic errors

```rust
// Bad: unclear what went wrong
return Err(PriceSourceError::InvalidSwapData("bad data".to_string()));
```

### 2. Return `None` for Irrelevant Events

If a log isn't relevant to price calculation, return `Ok(None)`:

```rust
fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
    let event = MySwapEvent::decode_log(&log.into())
        .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

    // Skip internal swaps or zero-amount swaps
    if event.sender == event.recipient || event.amount.is_zero() {
        return Ok(None);
    }

    Ok(Some(SwapData { /* ... */ }))
}
```

### 3. Validate All Invariants

Check for invalid data before constructing `SwapData`:

```rust
// Check array lengths match
if event.tokensIn.len() != event.amountsIn.len() {
    return Err(PriceSourceError::InvalidSwapData(
        format!(
            "Token count mismatch: {} tokens, {} amounts",
            event.tokensIn.len(),
            event.amountsIn.len()
        )
    ));
}

// Check for zero amounts
if amount_in.is_zero() || amount_out.is_zero() {
    return Ok(None);  // Valid event, but not relevant for price
}
```

### 4. Document Your Implementation

Add module-level docs explaining:

```rust
//! Uniswap V3 price source implementation
//!
//! # Pool Discovery
//!
//! Uniswap V3 pools must be discovered off-chain using the Factory contract.
//! Each pool address corresponds to a specific token pair and fee tier.
//!
//! # Example
//!
//! ```rust,ignore
//! let pool = "0x...".parse()?;
//! let token0 = "0x...".parse()?;
//! let token1 = "0x...".parse()?;
//!
//! let price_source = UniswapV3PriceSource::new(pool, token0, token1);
//! ```
```

### 5. Test with Real Data

Always test your implementation against real blockchain logs:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_uniswap_v3_swap() {
        // Use a real log from a transaction
        let log_data = "0x...";  // Raw log data from Etherscan

        let price_source = UniswapV3PriceSource::new(/* ... */);
        let result = price_source.extract_swap_from_log(&log).unwrap();

        assert!(result.is_some());
        let swap = result.unwrap();
        assert_eq!(swap.token_in, expected_token);
        assert_eq!(swap.token_in_amount, expected_amount);
    }
}
```

## Testing Your Implementation

### Unit Testing

Test the `extract_swap_from_log()` method in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloy_rpc_types::Log;
    use alloy_primitives::B256;

    #[test]
    fn test_valid_swap() {
        let price_source = MyPriceSource::new(/* ... */);

        // Create a mock log with known data
        let log = Log {
            address: my_router_address,
            topics: vec![MySwapEvent::SIGNATURE_HASH],
            data: /* encoded event data */,
            ..Default::default()
        };

        let result = price_source.extract_swap_from_log(&log).unwrap();
        assert!(result.is_some());

        let swap = result.unwrap();
        assert_eq!(swap.token_in, expected_token_in);
        assert_eq!(swap.token_in_amount, expected_amount_in);
    }

    #[test]
    fn test_invalid_data_returns_error() {
        let price_source = MyPriceSource::new(/* ... */);

        // Create a log with malformed data
        let log = /* invalid log */;

        let result = price_source.extract_swap_from_log(&log);
        assert!(result.is_err());
    }

    #[test]
    fn test_irrelevant_log_returns_none() {
        let price_source = MyPriceSource::new(/* ... */);

        // Create a log that should be skipped
        let log = /* zero-amount swap */;

        let result = price_source.extract_swap_from_log(&log).unwrap();
        assert!(result.is_none());
    }
}
```

### Integration Testing

Test with real blockchain data using an example:

```rust
// examples/my_dex_price.rs
use semioscan::price::my_dex::MyDexPriceSource;
use semioscan::PriceCalculator;
use alloy_provider::ProviderBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = ProviderBuilder::new()
        .connect_http(std::env::var("RPC_URL")?.parse()?);

    let price_source = MyDexPriceSource::new(/* ... */);
    let calculator = PriceCalculator::with_price_source(
        provider,
        Box::new(price_source)
    );

    // Test against a known block range with known swaps
    let token = "0x...".parse()?;
    let result = calculator
        .get_price(token, start_block, end_block)
        .await?;

    println!("Average price: {}", result.average_price);
    println!("Swap count: {}", result.swap_count);

    Ok(())
}
```

## Common Pitfalls

### 1. Forgetting Indexed Parameters

**Wrong**: Omitting `indexed` in event definition

```rust
sol! {
    event Swap(address sender, uint256 amount);  // Missing 'indexed'
}
```

**Right**: Include `indexed` exactly as in contract

```rust
sol! {
    event Swap(address indexed sender, uint256 amount);  // Correct
}
```

### 2. Token Amount Normalization

**Wrong**: Normalizing amounts in `PriceSource`

```rust
// Don't do this - semioscan handles normalization
let normalized = amount / U256::from(10u128.pow(decimals));
```

**Right**: Return raw amounts

```rust
// Correct - return raw U256 values
Ok(Some(SwapData {
    token_in: event.token_in,
    token_in_amount: event.amount_in,  // Raw amount
    token_out: event.token_out,
    token_out_amount: event.amount_out,  // Raw amount
    sender: Some(event.sender),
    tx_hash: log.transaction_hash,
    block_number: log.block_number,
}))
```

### 3. Hardcoding Token Addresses

**Wrong**: Hardcoding specific token addresses

```rust
pub struct MyPriceSource {
    router: Address,
    // Bad: hardcoded USDC address
}

impl PriceSource for MyPriceSource {
    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap();
        // ...
    }
}
```

**Right**: Make token addresses configurable

```rust
pub struct MyPriceSource {
    router: Address,
    token0: Address,  // Configurable
    token1: Address,  // Configurable
}
```

### 4. Incorrect Sign Handling

For protocols using signed integers (e.g., Uniswap V3):

**Wrong**: Casting without checking sign

```rust
let amount = U256::from(event.amount0);  // Panics if negative!
```

**Right**: Check sign and use `unsigned_abs()`

```rust
let amount = if event.amount0.is_negative() {
    event.amount0.unsigned_abs()
} else {
    U256::try_from(event.amount0)?
};
```

### 5. Missing Array Length Validation

**Wrong**: Indexing without bounds checking

```rust
let token_in = event.tokens[0];  // Panics if empty!
```

**Right**: Validate array lengths first

```rust
if event.tokens.is_empty() {
    return Err(PriceSourceError::InvalidSwapData(
        "Empty tokens array".to_string()
    ));
}
let token_in = event.tokens[0];
```

## Next Steps

1. **Study the reference implementations**:
   - `OdosPriceSource` in `src/price/odos.rs`
   - Uniswap V3 example in `src/price/mod.rs` docs

2. **Try implementing a price source for your DEX**:
   - Start with a simple single-event protocol
   - Add tests with real blockchain data
   - Contribute back to the community!

3. **Join the discussion**:
   - Open issues for questions
   - Submit PRs for new implementations
   - Share your use cases

## Additional Resources

- [Alloy Documentation](https://alloy.rs)
- [Semioscan API Docs](https://docs.rs/semioscan)
- [ERC-20 Token Standard](https://eips.ethereum.org/EIPS/eip-20)
- [Uniswap V2 Core](https://github.com/Uniswap/v2-core)
- [Uniswap V3 Core](https://github.com/Uniswap/v3-core)
