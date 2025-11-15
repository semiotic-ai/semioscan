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
//!     | × price_per_token
//!     ↓
//! USD value (f64)
//! ```

mod amount;
mod decimals;
mod normalized;
mod set;
mod usd;

pub use amount::TokenAmount;
pub use decimals::TokenDecimals;
pub use normalized::NormalizedAmount;
pub use set::TokenSet;
pub use usd::UsdValue;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_full_workflow() {
        // Simulate receiving a token transfer and calculating USD value
        let raw_amount = TokenAmount::new(U256::from(250_000_000u64)); // 250 USDC
        let decimals = TokenDecimals::USDC;
        let price_per_usdc = 1.0; // $1 per USDC

        let normalized = raw_amount.normalize(decimals);
        assert_eq!(normalized.as_f64(), 250.0);

        let usd_value = normalized.to_usd(price_per_usdc);
        assert_eq!(usd_value, UsdValue::new(250.0));
    }
}
