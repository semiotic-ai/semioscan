// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Block window calculations for daily and custom time periods.
//!
//! This module provides functionality for:
//! - Calculating block ranges for time windows
//! - Daily block window computations
//! - Caching block window results with multiple backends

pub mod cache;
pub mod window;

// Re-export public API
pub use cache::{BlockWindowCache, CacheKey, CacheStats, DiskCache, MemoryCache, NoOpCache};
pub use window::*;
