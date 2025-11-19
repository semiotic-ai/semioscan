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
//! ## Key Features
//!
//! - Automatic L1 data fee calculation for L2 chains
//! - EIP-4844 blob gas support
//! - Built-in caching for improved performance
//! - Configurable rate limiting and batch sizes
//!
//! ## Internal Modules
//!
//! - `core` - Pure calculation functions
//! - `cache` - Gas result caching
//! - `adapter` - Network-specific logic

pub mod adapter;
pub mod cache;
pub mod calculator;
pub mod core;

// Re-export public API
pub use calculator::*;
pub use core::EventType;
