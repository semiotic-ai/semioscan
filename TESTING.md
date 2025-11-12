# Testing Strategy

This document explains semioscan's testing approach and philosophy.

## Core Principle

**Test your code, not your dependencies.**

We focus testing efforts on semioscan's business logic, data structures, and algorithms. We trust that external dependencies (alloy, odos-sdk, etc.) are tested by their maintainers.

## What We Test

### 1. Business Logic Without External Dependencies

Tests that validate semioscan's core algorithms and logic:

- **Configuration validation** (`DailyBlockWindow::new`)
  - Invalid block ranges (end < start)
  - Invalid timestamp ranges
  - Edge cases (zero values, large numbers)

- **Gas cache operations** (`GasCache`)
  - Gap calculation and merging logic
  - Cache hit/miss behavior
  - Overlap detection and consolidation

- **Data structure invariants**
  - Block counting arithmetic
  - Range validation
  - Overflow protection (saturating arithmetic)

Example from `tests/gas_calculator_tests.rs`:
```rust
#[test]
fn test_calculate_gaps() {
    let mut cache = GasCache::default();
    // Insert ranges: [100-200], [300-400], [600-700]
    cache.insert(...);

    // Calculate gaps for range [50-800]
    let (result, gaps) = cache.calculate_gaps(1, from, to, 50, 800);

    // Expected gaps: 50-99, 201-299, 401-599, 701-800
    assert_eq!(gaps.len(), 4);
    assert_eq!(gaps[0], (50, 99));
    // ...
}
```

### 2. Public API Contracts

Tests that ensure the library's public interface behaves correctly:

- **Type safety** (newtypes prevent value mixing)
  - `ChainId` cannot be mixed with raw `u64`
  - `UnixTimestamp` cannot be mixed with raw `i64`
  - Compilation errors catch misuse at compile time

- **Trait object safety**
  - `PriceSource` can be used as `Box<dyn PriceSource>`
  - Generic parameters work as expected

- **Constructor behavior**
  - Builders return expected types
  - Optional parameters work correctly

### 3. Error Handling

Tests that validate error cases are handled appropriately:

- **Invalid inputs return errors** (not panics)
- **Error messages are descriptive**
- **Edge cases don't cause crashes**

Example from `tests/block_window_tests.rs`:
```rust
#[test]
fn test_block_window_validation_errors() {
    // Error: end_block < start_block
    let result = DailyBlockWindow::new(2000, 1000, start_ts, end_ts);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid block range"));
}
```

## What We Don't Test

### 1. Blockchain Interactions

We don't test that Ethereum returns correct data or that RPC endpoints work:

- Block timestamp accuracy
- Transaction receipt validity
- Event log formats

**Rationale**: Blockchain behavior is external to semioscan. Testing it would require expensive integration tests or mocks that add complexity without validating our code.

### 2. External Dependencies

We don't test that alloy or odos-sdk work correctly:

- `alloy_provider::Provider` trait implementations
- Event parsing from `alloy_sol_types`
- Odos API responses

**Rationale**: These are tested by their maintainers. Our tests would duplicate effort and become brittle when dependencies update.

### 3. Complex Provider Mocking

We avoid mocking alloy's `Provider` trait for algorithmic tests:

- Binary search algorithms in `BlockWindowCalculator`
- Block timestamp fetching logic

**Rationale**: The `Provider` trait is complex (multiple lifetime parameters, async methods, transport abstractions). Mocking it correctly requires more test code than the feature being tested. Instead:

- Algorithm correctness is validated through examples that connect to real chains
- Production usage provides ongoing validation
- The test/value ratio favors examples over mocks for this use case

## How to Add Tests

### For Business Logic

1. **Extract logic into pure functions when possible**
   ```rust
   // Good: pure function, easy to test
   fn calculate_gap(start: u64, end: u64, cached: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
       // ...
   }

   // Test all edge cases without external dependencies
   #[test]
   fn test_gap_with_overlapping_ranges() { ... }
   ```

