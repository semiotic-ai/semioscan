//! Tests for PriceCache gap calculation, merging, and overlap handling
//!
//! This module tests the complex caching logic that enables efficient price
//! data retrieval by identifying which block ranges need to be queried from
//! the blockchain and which can be served from cache.

#![cfg(feature = "odos-example")]

use alloy_primitives::{address, Address};
use semioscan::{PriceCache, TokenPriceResult};

/// Helper to create a test price result with specified amounts
fn create_price_result(token: Address, token_amount: f64, usdc_amount: f64) -> TokenPriceResult {
    let mut result = TokenPriceResult::new(token);
    result.merge(&TokenPriceResult {
        token_address: token,
        total_token_amount: token_amount,
        total_usdc_amount: usdc_amount,
        transaction_count: 1,
    });
    result
}

#[test]
fn test_cache_empty_get_returns_none() {
    let cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    let result = cache.get(token, 100, 200);
    assert!(result.is_none(), "Empty cache should return None");
}

#[test]
fn test_cache_exact_match() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let expected = create_price_result(token, 1000.0, 500.0);

    cache.insert(token, 100, 200, expected.clone());

    let result = cache.get(token, 100, 200);
    assert!(result.is_some(), "Should find exact match");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.total_token_amount, expected.total_token_amount);
    assert_eq!(retrieved.total_usdc_amount, expected.total_usdc_amount);
}

#[test]
fn test_cache_fully_contained_range() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let expected = create_price_result(token, 1000.0, 500.0);

    // Cache blocks 50-250
    cache.insert(token, 50, 250, expected.clone());

    // Request blocks 100-200 (fully contained)
    let result = cache.get(token, 100, 200);
    assert!(result.is_some(), "Should find contained range");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.total_token_amount, expected.total_token_amount);
    assert_eq!(retrieved.total_usdc_amount, expected.total_usdc_amount);
}

#[test]
fn test_cache_partial_overlap_returns_none() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let cached = create_price_result(token, 1000.0, 500.0);

    // Cache blocks 100-200
    cache.insert(token, 100, 200, cached);

    // Request blocks 150-250 (partial overlap)
    let result = cache.get(token, 150, 250);
    assert!(
        result.is_none(),
        "Partial overlap should return None from get()"
    );
}

#[test]
fn test_cache_different_token_returns_none() {
    let mut cache = PriceCache::default();
    let token1 = address!("0000000000000000000000000000000000000001");
    let token2 = address!("0000000000000000000000000000000000000002");
    let cached = create_price_result(token1, 1000.0, 500.0);

    cache.insert(token1, 100, 200, cached);

    let result = cache.get(token2, 100, 200);
    assert!(result.is_none(), "Different token should return None");
}

#[test]
fn test_calculate_gaps_empty_cache() {
    let cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    let (result, gaps) = cache.calculate_gaps(token, 100, 200);

    assert!(result.is_none(), "Empty cache should return None result");
    assert_eq!(gaps.len(), 1, "Should have one gap covering entire range");
    assert_eq!(gaps[0], (100, 200));
}

#[test]
fn test_calculate_gaps_fully_cached() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let expected = create_price_result(token, 1000.0, 500.0);

    cache.insert(token, 50, 250, expected.clone());

    let (result, gaps) = cache.calculate_gaps(token, 100, 200);

    assert!(result.is_some(), "Should return cached result");
    let retrieved = result.unwrap();
    assert_eq!(retrieved.total_token_amount, expected.total_token_amount);
    assert_eq!(retrieved.total_usdc_amount, expected.total_usdc_amount);
    assert_eq!(gaps.len(), 0, "No gaps when fully cached");
}

#[test]
fn test_calculate_gaps_single_gap_at_start() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let cached = create_price_result(token, 1000.0, 500.0);

    // Cache blocks 150-250
    cache.insert(token, 150, 250, cached);

    // Request blocks 100-250
    let (result, gaps) = cache.calculate_gaps(token, 100, 250);

    assert!(result.is_some(), "Should merge cached data");
    assert_eq!(gaps.len(), 1, "Should have gap at start");
    assert_eq!(gaps[0], (100, 149), "Gap should be from 100 to 149");
}

