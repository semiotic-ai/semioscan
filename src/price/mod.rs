//! Price extraction from DEX swap events
//!
//! This module provides a trait-based architecture for extracting price data from DEX events.
//! Users can implement the [`PriceSource`] trait to support any DEX protocol.
//!
//! # Architecture
//!
//! The price calculation workflow:
//!
//! 1. **PriceCalculator** scans blockchain logs using filters from [`PriceSource::event_topics`]
//! 2. For each log, calls [`PriceSource::extract_swap_from_log`] to parse swap data
//! 3. Filters swaps using [`PriceSource::should_include_swap`]
//! 4. Normalizes token amounts and aggregates into price results
//!
//! # Example: Implementing PriceSource for Uniswap V3
//!
//! ```rust,ignore
//! use semioscan::price::{PriceSource, SwapData, PriceSourceError};
//! use alloy_primitives::{Address, B256};
//! use alloy_rpc_types::Log;
//! use alloy_sol_types::sol;
//!
//! // Define Uniswap V3 Swap event
//! sol! {
//!     event SwapV3(
//!         address indexed sender,
//!         address indexed recipient,
//!         int256 amount0,
//!         int256 amount1,
//!         uint160 sqrtPriceX96,
//!         uint128 liquidity,
//!         int24 tick
//!     );
//! }
//!
//! pub struct UniswapV3PriceSource {
//!     pool_address: Address,
//!     token0: Address,
//!     token1: Address,
//! }
//!
//! impl PriceSource for UniswapV3PriceSource {
//!     fn router_address(&self) -> Address {
//!         self.pool_address
//!     }
//!
//!     fn event_topics(&self) -> Vec<B256> {
//!         vec![SwapV3::SIGNATURE_HASH]
//!     }
//!
//!     fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
//!         let event = SwapV3::decode_log(&log.into())
//!             .map_err(|e| PriceSourceError::DecodeError(e.to_string()))?;
//!
//!         // Determine direction based on sign of amounts
//!         let (token_in, amount_in, token_out, amount_out) = if event.amount0.is_negative() {
//!             (self.token0, event.amount0.unsigned_abs(), self.token1, event.amount1.into())
//!         } else {
//!             (self.token1, event.amount1.unsigned_abs(), self.token0, event.amount0.into())
//!         };
//!
//!         Ok(Some(SwapData {
//!             token_in,
//!             amount_in,
//!             token_out,
//!             amount_out,
//!             sender: Some(event.sender),
//!         }))
//!     }
//! }
//! ```
//!
//! # Odos Example Implementation
//!
//! An Odos DEX implementation is available behind the `odos-example` feature flag:
//!
//! ```toml
//! [dependencies]
//! semioscan = { version = "0.1", features = ["odos-example"] }
//! ```
//!
//! See [`odos::OdosPriceSource`] for implementation details.

use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::Log;

#[cfg(feature = "odos-example")]
pub mod odos;

/// Represents a single token swap extracted from on-chain events
///
/// This is the core data structure that [`PriceSource`] implementations must produce.
/// Token amounts are raw U256 values (not normalized) - the [`PriceCalculator`](crate::price::PriceCalculator)
/// handles decimal normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapData {
    /// Token that was sold (input token)
    pub token_in: Address,
    /// Amount of input token sold (raw U256, not normalized for decimals)
    pub token_in_amount: U256,
    /// Token that was bought (output token)
    pub token_out: Address,
    /// Amount of output token received (raw U256, not normalized for decimals)
    pub token_out_amount: U256,
    /// Optional: transaction initiator (useful for filtering specific addresses)
    pub sender: Option<Address>,
}

/// Trait for extracting price data from DEX swap events
///
/// Implement this trait to add support for any DEX protocol. The trait is object-safe,
/// allowing runtime pluggability via `Box<dyn PriceSource>`.
///
/// # Design Philosophy
///
/// - **Synchronous**: All methods are synchronous since log processing doesn't require async
/// - **Minimal**: Only the essential methods needed for event extraction
/// - **Flexible**: Default implementations for optional behavior like filtering
///
/// # Required Methods
///
/// - [`router_address`](PriceSource::router_address): Contract address to scan for events
/// - [`event_topics`](PriceSource::event_topics): Event signatures to filter logs
/// - [`extract_swap_from_log`](PriceSource::extract_swap_from_log): Parse log into swap data
///
/// # Optional Methods
///
/// - [`should_include_swap`](PriceSource::should_include_swap): Filter swaps (default: accept all)
pub trait PriceSource: Send + Sync {
    /// Returns the contract address to scan for swap events
    ///
    /// For DEXes like Uniswap, this is typically the pool address.
    /// For aggregators like Odos, this is the router address.
    fn router_address(&self) -> Address;

    /// Returns the event topic hashes to filter for
    ///
    /// These are used to create efficient RPC filters. Return all event signatures
    /// that represent swaps in your DEX protocol.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// fn event_topics(&self) -> Vec<B256> {
    ///     vec![
    ///         SwapEvent::SIGNATURE_HASH,
    ///         SwapMultiEvent::SIGNATURE_HASH,
    ///     ]
    /// }
    /// ```
    fn event_topics(&self) -> Vec<B256>;

    /// Extract swap data from a log entry
    ///
    /// This is the core parsing logic that decodes DEX-specific events into the generic
    /// [`SwapData`] format.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(SwapData))` - Successfully extracted a relevant swap
    /// - `Ok(None)` - Log is not a relevant swap event (e.g., wrong token pair)
    /// - `Err(PriceSourceError)` - Failed to decode the log
    ///
    /// # Error Handling
    ///
    /// Return `DecodeError` if the log doesn't match the expected event structure.
    /// Return `InvalidSwapData` if the event data is malformed (e.g., empty arrays).
    fn extract_swap_from_log(&self, log: &Log) -> Result<Option<SwapData>, PriceSourceError>;

    /// Optional filter to exclude certain swaps
    ///
    /// Default implementation accepts all swaps. Override to implement custom filtering
    /// logic (e.g., only swaps from a specific sender address).
    ///
    /// # Example: Filter by sender
    ///
    /// ```rust,ignore
    /// fn should_include_swap(&self, swap: &SwapData) -> bool {
    ///     swap.sender.map_or(false, |s| s == self.allowed_sender)
    /// }
    /// ```
    fn should_include_swap(&self, _swap: &SwapData) -> bool {
        true // Accept all swaps by default
    }
}

/// Errors that can occur when extracting price data from logs
#[derive(Debug, thiserror::Error)]
pub enum PriceSourceError {
    /// Failed to decode an event from the log data
    ///
    /// This typically means the log doesn't match the expected event signature,
    /// or the event data is corrupted.
    #[error("Failed to decode event: {0}")]
    DecodeError(String),

    /// Event was decoded but the data is invalid
    ///
    /// Examples: empty token arrays, zero amounts, mismatched array lengths.
    #[error("Invalid swap data: {0}")]
    InvalidSwapData(String),
}
