/// Template example showing how to implement PriceSource for a custom DEX
///
/// This example demonstrates:
/// 1. How to implement the PriceSource trait for any DEX
/// 2. How to decode DEX-specific swap events
/// 3. How to integrate with PriceCalculator
/// 4. Best practices for error handling and filtering
///
/// This is a template/tutorial - adapt it for your specific DEX protocol.
///
/// Run with:
/// ```bash
/// cargo run --package semioscan --example custom_dex_integration
/// ```
///
/// # Steps to Integrate Your DEX
///
/// 1. **Define your swap events** using alloy_sol_types::sol!
/// 2. **Implement PriceSource trait** with three required methods:
///    - `router_address()`: Return the DEX contract address
///    - `event_topics()`: Return event signature hashes to filter
///    - `extract_swap_from_log()`: Parse events into SwapData
/// 3. **Optional**: Implement `should_include_swap()` for custom filtering
/// 4. **Use with PriceCalculator** to extract prices from blockchain
///
/// # Example: Uniswap V3 Integration
///
/// This template shows how to integrate Uniswap V3 as an example.
/// Follow the same pattern for any DEX (Curve, Balancer, etc.)
use alloy_primitives::{address, Address, B256, I256, U256};
use alloy_rpc_types::Log;
use alloy_sol_types::{sol, SolEvent};

// Import the PriceSource trait and related types
use semioscan::price::{PriceSource, PriceSourceError, SwapData};

// =============================================================================
// Step 1: Define Your DEX Events
// =============================================================================

// Define the Uniswap V3 Swap event using alloy's sol! macro
sol! {
    /// Uniswap V3 Pool Swap event
    ///
    /// Emitted when a swap occurs in a Uniswap V3 pool
    #[derive(Debug)]
    event UniswapV3Swap(
        address indexed sender,
        address indexed recipient,
        int256 amount0,
        int256 amount1,
        uint160 sqrtPriceX96,
        uint128 liquidity,
        int24 tick
    );
}

// =============================================================================
// Step 2: Implement PriceSource for Your DEX
// =============================================================================

/// Custom price source for Uniswap V3
///
/// This demonstrates how to extract swap data from a Uniswap V3 pool.
/// The key challenge with Uniswap V3 is handling signed amounts (int256)
/// and determining swap direction.
pub struct UniswapV3PriceSource {
    /// The Uniswap V3 pool contract address to monitor
    pool_address: Address,
    /// Token0 in the pool (lower address)
    token0: Address,
    /// Token1 in the pool (higher address)
    token1: Address,
    /// Optional: Only include swaps from this address
    allowed_sender: Option<Address>,
}

impl UniswapV3PriceSource {
    /// Create a new Uniswap V3 price source
    ///
    /// # Arguments
    ///
    /// * `pool_address` - The Uniswap V3 pool contract address
    /// * `token0` - The first token in the pool (lower address)
    /// * `token1` - The second token in the pool (higher address)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // USDC/WETH pool on Ethereum (0.05% fee tier)
    /// let pool = "0x88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640".parse().unwrap();
    /// let usdc = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap();
    /// let weth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap();
    ///
    /// let price_source = UniswapV3PriceSource::new(pool, usdc, weth);
    /// ```
    pub fn new(pool_address: Address, token0: Address, token1: Address) -> Self {
        Self {
            pool_address,
            token0,
            token1,
            allowed_sender: None,
        }
    }

    /// Add a sender filter (optional)
    ///
    /// When set, only swaps initiated by this address will be included.
    pub fn with_sender_filter(mut self, sender: Address) -> Self {
        self.allowed_sender = Some(sender);
        self
    }

    /// Convert signed amount to unsigned
    ///
    /// Uniswap V3 uses signed amounts where:
    /// - Negative = tokens leaving the pool (user receiving)
    /// - Positive = tokens entering the pool (user paying)
    fn to_unsigned(amount: I256) -> U256 {
        if amount.is_negative() {
            amount.unsigned_abs()
        } else {
            amount.into_raw()
        }
    }
}

// =============================================================================
// Step 3: Implement Required PriceSource Methods
// =============================================================================

impl PriceSource for UniswapV3PriceSource {
    /// Return the pool address to monitor
    fn router_address(&self) -> Address {
        self.pool_address
    }

    /// Return the event signature hashes to filter for
    ///
    /// For Uniswap V3, we only need the Swap event signature
    fn event_topics(&self) -> Vec<B256> {
        vec![UniswapV3Swap::SIGNATURE_HASH]
    }

    /// Extract swap data from a Uniswap V3 Swap event
    ///
    /// This is the core parsing logic. Key steps:
    /// 1. Decode the event using alloy's SolEvent::decode_log
    /// 2. Determine swap direction from amount signs
    /// 3. Convert signed amounts to unsigned
    /// 4. Create SwapData with correct token ordering
    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        // Decode the Uniswap V3 Swap event
        let event = UniswapV3Swap::decode_log(&log.clone().into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Determine swap direction based on amount signs
        // In Uniswap V3:
        // - If amount0 is negative, user received token0 (sold token1)
        // - If amount0 is positive, user paid token0 (bought token1)
        let (token_in, token_in_amount, token_out, token_out_amount) =
            if event.amount0.is_negative() {
                // User received token0, so they sold token1
                // token_in = token1, token_out = token0
                (
                    self.token1,
                    Self::to_unsigned(event.amount1),
                    self.token0,
                    Self::to_unsigned(event.amount0),
                )
            } else {
                // User paid token0, so they bought token1
                // token_in = token0, token_out = token1
                (
                    self.token0,
                    Self::to_unsigned(event.amount0),
                    self.token1,
                    Self::to_unsigned(event.amount1),
                )
            };

        // Validate amounts are non-zero
        if token_in_amount.is_zero() || token_out_amount.is_zero() {
            return Err(PriceSourceError::InvalidSwapData(
                "Zero amount in swap".to_string(),
            ));
        }

        Ok(Some(SwapData {
            token_in,
            token_in_amount,
            token_out,
            token_out_amount,
            sender: Some(event.sender),
        }))
    }

