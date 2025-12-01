// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Semioscan: Blockchain analytics library for EVM chains
//!
//! Semioscan provides production-grade tools for:
//! - Gas cost calculation (L1 and L2 chains)
//! - Price extraction from DEX events
//! - Block window calculations
//! - Token event processing
//!
//! # Domain Organization
//!
//! - `types` - Strong types for type safety
//! - `config` - Configuration system
//! - `gas` - Gas calculation domain
//! - `price` - Price extraction domain
//! - `blocks` - Block window calculations
//! - `events` - Event processing
//! - `cache` - Caching infrastructure (internal)
//! - `retrieval` - Data orchestration (internal)
//! - `tracing` - Observability (internal)

// === Module Declarations ===
mod blocks;
mod cache;
pub mod config;
pub mod errors;
mod events;
mod gas;
pub mod price;
mod retrieval;
mod tracing;
mod types;

// === Core Types (from types/) ===
pub use types::config::{BlockCount, MaxBlockRange, TransactionCount};
pub use types::fees::{L1DataFee, Percentage};
pub use types::gas::{BlobCount, BlobGasAmount, GasAmount, GasPrice};
pub use types::tokens::{
    NormalizedAmount, TokenAmount, TokenDecimals, TokenPrice, TokenSet, UsdValue, UsdValueError,
};
pub use types::wei::WeiAmount;

// === Configuration (from config/) ===
pub use config::constants;
pub use config::{ChainConfig, SemioscanConfig, SemioscanConfigBuilder};

// === Error Types (from errors/) ===
pub use errors::{
    BlockWindowError, EventProcessingError, GasCalculationError, PriceCalculationError,
    RetrievalError, RpcError, SemioscanError,
};

// === Gas Calculation (from gas/) ===
pub use gas::adapter::{EthereumReceiptAdapter, OptimismReceiptAdapter, ReceiptAdapter};
pub use gas::cache::GasCache;
pub use gas::{EventType, GasCostCalculator, GasCostResult, GasForTx};

// === Price Extraction (from price/) ===
// Core trait and types are always available
pub use price::{PriceSource, PriceSourceError, SwapData};
// Calculator is feature-gated
#[cfg(feature = "odos-example")]
pub use price::odos::OdosPriceSource;
#[cfg(feature = "odos-example")]
pub use price::{PriceCalculator, TokenPriceResult};

// === Block Windows (from blocks/) ===
pub use blocks::{
    BlockWindowCache, BlockWindowCalculator, CacheKey, CacheStats, DailyBlockWindow, DiskCache,
    MemoryCache, NoOpCache, UnixTimestamp,
};

// === Cache Types (from blocks/cache/types, re-exported via types/cache) ===
pub use types::cache::{AccessSequence, TimestampMillis};

// === Events (from events/) ===
pub use events::{extract_transferred_to_tokens, extract_transferred_to_tokens_with_config};
pub use events::{AmountCalculator, AmountResult};
pub use events::{Approval, Transfer};

// === Retrieval (Data Orchestration) ===
pub use retrieval::{
    get_token_decimal_precision, u256_to_bigdecimal, CombinedCalculator, CombinedDataResult,
    DecimalPrecision,
};

// Re-export RouterType from odos-sdk for convenience
#[cfg(feature = "odos-example")]
pub use odos_sdk::RouterType;

// Note: Cache internals (cache::BlockRangeCache) and tracing spans are NOT re-exported
// as they are implementation details. Users can access them via fully-qualified paths if needed.
