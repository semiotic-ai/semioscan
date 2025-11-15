//! Error types for the semioscan library.
//!
//! This module provides strongly-typed errors for all public APIs in semioscan.
//! It follows a hybrid approach:
//!
//! - **Module-specific errors** for fine-grained error handling (`BlockWindowError`,
//!   `GasCalculationError`, etc.)
//! - **Unified error type** (`SemioscanError`) for convenience when you don't need
//!   to distinguish between error sources
//!
//! # Architecture
//!
//! Each major module has its own error type:
//! - [`BlockWindowError`] - Errors from block window calculations
//! - [`GasCalculationError`] - Errors from gas cost calculations
//! - [`PriceCalculationError`] - Errors from price calculations (wraps [`PriceSourceError`])
//! - [`EventProcessingError`] - Errors from event scanning and processing
//! - [`RetrievalError`] - Errors from combined data retrieval operations
//!
//! Additionally, [`RpcError`] provides shared error variants for blockchain RPC operations.
//!
//! # Examples
//!
//! ## Fine-grained error handling
//!
//! ```rust,ignore
//! use semioscan::{BlockWindowCalculator, BlockWindowError};
//! use alloy_chains::NamedChain;
//! use chrono::NaiveDate;
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     let calculator = BlockWindowCalculator::new(provider, "cache.json");
//!     let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
//!
//!     match calculator.get_daily_window(NamedChain::Arbitrum, date).await {
//!         Ok(window) => println!("Block window: {:?}", window),
//!         Err(BlockWindowError::InvalidRange { reason }) => {
//!             eprintln!("Invalid range: {}", reason);
//!         }
//!         Err(BlockWindowError::Rpc(rpc_err)) => {
//!             eprintln!("RPC failure, retrying...: {}", rpc_err);
//!         }
//!         Err(e) => eprintln!("Other error: {}", e),
//!     }
//!     Ok(())
//! }
//! ```
//!
//! ## Using the unified error type
//!
//! ```rust,ignore
//! use semioscan::{SemioscanError, BlockWindowCalculator};
//! use alloy_chains::NamedChain;
//! use chrono::NaiveDate;
//!
//! async fn example() -> Result<(), SemioscanError> {
//!     let calculator = BlockWindowCalculator::new(provider, "cache.json");
//!     let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
//!     let window = calculator.get_daily_window(NamedChain::Arbitrum, date).await?;
//!     // Errors automatically convert to SemioscanError via From implementations
//!     Ok(())
//! }
//! ```

mod blocks;
mod events;
mod gas;
mod price;
mod retrieval;
mod rpc;

pub use blocks::BlockWindowError;
pub use events::EventProcessingError;
pub use gas::GasCalculationError;
pub use price::PriceCalculationError;
pub use retrieval::RetrievalError;
pub use rpc::RpcError;

/// Unified error type for all semioscan operations.
///
/// This enum wraps all module-specific error types, providing a convenient way to
/// handle errors when you don't need to distinguish between different error sources.
///
/// All module-specific error types automatically convert to `SemioscanError` via
/// `From` implementations, so you can use `?` to propagate errors naturally.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::{SemioscanError, BlockWindowCalculator, GasCostCalculator};
/// use alloy_chains::NamedChain;
/// use chrono::NaiveDate;
///
/// async fn process_data() -> Result<(), SemioscanError> {
///     let block_calc = BlockWindowCalculator::new(provider, "cache.json");
///     let gas_calc = GasCostCalculator::new(provider);
///
///     // Both error types automatically convert to SemioscanError
///     let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
///     let window = block_calc.get_daily_window(NamedChain::Arbitrum, date).await?;
///     let gas_cost = gas_calc.calculate_gas_cost_for_transfers_between_blocks(
///         NamedChain::Arbitrum, from_address, to_address, token_address, start_block, end_block
///     ).await?;
///
///     Ok(())
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum SemioscanError {
    /// Error from block window calculations.
    #[error("Block window error: {0}")]
    BlockWindow(#[from] BlockWindowError),

    /// Error from gas cost calculations.
    #[error("Gas calculation error: {0}")]
    Gas(#[from] GasCalculationError),

    /// Error from price calculations.
    #[error("Price calculation error: {0}")]
    Price(#[from] PriceCalculationError),

    /// Error from event processing operations.
    #[error("Event processing error: {0}")]
    Events(#[from] EventProcessingError),

    /// Error from combined data retrieval operations.
    #[error("Data retrieval error: {0}")]
    Retrieval(#[from] RetrievalError),
}
