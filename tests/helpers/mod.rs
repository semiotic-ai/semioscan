// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Test helpers for semioscan integration tests
//!
//! Provides mock implementations of traits to enable testing without
//! real blockchain connections.

use alloy_primitives::{Address, LogData, B256, U256};
use alloy_rpc_types::Log;
use semioscan::price::{PriceSource, PriceSourceError, SwapData};

/// Mock PriceSource for testing PriceCalculator logic
///
/// Allows complete control over swap extraction and filtering
/// behavior without requiring real blockchain events.
///
/// # Example
///
/// ```rust,ignore
/// let mock = MockPriceSource::new(router_address)
///     .with_swaps(vec![
///         SwapData { token_in: token_a, token_out: usdc, ... },
///     ])
///     .with_filter(|swap| swap.token_in_amount > MIN_THRESHOLD);
///
/// let calculator = PriceCalculator::new(provider, usdc, Box::new(mock));
/// ```
pub struct MockPriceSource {
    router: Address,
    swaps: Vec<SwapData>,
    filter_fn: Box<dyn Fn(&SwapData) -> bool + Send + Sync>,
    swap_index: std::sync::Mutex<usize>,
}

impl MockPriceSource {
    /// Create a new MockPriceSource with no swaps
    pub fn new(router: Address) -> Self {
        Self {
            router,
            swaps: Vec::new(),
            filter_fn: Box::new(|_| true), // Accept all by default
            swap_index: std::sync::Mutex::new(0),
        }
    }

    /// Set the swaps that will be returned by extract_swap_from_log
    ///
    /// Each call to extract_swap_from_log will cycle through the provided swaps.
    pub fn with_swaps(mut self, swaps: Vec<SwapData>) -> Self {
        self.swaps = swaps;
        self
    }

    /// Set the filter function for should_include_swap
    ///
    /// By default, all swaps are included. Use this to test filtering logic.
    pub fn with_filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&SwapData) -> bool + Send + Sync + 'static,
    {
        self.filter_fn = Box::new(filter);
        self
    }
}

impl PriceSource for MockPriceSource {
    fn router_address(&self) -> Address {
        self.router
    }

    fn event_topics(&self) -> Vec<B256> {
        // Return a dummy topic for testing
        vec![B256::ZERO]
    }

    fn extract_swap_from_log(&self, _log: &Log) -> Result<Option<SwapData>, PriceSourceError> {
        // Cycle through swaps on each call
        let mut index = self.swap_index.lock().unwrap();
        if self.swaps.is_empty() {
            return Ok(None);
        }

        let swap = self.swaps[*index % self.swaps.len()].clone();
        *index += 1;
        Ok(Some(swap))
    }

    fn should_include_swap(&self, swap: &SwapData) -> bool {
        (self.filter_fn)(swap)
    }
}

/// Helper to create a minimal Log for testing
#[allow(dead_code)]
pub fn create_test_log(address: Address, topics: Vec<B256>, data: Vec<u8>) -> Log {
    Log {
        inner: alloy_primitives::Log {
            address,
            data: LogData::new(topics, data.into()).unwrap(),
        },
        block_hash: Some(B256::ZERO),
        block_number: Some(1000),
        block_timestamp: Some(1234567890),
        transaction_hash: Some(B256::ZERO),
        transaction_index: Some(0),
        log_index: Some(0),
        removed: false,
    }
}

/// Helper to create a test SwapData
#[allow(dead_code)]
pub fn create_test_swap(
    token_in: Address,
    token_in_amount: u64,
    token_out: Address,
    token_out_amount: u64,
    sender: Option<Address>,
) -> SwapData {
    SwapData {
        token_in,
        token_in_amount: U256::from(token_in_amount),
        token_out,
        token_out_amount: U256::from(token_out_amount),
        sender,
        tx_hash: None,
        block_number: None,
    }
}
