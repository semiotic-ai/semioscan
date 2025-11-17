# Changelog

All notable changes to semioscan will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2025-11-16

### Breaking Changes

**Removed default feature coupling**

- `odos-example` is no longer included in default features
- Users must now explicitly enable `features = ["odos-example"]` if they want the Odos DEX reference implementation
- This reduces dependencies for users who only need core functionality (gas calculation, block windows, event scanning)

### Added

- **RPC Timeout Support**: Configurable timeouts for RPC requests to prevent hanging on unresponsive providers
  - Added `rpc_timeout: Duration` field to `SemioscanConfig` (default: 30 seconds)
  - Added `rpc_timeout: Option<Duration>` to `ChainConfig` for per-chain overrides
  - Added `RpcError::Timeout` variant for timeout errors
  - Added `SemioscanConfigBuilder::rpc_timeout()` method
  - Added `SemioscanConfigBuilder::chain_timeout()` method for per-chain configuration
  - Added `SemioscanConfig::get_rpc_timeout()` method

- **Documentation**: Added comprehensive open-source preparation documentation
  - `SECURITY.md`: Security policy, vulnerability reporting, and security considerations
  - `CODE_OF_CONDUCT.md`: Contributor Covenant v2.1 code of conduct
  - `ROADMAP.md`: Version milestones and development roadmap
  - `docs/STAFF_REVIEW.md`: Comprehensive staff engineer review for open-sourcing

### Changed

- **README**: Updated feature flag documentation to reflect that `odos-example` is optional, not default
- **Configuration**: All builder methods now properly preserve the new `rpc_timeout` field when updating chain overrides

### Migration Guide

**For Users Relying on Default Features**:

If you were implicitly using the Odos price source via default features, you now need to explicitly enable it:

```toml
# Before (v0.2.x) - odos-example included by default
[dependencies]
semioscan = "0.2"

# After (v0.3.0) - explicitly enable if needed
[dependencies]
semioscan = { version = "0.3", features = ["odos-example"] }

# Or if you only need core functionality
[dependencies]
semioscan = "0.3"  # No Odos dependency
```

**For Users Implementing Custom Configurations**:

Chain configuration structs now include an `rpc_timeout` field:

```rust
// Before (v0.2.x)
let chain_config = ChainConfig {
    max_block_range: Some(MaxBlockRange::new(1000)),
    rate_limit_delay: Some(Duration::from_millis(250)),
};

// After (v0.3.0)
let chain_config = ChainConfig {
    max_block_range: Some(MaxBlockRange::new(1000)),
    rate_limit_delay: Some(Duration::from_millis(250)),
    rpc_timeout: None,  // Use default or specify custom timeout
};
```

## [0.2.0] - 2025-11-15

### Breaking Changes

**Semioscan is now a library-only crate**. All application-layer functionality (binaries, CLI, API server) has been removed to make the crate more focused and reusable.

#### Removed

- **All binaries and application code** (~1,150 LOC removed):
  - CLI entry point (`src/main.rs`)
  - CLI bootstrapping (`src/bootstrap.rs`)
  - CLI commands (`src/command.rs`)
  - HTTP API server (`src/api.rs`)
- **Provider creation module** (`src/provider.rs`, 265 LOC):
  - Removed `create_ethereum_provider()` and `create_optimism_provider()` functions
  - Removed `ChainFeatures` trait
  - Provider creation is now the responsibility of application code (see Migration Guide below)
- **Feature flags**:
  - Removed `cli` feature (CLI code removed)
  - Removed `api-server` feature (API server code removed)
  - Removed `core` feature (all features are now part of core library)
- **Cloud infrastructure**:
  - Removed `infra/semioscan/` directory
  - Removed semioscan Cloud Run service from GCP deployment
- **Dependencies**:
  - Removed `clap` (CLI parsing)
  - Removed `axum` (HTTP server)
  - Removed `tower` and `tower-http` (API middleware)

#### Migration Guide

**For Applications Using Semioscan**:

If your application was using semioscan's provider creation functions, you now need to create providers yourself using [Alloy](https://github.com/alloy-rs/alloy):

