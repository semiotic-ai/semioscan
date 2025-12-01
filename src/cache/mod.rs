// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Generic caching infrastructure for block-range-based data.
//!
//! This module provides reusable caching abstractions that are used by:
//! - Gas calculation caching
//! - Price calculation caching
//! - Other block-range-based data

pub mod block_range;

// Note: block_range types are internal and not re-exported