2. **Use simple mocks for complex dependencies**
   - Only mock what you need
   - Keep mocks in test modules
   - Don't try to fully implement complex traits

3. **Focus on edge cases and invariants**
   - Zero values
   - Maximum values (u64::MAX)
   - Empty inputs
   - Single-element inputs
   - Overlapping ranges

### For Integration Testing

Use the `examples/` directory for integration tests that require real blockchain data:

```rust
// examples/daily_block_window.rs
// Demonstrates calculating block windows for a specific date
// Validates binary search against real Arbitrum blocks
fn main() -> Result<()> {
    let provider = ProviderBuilder::new().on_http(rpc_url);
    let calculator = BlockWindowCalculator::new(provider, "cache.json");

    let window = calculator.get_daily_window(NamedChain::Arbitrum, date).await?;

    println!("Blocks: {} to {}", window.start_block, window.end_block);
    // Manually validate output against block explorer
    Ok(())
}
```

**Benefits of examples over tests:**
- Connect to real networks (no mocking required)
- Demonstrate actual usage patterns
- Serve as documentation
- Can be run manually for validation
- Show performance characteristics

## Test Organization

```
crates/semioscan/
├── src/
│   ├── lib.rs
│   ├── block_window.rs      # Contains: mod tests { ... }
│   ├── gas_cache.rs          # Contains: mod tests { ... }
│   └── ...
├── tests/                     # Integration tests
│   ├── block_window_tests.rs
│   ├── gas_calculator_tests.rs
│   └── price_source_tests.rs
└── examples/                  # Real-world validation
    ├── daily_block_window.rs
    ├── base_oct15_2025.rs
    └── ...
```

- **Unit tests** (`src/*/mod tests`): Test internal functions with full access to private items
- **Integration tests** (`tests/`): Test public API as external users would
- **Examples** (`examples/`): Validate against real blockchains, demonstrate usage

## Running Tests

```bash
# Run all tests
cargo test --package semioscan

# Run specific test file
cargo test --package semioscan --test block_window_tests

# Run unit tests only (in src/)
cargo test --package semioscan --lib

# Run with all features
cargo test --package semioscan --all-features

# Run examples (manual validation)
RPC_URL=https://arb1.arbitrum.io/rpc cargo run --package semioscan --example daily_block_window
```

## Test Coverage Goals

We aim for:

- **High coverage** (>80%) of business logic
- **Complete coverage** of error paths
- **Comprehensive** edge case testing
- **Minimal** mocking complexity

We explicitly **do not** aim for:

- 100% code coverage (diminishing returns)
- Coverage of third-party dependencies
- Testing every possible blockchain scenario

## When to Add Tests vs. Examples

| Scenario | Use Test | Use Example |
|----------|----------|-------------|
| Pure business logic | ✅ Test | ❌ No |
| Data structure validation | ✅ Test | ❌ No |
| Error handling | ✅ Test | ❌ No |
| Type safety | ✅ Test | ❌ No |
| RPC interaction | ❌ No | ✅ Example |
| Binary search with real blocks | ❌ No | ✅ Example |
| PriceSource with real events | ❌ No | ✅ Example |
| Gas calculation on real txns | ❌ No | ✅ Example |

## Contributing Tests

When contributing, please:

1. **Add tests for new business logic** (required)
2. **Add tests for new error cases** (required)
3. **Add examples for new Provider-dependent features** (recommended)
4. **Update this document if testing strategy changes** (recommended)
5. **Don't mock complex external dependencies** (avoid)

## Questions?

- "Should I mock the Provider?" → Probably not. Use an example instead.
- "Should I test this error path?" → Yes! Error tests are valuable and simple.
- "Should I test that alloy parses events correctly?" → No, that's alloy's job.
- "Should I test gap calculation logic?" → Yes! That's semioscan's business logic.

---

**Summary**: Test what semioscan does, not what its dependencies do. Use examples to validate integration with real blockchains. Keep tests simple and focused on business logic.