#[test]
fn test_calculate_gaps_single_gap_at_end() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");
    let cached = create_price_result(token, 1000.0, 500.0);

    // Cache blocks 100-150
    cache.insert(token, 100, 150, cached);

    // Request blocks 100-200
    let (result, gaps) = cache.calculate_gaps(token, 100, 200);

    assert!(result.is_some(), "Should merge cached data");
    assert_eq!(gaps.len(), 1, "Should have gap at end");
    assert_eq!(gaps[0], (151, 200), "Gap should be from 151 to 200");
}

#[test]
fn test_calculate_gaps_middle_gap() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Cache blocks 100-150 and 200-250
    cache.insert(token, 100, 150, create_price_result(token, 500.0, 250.0));
    cache.insert(token, 200, 250, create_price_result(token, 800.0, 400.0));

    // Request blocks 100-250
    let (result, gaps) = cache.calculate_gaps(token, 100, 250);

    assert!(result.is_some(), "Should merge cached data");

    // Should have a gap in the middle
    assert_eq!(gaps.len(), 1, "Should have one gap in middle");
    assert_eq!(gaps[0], (151, 199), "Gap should be from 151 to 199");

    // Verify merged result has combined amounts
    let merged = result.unwrap();
    assert_eq!(merged.total_token_amount, 1300.0); // 500 + 800
    assert_eq!(merged.total_usdc_amount, 650.0); // 250 + 400
}

#[test]
fn test_calculate_gaps_multiple_gaps() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Cache blocks: [100-150], [200-250], [300-350]
    cache.insert(token, 100, 150, create_price_result(token, 100.0, 50.0));
    cache.insert(token, 200, 250, create_price_result(token, 200.0, 100.0));
    cache.insert(token, 300, 350, create_price_result(token, 300.0, 150.0));

    // Request blocks 100-350
    let (result, gaps) = cache.calculate_gaps(token, 100, 350);

    assert!(result.is_some(), "Should merge all cached data");

    // Should have two gaps: 151-199 and 251-299
    assert_eq!(gaps.len(), 2, "Should have two gaps");
    assert_eq!(gaps[0], (151, 199));
    assert_eq!(gaps[1], (251, 299));

    // Verify merged result
    let merged = result.unwrap();
    assert_eq!(merged.total_token_amount, 600.0); // 100+200+300
    assert_eq!(merged.total_usdc_amount, 300.0); // 50+100+150
}

#[test]
fn test_calculate_gaps_with_surrounding_gaps() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Cache blocks 200-300
    cache.insert(token, 200, 300, create_price_result(token, 1000.0, 500.0));

    // Request blocks 100-400 (gaps before and after cached range)
    let (result, gaps) = cache.calculate_gaps(token, 100, 400);

    assert!(result.is_some(), "Should include cached data");
    assert_eq!(gaps.len(), 2, "Should have gaps at start and end");
    assert_eq!(gaps[0], (100, 199), "Gap before cached range");
    assert_eq!(gaps[1], (301, 400), "Gap after cached range");
}

#[test]
fn test_insert_with_no_overlap() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    cache.insert(token, 100, 200, create_price_result(token, 100.0, 50.0));
    cache.insert(token, 300, 400, create_price_result(token, 200.0, 100.0));

    // Both ranges should be cached separately
    let result1 = cache.get(token, 100, 200);
    let result2 = cache.get(token, 300, 400);

    assert!(result1.is_some(), "First range should be cached");
    assert!(result2.is_some(), "Second range should be cached");
    assert_eq!(result1.unwrap().total_token_amount, 100.0);
    assert_eq!(result2.unwrap().total_token_amount, 200.0);
}

#[test]
fn test_insert_with_overlap_merges() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Insert first range: 100-200
    cache.insert(token, 100, 200, create_price_result(token, 500.0, 250.0));

    // Insert overlapping range: 150-250
    cache.insert(token, 150, 250, create_price_result(token, 800.0, 400.0));

    // Should be merged into single range: 100-250
    let result = cache.get(token, 100, 250);
    assert!(result.is_some(), "Should find merged range");

    let merged = result.unwrap();
    assert_eq!(merged.total_token_amount, 1300.0); // 500 + 800
    assert_eq!(merged.total_usdc_amount, 650.0); // 250 + 400

    // Original individual ranges should not be separately cached
    let (_, gaps) = cache.calculate_gaps(token, 100, 250);
    assert_eq!(gaps.len(), 0, "No gaps in merged range");
}

