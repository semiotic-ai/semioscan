// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Strong types for type safety across semioscan.
//!
//! This module provides newtype wrappers for various domain concepts:
//! - Wei amounts and gas calculations
//! - Token amounts and decimals
//! - Configuration values (block ranges, rate limits)
//! - Fee calculations
//! - Cache metadata (timestamps, access sequences)
//! - Price source errors (type-safe error handling without type erasure)

pub mod cache;
pub mod config;
pub mod fees;
pub mod gas;
pub mod price;
pub mod tokens;
pub mod wei;

// Note: Public types are re-exported from lib.rs, not here
