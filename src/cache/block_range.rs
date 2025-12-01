// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! Generic block-range cache with gap detection
//!
//! This module provides a generic caching mechanism for any data that is keyed by
//! block ranges. It supports automatic merging of overlapping ranges and gap detection
//! to identify uncached regions.

use std::cmp::{max, min};
use std::collections::HashMap;
use std::hash::Hash;

use alloy_primitives::BlockNumber;

/// Trait for values that can be merged when overlapping cache entries are combined
pub trait Mergeable {
    /// Merge another value into self
    fn merge(&mut self, other: &Self);
}

/// Generic cache for data associated with block ranges
///
/// This cache stores values keyed by `(K, start_block, end_block)` where `K` is a
/// domain-specific key (e.g., token address, address pair, etc.).
///
/// # Type Parameters
///
/// * `K` - The domain key type (must be `Clone + Eq + Hash`)
/// * `V` - The cached value type (must implement `Mergeable` and `Clone`)
///
/// # Features
///
/// - **Range queries**: Retrieve cached data that fully contains a requested range
/// - **Auto-merging**: Overlapping inserts are automatically merged
/// - **Gap detection**: Calculate precisely which blocks are not yet cached
#[derive(Debug, Clone, Default)]
pub struct BlockRangeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Mergeable + Clone,
{
    cache: HashMap<(K, BlockNumber, BlockNumber), V>,
}

impl<K, V> BlockRangeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Mergeable + Clone,
{
    /// Retrieve cached result that fully contains the requested range
    ///
    /// Returns a cached result if there exists an entry that completely covers
    /// the requested block range. Checks both exact matches and larger ranges
    /// that encompass the request.
    ///
    /// # Arguments
    ///
    /// * `key` - Domain-specific key
    /// * `start_block` - Start of requested range (inclusive)
    /// * `end_block` - End of requested range (inclusive)
    ///
    /// # Returns
    ///
    /// - `Some(result)`: Cached data that covers `[start_block, end_block]`
    /// - `None`: No cached entry fully contains this range
    pub fn get(&self, key: &K, start_block: BlockNumber, end_block: BlockNumber) -> Option<V> {
        // First check for exact match
        if let Some(result) = self.cache.get(&(key.clone(), start_block, end_block)) {
            return Some(result.clone());
        }

        // Check for cached results that fully cover our requested range
        for ((cached_key, cached_start, cached_end), result) in &self.cache {
            if cached_key == key && *cached_start <= start_block && *cached_end >= end_block {
                return Some(result.clone());
            }
        }

        None
    }