#[test]
fn test_insert_adjacent_ranges_no_merge() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Insert adjacent ranges: 100-200 and 201-300 (no overlap, but contiguous)
    cache.insert(token, 100, 200, create_price_result(token, 100.0, 50.0));
    cache.insert(token, 201, 300, create_price_result(token, 200.0, 100.0));

    // Adjacent ranges don't overlap, so they won't be merged by get()
    let result = cache.get(token, 100, 300);
    assert!(
        result.is_none(),
        "Adjacent ranges (no overlap) are not merged by get()"
    );

    // calculate_gaps should merge the results and find no gaps (ranges are contiguous)
    let (merged_result, gaps) = cache.calculate_gaps(token, 100, 300);
    assert!(
        merged_result.is_some(),
        "calculate_gaps should merge adjacent ranges"
    );
    assert_eq!(gaps.len(), 0, "No gaps - ranges are contiguous");

    // Verify the merged result has combined amounts
    let merged = merged_result.unwrap();
    assert_eq!(merged.total_token_amount, 300.0); // 100 + 200
    assert_eq!(merged.total_usdc_amount, 150.0); // 50 + 100
}

#[test]
fn test_insert_multiple_overlaps_merges_all() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Insert three separate ranges
    cache.insert(token, 100, 150, create_price_result(token, 100.0, 50.0));
    cache.insert(token, 200, 250, create_price_result(token, 200.0, 100.0));
    cache.insert(token, 300, 350, create_price_result(token, 300.0, 150.0));

    // Insert a range that overlaps all three: 140-340
    cache.insert(token, 140, 340, create_price_result(token, 500.0, 250.0));

    // Should merge everything into 100-350
    let result = cache.get(token, 100, 350);
    assert!(result.is_some(), "All ranges should be merged");

    let merged = result.unwrap();
    // Total: 100 + 200 + 300 + 500 = 1100
    assert_eq!(merged.total_token_amount, 1100.0);
    // Total: 50 + 100 + 150 + 250 = 550
    assert_eq!(merged.total_usdc_amount, 550.0);

    // No gaps in the merged range
    let (_, gaps) = cache.calculate_gaps(token, 100, 350);
    assert_eq!(gaps.len(), 0, "No gaps after merging all overlaps");
}

#[test]
fn test_edge_case_zero_length_range() {
    let cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Request zero-length range (same start and end)
    let (result, gaps) = cache.calculate_gaps(token, 100, 100);

    assert!(result.is_none(), "Empty cache returns None");
    assert_eq!(gaps.len(), 1, "Should have one gap");
    assert_eq!(gaps[0], (100, 100), "Gap covers the single block");
}

#[test]
fn test_edge_case_large_block_numbers() {
    let mut cache = PriceCache::default();
    let token = address!("0000000000000000000000000000000000000001");

    // Use large block numbers (realistic for Arbitrum, etc.)
    let large_block = 250_000_000_u64;
    cache.insert(
        token,
        large_block,
        large_block + 1000,
        create_price_result(token, 1000.0, 500.0),
    );

    let result = cache.get(token, large_block, large_block + 1000);
    assert!(result.is_some(), "Should handle large block numbers");
}

#[test]
fn test_multiple_tokens_isolated() {
    let mut cache = PriceCache::default();
    let token1 = address!("0000000000000000000000000000000000000001");
    let token2 = address!("0000000000000000000000000000000000000002");

    cache.insert(token1, 100, 200, create_price_result(token1, 100.0, 50.0));
    cache.insert(token2, 100, 200, create_price_result(token2, 200.0, 100.0));

    let result1 = cache.get(token1, 100, 200);
    let result2 = cache.get(token2, 100, 200);

    assert!(result1.is_some(), "Token 1 should be cached");
    assert!(result2.is_some(), "Token 2 should be cached");

    // Verify they have different values
    assert_eq!(result1.unwrap().total_token_amount, 100.0);
    assert_eq!(result2.unwrap().total_token_amount, 200.0);
}
