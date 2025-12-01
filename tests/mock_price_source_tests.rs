// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Tests for MockPriceSource test helper
//!
//! Validates that the mock infrastructure works correctly for testing.
//!
//! Note: Full PriceCalculator integration tests require a Provider implementation
//! and are validated through examples/ that connect to real blockchain networks.

#![cfg(feature = "odos-example")]

mod helpers;

use alloy_primitives::address;
use helpers::{create_test_log, create_test_swap, MockPriceSource};
use semioscan::price::PriceSource;

#[test]
fn test_mock_price_source_returns_configured_swaps() {
    let router = address!("1111111111111111111111111111111111111111");
    let token = address!("2222222222222222222222222222222222222222");
    let usdc = address!("3333333333333333333333333333333333333333");

    let swap = create_test_swap(token, 1000, usdc, 2000, None);

    let mock = MockPriceSource::new(router).with_swaps(vec![swap.clone()]);

    // Create a dummy log
    let log = create_test_log(router, vec![], vec![]);

    // Extract swap from log should return our configured swap
    let result = mock.extract_swap_from_log(&log).unwrap();
    assert!(result.is_some());

    let extracted = result.unwrap();
    assert_eq!(extracted.token_in, token);
    assert_eq!(extracted.token_out, usdc);
}

#[test]
fn test_mock_price_source_filtering() {
    let router = address!("1111111111111111111111111111111111111111");
    let allowed_sender = address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let blocked_sender = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

    let token = address!("2222222222222222222222222222222222222222");
    let usdc = address!("3333333333333333333333333333333333333333");

    let allowed_swap = create_test_swap(token, 1000, usdc, 2000, Some(allowed_sender));
    let blocked_swap = create_test_swap(token, 1000, usdc, 2000, Some(blocked_sender));

    let mock =
        MockPriceSource::new(router).with_filter(move |swap| swap.sender == Some(allowed_sender));

    // Allowed swap should pass filter
    assert!(mock.should_include_swap(&allowed_swap));

    // Blocked swap should fail filter
    assert!(!mock.should_include_swap(&blocked_swap));
}

#[test]
fn test_mock_price_source_cycles_through_swaps() {
    let router = address!("1111111111111111111111111111111111111111");
    let token1 = address!("2222222222222222222222222222222222222222");
    let token2 = address!("3333333333333333333333333333333333333333");
    let usdc = address!("4444444444444444444444444444444444444444");

    let swap1 = create_test_swap(token1, 1000, usdc, 2000, None);
    let swap2 = create_test_swap(token2, 5000, usdc, 10000, None);

    let mock = MockPriceSource::new(router).with_swaps(vec![swap1.clone(), swap2.clone()]);

    let log = create_test_log(router, vec![], vec![]);

    // First call returns swap1
    let result1 = mock.extract_swap_from_log(&log).unwrap().unwrap();
    assert_eq!(result1.token_in, token1);

    // Second call returns swap2
    let result2 = mock.extract_swap_from_log(&log).unwrap().unwrap();
    assert_eq!(result2.token_in, token2);

    // Third call cycles back to swap1
    let result3 = mock.extract_swap_from_log(&log).unwrap().unwrap();
    assert_eq!(result3.token_in, token1);
}

#[test]
fn test_mock_price_source_empty_swaps() {
    let router = address!("1111111111111111111111111111111111111111");
    let mock = MockPriceSource::new(router);

    let log = create_test_log(router, vec![], vec![]);

    // Should return None when no swaps configured
    let result = mock.extract_swap_from_log(&log).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_mock_price_source_router_address() {
    let router = address!("1111111111111111111111111111111111111111");
    let mock = MockPriceSource::new(router);

    assert_eq!(mock.router_address(), router);
}

#[test]
fn test_mock_price_source_event_topics() {
    let router = address!("1111111111111111111111111111111111111111");
    let mock = MockPriceSource::new(router);

    let topics = mock.event_topics();
    assert!(!topics.is_empty(), "Should return at least one dummy topic");
}