    /// Find all cached results that overlap with the requested range
    fn find_overlapping(
        &self,
        key: &K,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> Vec<((K, BlockNumber, BlockNumber), &V)> {
        let mut overlapping = Vec::new();

        for (cache_key @ (cached_key, cached_start, cached_end), result) in &self.cache {
            if cached_key == key && !(*cached_end < start_block || *cached_start > end_block) {
                overlapping.push((cache_key.clone(), result));
            }
        }

        // Sort by start block to make merging easier
        overlapping.sort_by_key(|((_, start, _), _)| *start);

        overlapping
    }

    /// Insert a result and automatically merge with overlapping entries
    ///
    /// When inserting a result that overlaps with existing cached data, this method:
    /// 1. Finds all overlapping entries
    /// 2. Merges their values using the `Mergeable` trait
    /// 3. Extends the block range to cover all overlapping entries
    /// 4. Removes the old entries and stores the merged result
    ///
    /// # Arguments
    ///
    /// * `key` - Domain-specific key
    /// * `start_block` - Start of block range (inclusive)
    /// * `end_block` - End of block range (inclusive)
    /// * `value` - Data for this range
    pub fn insert(&mut self, key: K, start_block: BlockNumber, end_block: BlockNumber, value: V) {
        // Find overlapping results
        let overlapping = self.find_overlapping(&key, start_block, end_block);

        if overlapping.is_empty() {
            // No overlap, simple insert
            self.cache.insert((key, start_block, end_block), value);
            return;
        }

        // There's overlap - we need to merge results
        let mut merged_value = value;
        let mut min_start = start_block;
        let mut max_end = end_block;

        // Collect keys to remove after merging
        let keys_to_remove: Vec<(K, BlockNumber, BlockNumber)> =
            overlapping.iter().map(|(k, _)| k.clone()).collect();

        // Merge all overlapping results
        for ((_, cached_start, cached_end), cached_value) in overlapping {
            min_start = min(min_start, cached_start);
            max_end = max(max_end, cached_end);

            merged_value.merge(cached_value);
        }

        // Remove old entries
        for cache_key in keys_to_remove {
            self.cache.remove(&cache_key);
        }

        // Insert the merged result
        self.cache.insert((key, min_start, max_end), merged_value);
    }

    /// Calculate uncached block ranges (gaps) and return merged cached data
    ///
    /// This is the key method for incremental scanning. It analyzes which portions of
    /// a requested block range are already cached and which need to be scanned.
    ///
    /// # Behavior
    ///
    /// 1. If the entire range is cached, returns `(Some(data), vec![])`
    /// 2. If nothing is cached, returns `(None, vec![(start, end)])`
    /// 3. If partially cached, returns merged cached data and a list of gaps
    ///
    /// # Arguments
    ///
    /// * `key` - Domain-specific key
    /// * `start_block` - Start of requested range (inclusive)
    /// * `end_block` - End of requested range (inclusive)
    /// * `create_empty` - Function to create an empty value for merging
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `Option<V>`: Merged data from all overlapping cached entries
    /// - `Vec<(u64, u64)>`: Sorted list of uncached ranges (gaps) to scan
    pub fn calculate_gaps<F>(
        &self,
        key: &K,
        start_block: BlockNumber,
        end_block: BlockNumber,
        create_empty: F,
    ) -> (Option<V>, Vec<(BlockNumber, BlockNumber)>)
    where
        F: FnOnce() -> V,
    {
        // First check for exact match or fully contained range
        if let Some(result) = self.get(key, start_block, end_block) {
            return (Some(result), vec![]);
        }

        // Find overlapping results
        let overlapping = self.find_overlapping(key, start_block, end_block);

        if overlapping.is_empty() {
            // No cached data, process the entire range
            return (None, vec![(start_block, end_block)]);
        }

        // Merge the overlapping results
        let mut merged_result = create_empty();
        for (_, result) in &overlapping {
            merged_result.merge(result);
        }

        // Identify gaps by tracking covered ranges
        let mut covered_ranges: Vec<(BlockNumber, BlockNumber)> = overlapping
            .iter()
            .map(|((_, block_start, block_end), _)| (*block_start, *block_end))
            .collect();

        // Sort by start block
        covered_ranges.sort_by_key(|(start, _)| *start);

        // Find gaps
        let mut gaps = vec![];
        let mut current = start_block;

        for (range_start, range_end) in covered_ranges {
            if current < range_start {
                // Found a gap
                gaps.push((current, range_start - 1));
            }
            // Move pointer past this range
            current = max(current, range_end + 1);
        }

        // Check if there's a gap after the last range
        if current <= end_block {
            gaps.push((current, end_block));
        }

        (Some(merged_result), gaps)
    }

    /// Get the total number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache contains no entries
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear all entries matching a predicate on the key
    pub fn retain<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&K, BlockNumber, BlockNumber) -> bool,
    {
        self.cache
            .retain(|(key, start, end), _| predicate(key, *start, *end));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test value that can be merged
    #[derive(Debug, Clone, PartialEq, Default)]
    struct TestValue {
        count: usize,
        total: u64,
    }

    impl TestValue {
        fn new(count: usize, total: u64) -> Self {
            Self { count, total }
        }
    }

    impl Mergeable for TestValue {
        fn merge(&mut self, other: &Self) {
            self.count += other.count;
            self.total += other.total;
        }
    }

    #[test]
    fn test_cache_empty_get_returns_none() {
        let cache: BlockRangeCache<String, TestValue> = BlockRangeCache::default();
        let key = "test".to_string();

        let result = cache.get(&key, 100, 200);
        assert!(result.is_none(), "Empty cache should return None");
    }

    #[test]
    fn test_cache_exact_match() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();
        let value = TestValue::new(5, 1000);

        cache.insert(key.clone(), 100, 200, value.clone());

        let result = cache.get(&key, 100, 200);
        assert!(result.is_some(), "Should find exact match");
        assert_eq!(result.unwrap(), value);
    }