    /// Optional: Filter swaps by sender address
    ///
    /// If allowed_sender is set, only include swaps from that address
    fn should_include_swap(&self, swap: &SwapData) -> bool {
        match self.allowed_sender {
            Some(allowed) => swap.sender == Some(allowed),
            None => true, // Include all swaps if no filter is set
        }
    }
}

// =============================================================================
// Step 4: Demonstrate Usage
// =============================================================================

fn main() {
    println!("\n=== Custom DEX Integration Template ===\n");

    println!("This example shows how to integrate any DEX with semioscan's");
    println!("PriceSource trait. Follow these steps:\n");

    println!("1. Define Your DEX Events");
    println!("   ├─ Use alloy_sol_types::sol! macro");
    println!("   ├─ Copy event signatures from your DEX contract");
    println!("   └─ Example: UniswapV3Swap event above\n");

    println!("2. Create a Price Source Struct");
    println!("   ├─ Store contract address(es)");
    println!("   ├─ Store token addresses (if needed)");
    println!("   └─ Add optional filters (sender, etc.)\n");

    println!("3. Implement PriceSource Trait");
    println!("   ├─ router_address() → contract to monitor");
    println!("   ├─ event_topics() → event signatures");
    println!("   ├─ extract_swap_from_log() → parse events");
    println!("   └─ should_include_swap() → optional filtering\n");

    println!("4. Use with PriceCalculator");
    println!("   ├─ Create your PriceSource impl");
    println!("   ├─ Wrap in Box<dyn PriceSource>");
    println!("   └─ Pass to PriceCalculator::with_price_source()\n");

    // Example instantiation
    let pool = address!("88e6A0c2dDD26FEEb64F039a2c41296FcB3f5640"); // USDC/WETH 0.05%
    let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
    let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");

    let _price_source = UniswapV3PriceSource::new(pool, usdc, weth);

    println!("Example: Uniswap V3 USDC/WETH Pool");
    println!("  Pool: {}", pool);
    println!("  Token0 (USDC): {}", usdc);
    println!("  Token1 (WETH): {}", weth);
    println!("\n  Price source created successfully!\n");

    println!("=== Real Usage Example ===\n");
    println!("```rust");
    println!("use semioscan::{{PriceCalculator, price::PriceSource}};");
    println!("use alloy_provider::ProviderBuilder;");
    println!();
    println!("// Create your custom price source");
    println!("let price_source = UniswapV3PriceSource::new(pool, usdc, weth);");
    println!();
    println!("// Create provider");
    println!("let provider = ProviderBuilder::new()");
    println!("    .connect_http(rpc_url.parse()?);");
    println!();
    println!("// Create calculator with your price source");
    println!("let calculator = PriceCalculator::with_price_source(");
    println!("    provider.root().clone(),");
    println!("    Box::new(price_source),");
    println!(");");
    println!();
    println!("// Extract prices");
    println!("let result = calculator.get_price_for_token_pair(");
    println!("    chain_id,");
    println!("    weth,  // token_in");
    println!("    usdc,  // token_out");
    println!("    start_block,");
    println!("    end_block,");
    println!(").await?;");
    println!();
    println!("println!(\"Average price: {{}}\", result.average_price_display());");
    println!("```\n");

    println!("=== DEX-Specific Considerations ===\n");

    println!("Uniswap V2/V3:");
    println!("  - Pool address is the router");
    println!("  - Handle signed amounts (V3)");
    println!("  - Determine direction from amount signs\n");

    println!("Curve:");
    println!("  - Pool address is the router");
    println!("  - Multiple event types (TokenExchange, etc.)");
    println!("  - May need to track pool indices\n");

    println!("Balancer:");
    println!("  - Vault address is the router");
    println!("  - PoolId identifies specific pools");
    println!("  - Multi-token swaps possible\n");

    println!("Aggregators (1inch, Odos, etc.):");
    println!("  - Router address for all swaps");
    println!("  - Filter by specific tokens");
    println!("  - May have multiple event types\n");

    println!("=== Testing Your Implementation ===\n");
    println!("1. Start with a known swap transaction hash");
    println!("2. Verify event decoding works correctly");
    println!("3. Check token direction is correct");
    println!("4. Validate amounts match blockchain data");
    println!("5. Test filtering logic (if implemented)\n");

    println!("=== Additional Resources ===\n");
    println!("- Alloy docs: https://alloy.rs");
    println!("- Semioscan examples: crates/semioscan/examples/");
    println!("- Odos implementation: crates/semioscan/src/price/odos.rs");
}
