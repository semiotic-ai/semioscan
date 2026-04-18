# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build / lint / fmt
cargo build --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt

# Tests — CI runs all three feature combos; run them locally before pushing
cargo test                          # default features
cargo test --all-features           # includes odos-example
cargo test --no-default-features

# Single test / doctest
cargo test <test_name>              # substring match across all test harnesses
cargo test --test rate_limiting_tests <test_name>
cargo test --doc <path::to::item>

# Examples requiring odos (the feature-gated path)
cargo run --example router_token_discovery --features odos-example

# Publish dry run (CI gate)
cargo publish --dry-run

# REUSE/SPDX compliance (CI gate)
reuse lint
```

`Cargo.lock` is `.gitignored` — this is a library crate by design, so CI always resolves dependencies fresh. A local `cargo update` only affects your working tree.

## Architecture

**Semioscan is a library-only crate** — no binaries, CLI, server, or database. Consumers bring their own Alloy provider and integrate the types directly.

### Module layout (`src/`)

Top-level modules split into **public** (part of the API surface, re-exported from `lib.rs`) and **private** (implementation detail):

- **Public**: `config`, `errors`, `price`, `provider`, `transport`
- **Private** (pub items re-exported selectively via `lib.rs`): `blocks`, `cache`, `events`, `gas`, `retrieval`, `tracing`, `types`

`lib.rs` is the **single source of truth for the public API**. When adding a new type, decide there whether to re-export, and keep the existing `// === Section ===` comment structure. Internal types stay reachable via fully-qualified paths if consumers need them.

### Domain boundaries

- `gas/` — L1 + L2 gas cost calculation. L2 (Optimism Stack) chains automatically include L1 data fees via `OptimismReceiptAdapter`; L1 chains use `EthereumReceiptAdapter`. EIP-4844 blob gas lives in `gas::blob`.
- `blocks/` — Maps UTC dates to block ranges. Results are cached (disk/memory/noop backends); past dates are immutable, so caching is effectively free.
- `price/` — `PriceSource` trait is the extension point. Consumers implement it per DEX. The `OdosPriceSource` implementation is gated behind the `odos-example` feature and serves as a reference.
- `events/` — Log scanning + `EventScanner` (supports WebSocket via the `ws` feature).
- `provider/` — Provider construction, pooling, and the `network_type_for_chain` dispatcher that picks `Ethereum` vs `Optimism` network type at runtime.
- `transport/` — Tower layers: `RateLimitLayer`, `RetryLayer` with exponential backoff.
- `retrieval/` — High-level orchestration (`CombinedCalculator`) that composes gas + price + balance fetches with partial-failure reporting.
- `types/` — Newtype wrappers (`WeiAmount`, `GasAmount`, `TokenAmount`, `UsdValue`, `NormalizedAmount`, etc.). Use these instead of bare `U256`/`u64`.

### Feature flags

- `default = []` — minimal core
- `ws` — enables WebSocket transport (`alloy-provider/pubsub` + `ws`) and `create_ws_provider`
- `odos-example` — pulls in `odos-sdk` and enables `OdosPriceSource`, `PriceCalculator`, and the `router_token_discovery` example

Any new feature-gated public export needs the matching `#[cfg(feature = "...")]` on the `pub use` line in `lib.rs`.

### Testing strategy

- Unit tests live alongside code in `src/`; integration tests in `tests/`; property tests use `proptest` (see `tests/rate_limiting_property_tests.rs`).
- RPC-dependent workflows belong in `examples/`, not tests — CI runs tests without network.
- Test the library's own logic, not Alloy/tokio/etc. Don't write tests that exercise dependencies.

## Conventions

- Every source/config file needs an SPDX header (`REUSE.toml` already covers docs, Cargo.toml, `.github/**`, example JSON). The `reuse` CI check will fail otherwise.
- Use `dotenvy` (not `dotenv`) for env loading in examples.
- String interpolation: named captures (`format!("{val}")`), not positional (`format!("{}", val)`).
- Finish work with `cargo clippy --all-targets --all-features -- -D warnings` — CI matrix runs clippy on all three feature combos.
- Rust MSRV: 1.92 (`rust-version` in `Cargo.toml`).

## Release flow

Tags matching `v*` trigger `.github/workflows/release.yml`, which gates on CI → `cargo semver-checks check-release` → `cargo publish`. Bump `version` in `Cargo.toml`, update `CHANGELOG.md`, then push the tag.