    #[test]
    fn test_cache_fully_contained_range() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();
        let value = TestValue::new(5, 1000);

        // Cache blocks 50-250
        cache.insert(key.clone(), 50, 250, value.clone());

        // Request blocks 100-200 (fully contained)
        let result = cache.get(&key, 100, 200);
        assert!(result.is_some(), "Should find contained range");
        assert_eq!(result.unwrap(), value);
    }

    #[test]
    fn test_cache_partial_overlap_returns_none() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();

        // Cache blocks 100-200
        cache.insert(key.clone(), 100, 200, TestValue::new(5, 1000));

        // Request blocks 150-250 (partial overlap)
        let result = cache.get(&key, 150, 250);
        assert!(
            result.is_none(),
            "Partial overlap should return None from get()"
        );
    }

    #[test]
    fn test_insert_with_overlap_merges() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();

        // Insert first range: 100-200
        cache.insert(key.clone(), 100, 200, TestValue::new(5, 500));

        // Insert overlapping range: 150-250
        cache.insert(key.clone(), 150, 250, TestValue::new(3, 800));

        // Should be merged into single range: 100-250
        let result = cache.get(&key, 100, 250);
        assert!(result.is_some(), "Should find merged range");

        let merged = result.unwrap();
        assert_eq!(merged.count, 8); // 5 + 3
        assert_eq!(merged.total, 1300); // 500 + 800
    }

    #[test]
    fn test_calculate_gaps_empty_cache() {
        let cache: BlockRangeCache<String, TestValue> = BlockRangeCache::default();
        let key = "test".to_string();

        let (result, gaps) = cache.calculate_gaps(&key, 100, 200, || TestValue::new(0, 0));

        assert!(result.is_none(), "Empty cache should return None result");
        assert_eq!(gaps.len(), 1, "Should have one gap covering entire range");
        assert_eq!(gaps[0], (100, 200));
    }

    #[test]
    fn test_calculate_gaps_fully_cached() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();

        cache.insert(key.clone(), 50, 250, TestValue::new(10, 1000));

        let (result, gaps) = cache.calculate_gaps(&key, 100, 200, || TestValue::new(0, 0));

        assert!(result.is_some(), "Should return cached result");
        assert_eq!(gaps.len(), 0, "No gaps when fully cached");
    }

    #[test]
    fn test_calculate_gaps_middle_gap() {
        let mut cache = BlockRangeCache::default();
        let key = "test".to_string();

        // Cache blocks 100-150 and 200-250
        cache.insert(key.clone(), 100, 150, TestValue::new(5, 500));
        cache.insert(key.clone(), 200, 250, TestValue::new(8, 800));

        // Request blocks 100-250
        let (result, gaps) = cache.calculate_gaps(&key, 100, 250, || TestValue::new(0, 0));

        assert!(result.is_some(), "Should merge cached data");

        // Should have a gap in the middle
        assert_eq!(gaps.len(), 1, "Should have one gap in middle");
        assert_eq!(gaps[0], (151, 199), "Gap should be from 151 to 199");

        // Verify merged result has combined amounts
        let merged = result.unwrap();
        assert_eq!(merged.count, 13); // 5 + 8
        assert_eq!(merged.total, 1300); // 500 + 800
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut cache: BlockRangeCache<String, TestValue> = BlockRangeCache::default();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        cache.insert("test".to_string(), 100, 200, TestValue::new(1, 100));
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_retain() {
        let mut cache = BlockRangeCache::default();
        let key1 = "keep".to_string();
        let key2 = "remove".to_string();

        cache.insert(key1.clone(), 100, 200, TestValue::new(1, 100));
        cache.insert(key2.clone(), 300, 400, TestValue::new(2, 200));

        // Remove entries where key contains "remove"
        cache.retain(|key, _start, _end| !key.contains("remove"));

        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key1, 100, 200).is_some());
        assert!(cache.get(&key2, 300, 400).is_none());
    }
}
