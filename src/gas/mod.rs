// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Gas calculation domain for EVM chains.
//!
//! This module provides production-grade gas cost calculation for L1 (Ethereum)
//! and L2 (Arbitrum, Base, Optimism, etc.) chains.
//!
//! ## Public API
//!
//! - [`GasCalculator`] - Main entry point for calculating gas costs
//! - [`GasCostResult`] - Result containing total gas cost and metadata
//! - [`EventType`] - Types of ERC-20 events to track
//!
//! ## EIP-4844 Blob Gas
//!
//! The [`blob`] module provides utilities for working with EIP-4844 blob transactions:
//! - [`blob::get_blob_base_fee`] - Fetch current blob base fee from latest block
//! - [`blob::estimate_blob_cost`] - Estimate cost for N blobs
//! - [`blob::calculate_blob_gas`] - Pure calculation of blob gas units
//!
//! ## Key Features
//!
//! - Automatic L1 data fee calculation for L2 chains
//! - EIP-4844 blob gas support with detailed breakdowns
//! - Built-in caching for improved performance
//! - Configurable rate limiting and batch sizes
//!
//! ## Internal Modules
//!
//! - `core` - Pure calculation functions
//! - `cache` - Gas result caching
//! - `adapter` - Network-specific logic

pub mod adapter;
pub mod blob;
pub mod cache;
pub mod calculator;
pub mod core;

// Re-export public API
pub use calculator::*;
pub use core::EventType;
