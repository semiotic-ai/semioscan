//! Block window calculations for daily and custom time periods.
//!
//! This module provides functionality for:
//! - Calculating block ranges for time windows
//! - Daily block window computations
//! - Caching block window results

pub mod window;

// Re-export public API
pub use window::*;