```rust
// Before (v0.1.x) - provider creation in semioscan
use semioscan::{create_ethereum_provider, create_optimism_provider};
let provider = create_ethereum_provider(NamedChain::Mainnet)?;

// After (v0.2.0) - use Alloy directly
use alloy_provider::ProviderBuilder;
let rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY";
let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
```

If you were using semioscan as a CLI tool or API server, those features have been removed. The library now focuses exclusively on providing reusable analytics primitives. You can build your own CLI/API using the library components.

### Added

#### New Architecture

- **Trait-based price extraction system**:
  - `PriceSource` trait for implementing custom DEX price extractors
  - Object-safe design allows runtime pluggability via `Box<dyn PriceSource>`
  - `SwapData` struct as common format for swap events
  - `OdosPriceSource` as reference implementation (behind `odos-example` feature)

- **Configuration system**:
  - `SemioscanConfig` for customizing RPC behavior per chain
  - `SemioscanConfigBuilder` for fluent API configuration
  - Chain-specific overrides for block ranges and rate limiting
  - Sane defaults for common chains (Base, Sonic, Arbitrum)

- **Enhanced documentation**:
  - Comprehensive README with quick start guides
  - Detailed rustdoc API documentation for all public types
  - Uniswap V3 implementation example in trait docs
  - Module-level documentation with examples

#### New Features

- **Flexible provider injection**: All calculators now accept providers via constructor rather than creating them internally
- **Configuration support**: All calculators support optional `SemioscanConfig` for customizing RPC behavior
- **Better error types**: `PriceSourceError` with clear `DecodeError` and `InvalidSwapData` variants

### Changed

#### API Changes

- **`PriceCalculator` is now generic over `PriceSource`**:

  ```rust
  // Before (v0.1.x) - hardcoded to Odos
  let calculator = PriceCalculator::new(provider);

  // After (v0.2.0) - generic over any PriceSource implementation
  let price_source = OdosPriceSource::new(router_address);
  let calculator = PriceCalculator::with_price_source(
      provider,
      Box::new(price_source)
  );
  ```

- **Feature flags simplified**:
  - `default = ["odos-example"]` - includes Odos reference implementation
  - `odos-example` - optional Odos DEX support (requires `odos-sdk` and `usdshe`)
  - All other functionality is always included (no feature gates for core library)

- **Gas calculation constants deprecated**:
  - `MAX_BLOCK_RANGE` constant deprecated in favor of `SemioscanConfig.max_block_range`
  - Use `config.get_max_block_range(chain)` for chain-specific limits

#### Module Organization

- **`price` module made public**:
  - `PriceSource` trait exported at `semioscan::price::PriceSource`
  - `SwapData` struct exported at `semioscan::price::SwapData`
  - `odos` submodule available with `odos-example` feature

- **Removed CLI-specific code**:
  - Removed `SupportedEvent` enum (CLI-specific)
  - Removed API handler methods from `gas.rs` and `price_calculator.rs`

### Fixed

- **Improved type safety**: Provider functions now use `NamedChain` consistently
- **Better documentation coverage**: All public types now have comprehensive rustdoc comments
- **Cleaner dependency tree**: Removed unused CLI and HTTP server dependencies

### Internal

- **Code size reduction**: ~1,415 lines of application code removed
- **Dependency cleanup**: Removed 5 dependencies (`clap`, `axum`, `tower`, `tower-http`, `http`)
- **Testing improvements**: All 16 unit tests passing, zero clippy warnings

## [0.1.0] - 2025-11-10

Initial internal release as part of Likwid workspace.

### Features

- Gas cost calculation for L1 and L2 chains
- Block window calculation for UTC dates
- Price extraction from Odos DEX events
- Transfer amount tracking for ERC-20 tokens
- Multi-chain support (12+ EVM chains)
- HTTP API server for price queries
- CLI tool for blockchain analytics

---

**Notes**:

- Version 0.1.x was used internally within the Likwid workspace
- Version 0.2.0 is the first version prepared for public open-source release
- This changelog will be maintained going forward for all public releases
