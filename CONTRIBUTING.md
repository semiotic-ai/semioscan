# Contributing to Semioscan

Thank you for your interest in contributing to semioscan! This document provides guidelines and instructions for contributing to the project.

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

## How to Contribute

### Reporting Issues

- Search existing issues before creating a new one
- Provide clear reproduction steps for bugs
- Include relevant system information (OS, Rust version, chain being used)
- For feature requests, explain the use case and why it's valuable

### Pull Requests

1. Fork the repository and create a branch from `main`
2. Make your changes following our coding standards
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test --package semioscan --all-features`
5. Run clippy and fix warnings: `cargo clippy --package semioscan --all-targets --all-features -- -D warnings`
6. Format your code: `cargo fmt --package semioscan`
7. Update documentation if needed
8. Submit a pull request with a clear description of changes

## Development Setup

### Prerequisites

- Rust 1.89 or later
- Git

### Building and Testing

```bash
# Build the library
cargo build --package semioscan

# Run unit tests
cargo test --package semioscan --lib

# Run integration tests
cargo test --package semioscan

# Run with all features
cargo test --package semioscan --all-features

# Check code quality
cargo clippy --package semioscan --all-targets --all-features -- -D warnings

# Format code
cargo fmt --package semioscan
```

### Testing Strategy

Semioscan uses a pragmatic testing approach focused on testing library code, not external dependencies:

**Unit Tests** (`src/**/*.rs` and `tests/*.rs`):

- Test business logic: configuration, caching, data structures
- Run fast with no external dependencies
- Focus on edge cases, error handling, and invariants
- Examples: Config validation, gas cache operations, price source event parsing

**Examples** (`examples/*.rs`):

- Demonstrate real-world usage patterns with actual blockchain data
- Require RPC connections to live chains
- Serve as both documentation and integration validation
- See [examples/README.md](examples/README.md) for comprehensive example documentation

**When to Write Tests vs. Examples:**

- **Tests**: Pure logic, error handling, data structures, type safety
- **Examples**: RPC interactions, real blockchain data, end-to-end workflows

## Adding Support for a New DEX Protocol

One of the most valuable contributions is implementing the `PriceSource` trait for new DEX protocols. Here's how:

### Step 1: Define the Swap Event

Use Alloy's `sol!` macro to define the on-chain event:

```rust
use alloy_sol_types::sol;

sol! {
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

Find event definitions in the protocol's smart contract source (usually on Etherscan).

### Step 2: Create the PriceSource Implementation

```rust
use semioscan::price::{PriceSource, SwapData, PriceSourceError};
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::Log;

pub struct YourDexPriceSource {
    pool_address: Address,
    token0: Address,
    token1: Address,
}

impl PriceSource for YourDexPriceSource {
    fn router_address(&self) -> Address {
        self.pool_address
    }

    fn event_topics(&self) -> Vec<B256> {
        vec![SwapV2::SIGNATURE_HASH]
    }

    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        let event = SwapV2::decode_log(&log.into())
            .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;

        // Your parsing logic here
        Ok(Some(SwapData {
            token_in: self.token0,
            token_in_amount: event.amount0In,
            token_out: self.token1,
            token_out_amount: event.amount1Out,
            sender: Some(event.sender),
            tx_hash: log.transaction_hash,
            block_number: log.block_number,
        }))
    }
}
```

### Step 3: Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_topics() {
        let source = YourDexPriceSource::new(/* ... */);
        let topics = source.event_topics();
        assert!(!topics.is_empty());
    }

    #[test]
    fn test_extract_swap_from_log() {
        // Test with sample log data
        // Verify SwapData is correctly extracted
    }
}
```

### Step 4: Add Documentation

- Add rustdoc comments to your implementation
- Consider creating an example in `examples/` showing usage
- Update the main README if this is a popular DEX

### Step 5: Submit PR

Include in your PR description:

- Which DEX protocol this supports
- Links to contract addresses and documentation
- Example usage
- Which chains it's been tested on

For detailed guidance, see [`docs/PRICESOURCE_GUIDE.md`](docs/PRICESOURCE_GUIDE.md).

## Coding Standards

### Rust Style

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for consistent formatting
- Run `cargo clippy` and address all warnings
- Write clear, self-documenting code with helpful comments

### Documentation

- Add rustdoc comments for all public APIs
- Include usage examples in doc comments
- Document units for numeric values (wei, raw amounts, etc.)
- Explain complex logic with inline comments

### Commit Messages

- Use clear, descriptive commit messages
- Format: `category(scope): description`
  - Categories: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`
  - Example: `feat(price): add Uniswap V3 price source implementation`

### Testing Requirements

- All new functionality must have tests or examples
- Unit tests for pure logic, error handling, and data structures
- Examples for blockchain interactions and real-world usage patterns
- Follow the "Tests vs. Examples" strategy outlined above

## Project Structure

```text
semioscan/
├── src/
│   ├── lib.rs               # Public API exports
│   ├── blocks/              # Block window calculations
│   ├── cache/               # Block range and gas caching
│   ├── config/              # Configuration and constants
│   ├── errors/              # Comprehensive error types
│   ├── events/              # Event scanning and discovery
│   ├── gas/                 # Gas calculation (EIP-1559, EIP-4844)
│   ├── price/               # PriceSource trait and implementations
│   ├── retrieval/           # Data retrieval and calculations
│   ├── tracing/             # Structured logging utilities
│   └── types/               # Strong types (Wei, TokenAmount, etc.)
├── tests/                   # Integration tests
├── examples/                # Real-world usage examples
├── docs/                    # Additional documentation
├── README.md
├── CHANGELOG.md
└── Cargo.toml
```

## Release Process

(For maintainers)

1. Update `CHANGELOG.md` with all changes
2. Bump version in `Cargo.toml`
3. Run full test suite: `cargo test --package semioscan --all-features`
4. Verify publish: `cargo publish --dry-run --package semioscan`
5. Create git tag: `git tag -a semioscan-v0.x.0 -m "Release v0.x.0"`
6. Publish: `cargo publish --package semioscan`
7. Push tag: `git push origin semioscan-v0.x.0`

## Getting Help

- **Documentation**: Start with the [README](README.md) and [PriceSource Guide](docs/PRICESOURCE_GUIDE.md)
- **Examples**: See [examples/README.md](examples/README.md) for comprehensive usage examples and troubleshooting
- **Issues**: Search existing issues or create a new one
- **Discussions**: Open a discussion for questions or ideas

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
