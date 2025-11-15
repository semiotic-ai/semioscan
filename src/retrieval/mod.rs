//! Data retrieval orchestration.
//!
//! This module handles coordinated retrieval of blockchain data including:
//! - Combined gas and price data extraction
//! - Transfer amount calculations
//! - Decimal precision handling

// Combined retrieval sub-modules
mod calculator;
mod decimal_precision;
mod gas_calculation;
mod types;
mod utils;

// Re-export public API
pub use calculator::CombinedCalculator;
pub use decimal_precision::DecimalPrecision;
pub use types::CombinedDataResult;
pub use utils::{get_token_decimal_precision, u256_to_bigdecimal};
