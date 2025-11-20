//! Strong types for token-related values
//!
//! This module provides newtype wrappers for token operations
//! to add type safety and prevent mixing incompatible units.
//!
//! # Type Relationships
//!
//! ```text
//! TokenAmount (U256, raw)
//!     |
//!     | normalize(TokenDecimals)
//!     ↓
//! NormalizedAmount (f64, human-readable)
//!     |
//!     | × TokenPrice
//!     ↓
//! UsdValue (f64, USD-denominated)
//! ```

mod amount;
mod decimals;
mod normalized;
mod price;
mod set;
mod usd;

pub use amount::TokenAmount;
pub use decimals::TokenDecimals;
pub use normalized::NormalizedAmount;
pub use price::TokenPrice;
pub use set::TokenSet;
pub use usd::{UsdValue, UsdValueError};
