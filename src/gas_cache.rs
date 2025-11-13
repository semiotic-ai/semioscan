use std::cmp::{max, min};
use std::collections::HashMap;

use alloy_primitives::Address;

use crate::GasCostResult;

type CacheKey = (Address, Address, u64, u64); // (from, to, start_block, end_block)

/// Cache for storing gas cost calculation results to avoid redundant calculations
#[derive(Debug, Clone, Default)]
pub struct GasCache {
    cache: HashMap<CacheKey, GasCostResult>,
}

impl GasCache {
    /// Check if we have a result that fully contains the requested range
    pub fn get(
        &self,
        from: Address,
        to: Address,
        start_block: u64,
        end_block: u64,
    ) -> Option<GasCostResult> {
        // First check for exact match
        if let Some(result) = self.cache.get(&(from, to, start_block, end_block)) {
            return Some(result.clone());
        }

        // Check for cached results that fully cover our requested range
        for ((cached_from, cached_to, cached_start, cached_end), result) in &self.cache {
            if *cached_from == from
                && *cached_to == to
                && *cached_start <= start_block
                && *cached_end >= end_block
            {
                return Some(result.clone());
            }
        }

        None
    }

    /// Find all cached results that overlap with the requested range
    fn find_overlapping(
        &self,
        from: Address,
        to: Address,
        start_block: u64,
        end_block: u64,
    ) -> Vec<(CacheKey, &GasCostResult)> {
        let mut overlapping = Vec::new();

        for (key @ (cached_from, cached_to, cached_start, cached_end), result) in &self.cache {
            if *cached_from == from
                && *cached_to == to
                && !(*cached_end < start_block || *cached_start > end_block)
            {
                overlapping.push((*key, result));
            }
        }

        // Sort by start block to make merging easier
        overlapping.sort_by_key(|((_, _, start, _), _)| *start);

        overlapping
    }

    /// Insert a new result, potentially merging with existing results
    pub fn insert(
        &mut self,
        from: Address,
        to: Address,
        start_block: u64,
        end_block: u64,
        result: GasCostResult,
    ) {
        // Find overlapping results
        let overlapping = self.find_overlapping(from, to, start_block, end_block);

        if overlapping.is_empty() {
            // No overlap, simple insert
            self.cache
                .insert((from, to, start_block, end_block), result);
            return;
        }

        // There's overlap - we need to merge results
        let mut merged_result = result;
        let mut min_start = start_block;
        let mut max_end = end_block;

        // Collect keys to remove after merging
        let keys_to_remove: Vec<CacheKey> = overlapping.iter().map(|(key, _)| *key).collect();

        // Merge all overlapping results
        for ((_, _, cached_start, cached_end), cached_result) in overlapping {
            min_start = min(min_start, cached_start);
            max_end = max(max_end, cached_end);

            merged_result.merge(cached_result);
        }

        // Remove old entries
        for key in keys_to_remove {
            self.cache.remove(&key);
        }

        // Insert the merged result
        self.cache
            .insert((from, to, min_start, max_end), merged_result);
    }

    /// Calculate which block ranges need to be processed by finding gaps in the cached data
    ///
    /// Returns:
    /// - `Option<GasCostResult>`: Any cached data that overlaps with the requested range
    /// - `Vec<(u64, u64)>`: Gaps in the cached data that need to be processed
    pub fn calculate_gaps(
        &self,
        chain_id: u64,
        from: Address,
        to: Address,
        start_block: u64,
        end_block: u64,
    ) -> (Option<GasCostResult>, Vec<(u64, u64)>) {
        // First check for exact match or fully contained range
        if let Some(result) = self.get(from, to, start_block, end_block) {
            return (Some(result), vec![]);
        }

        // Find overlapping results
        let overlapping = self.find_overlapping(from, to, start_block, end_block);

        if overlapping.is_empty() {
            // No cached data, process the entire range
            return (None, vec![(start_block, end_block)]);
        }

        // Merge the overlapping results
        let mut merged_result = GasCostResult::new(chain_id, from, to);
        for (_, result) in &overlapping {
            merged_result.merge(result);
        }

        // Identify gaps by tracking covered ranges
        let mut covered_ranges: Vec<(u64, u64)> = overlapping
            .iter()
            .map(|((_, _, block_start, block_end), _)| (*block_start, *block_end))
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

    /// Clear all cached data for a specific signer
    pub fn clear_signer_data(&mut self, from: Address, to: Address) {
        self.cache
            .retain(|(cached_from, cached_to, _, _), _| *cached_from != from && *cached_to != to);
    }

    /// Clear all cached data for blocks below a certain height
    pub fn clear_old_blocks(&mut self, min_block: u64) {
        self.cache
            .retain(|(_, _, _, end_block), _| *end_block >= min_block);
    }

    /// Get the total number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};

    fn create_test_result(
        chain_id: u64,
        from: Address,
        to: Address,
        tx_count: usize,
        gas_cost: u64,
    ) -> GasCostResult {
        let mut result = GasCostResult::new(chain_id, from, to);
        result.transaction_count = tx_count;
        result.total_gas_cost = U256::from(gas_cost);
        result
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = GasCache::default();
        let from = Address::ZERO;
        let to = Address::ZERO;

        // Insert a range
        let result = create_test_result(1, from, to, 5, 100_000);
        cache.insert(from, to, 100, 200, result.clone());

        // Exact match
        let cached = cache.get(from, to, 100, 200);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().transaction_count, 5);

        // Smaller range (fully contained)
        let cached = cache.get(from, to, 120, 180);
        assert!(cached.is_some());

        // Larger range (not fully covered)
        let cached = cache.get(from, to, 50, 300);
        assert!(cached.is_none());
    }

    #[test]
    fn test_calculate_gaps() {
        let mut cache = GasCache::default();
        let from = Address::ZERO;
        let to = Address::ZERO;

        // Insert a few ranges with gaps
        cache.insert(
            from,
            to,
            100,
            200,
            create_test_result(1, from, to, 5, 100_000),
        );
        cache.insert(
            from,
            to,
            300,
            400,
            create_test_result(1, from, to, 3, 60_000),
        );
        cache.insert(
            from,
            to,
            600,
            700,
            create_test_result(1, from, to, 2, 40_000),
        );

        // Calculate gaps for a range that covers all cached ranges
        let (result, gaps) = cache.calculate_gaps(1, from, to, 50, 800);
        assert!(result.is_some());

        // Expected gaps: 50-99, 201-299, 401-599, 701-800
        assert_eq!(gaps.len(), 4);
        assert_eq!(gaps[0], (50, 99));
        assert_eq!(gaps[1], (201, 299));
        assert_eq!(gaps[2], (401, 599));
        assert_eq!(gaps[3], (701, 800));

        // Merged result should have 10 transactions
        assert_eq!(result.unwrap().transaction_count, 10);
    }

    #[test]
    fn test_overlap_merging() {
        let mut cache = GasCache::default();
        let from = Address::ZERO;
        let to = Address::ZERO;

        // Insert overlapping ranges
        cache.insert(
            from,
            to,
            100,
            300,
            create_test_result(1, from, to, 5, 100_000),
        );
        cache.insert(
            from,
            to,
            250,
            400,
            create_test_result(1, from, to, 3, 60_000),
        );

        // Should have merged the two entries
        assert_eq!(cache.len(), 1);

        // Get the merged range
        let cached = cache.get(from, to, 100, 400);
        assert!(cached.is_some());

        let result = cached.unwrap();
        assert_eq!(result.transaction_count, 8);
        assert_eq!(result.total_gas_cost, U256::from(160_000u64));
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Strategy for generating valid block ranges
        fn block_range_strategy() -> impl Strategy<Value = (u64, u64)> {
            (0u64..100_000u64)
                .prop_flat_map(|start| (Just(start), start..start.saturating_add(10_000)))
        }

        /// Strategy for generating multiple non-overlapping cached ranges
        fn cached_ranges_strategy() -> impl Strategy<Value = Vec<(u64, u64)>> {
            prop::collection::vec(block_range_strategy(), 0..10).prop_map(|mut ranges| {
                // Sort and make them non-overlapping
                ranges.sort_by_key(|(start, _)| *start);
                let mut non_overlapping = Vec::new();
                let mut last_end = 0u64;

                for (start, end) in ranges {
                    let adjusted_start = start.max(last_end + 2);
                    if adjusted_start < end {
                        non_overlapping.push((adjusted_start, end));
                        last_end = end;
                    }
                }

                non_overlapping
            })
        }

        proptest! {
            /// Property: Gaps should never overlap with cached ranges
            #[test]
            fn test_gaps_never_overlap_with_cached(
                cached_ranges in cached_ranges_strategy(),
                (query_start, query_end) in block_range_strategy()
            ) {
                let mut cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain_id, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain_id, from, to, query_start, query_end);

                // Verify no gap overlaps with any cached range
                for (gap_start, gap_end) in &gaps {
                    for (cached_start, cached_end) in &cached_ranges {
                        // Skip ranges outside the query window
                        if *cached_end < query_start || *cached_start > query_end {
                            continue;
                        }

                        // Check for no overlap: gap ends before cached starts OR gap starts after cached ends
                        let no_overlap = *gap_end < *cached_start || *gap_start > *cached_end;
                        prop_assert!(
                            no_overlap,
                            "Gap [{gap_start}, {gap_end}] overlaps with cached range [{cached_start}, {cached_end}]"
                        );
                    }
                }
            }

            /// Property: All gaps should be sorted by start block
            #[test]
            fn test_gaps_are_sorted(
                cached_ranges in cached_ranges_strategy(),
                (query_start, query_end) in block_range_strategy()
            ) {
                let mut cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain_id, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain_id, from, to, query_start, query_end);

                // Verify gaps are sorted
                for i in 1..gaps.len() {
                    prop_assert!(
                        gaps[i - 1].0 < gaps[i].0,
                        "Gaps not sorted: gap[{i_prev}] = {prev:?}, gap[{i}] = {curr:?}",
                        i_prev = i - 1,
                        prev = gaps[i - 1],
                        curr = gaps[i]
                    );
                }
            }

            /// Property: Gaps should cover entire uncached space within the query range
            #[test]
            fn test_gaps_cover_uncached_space(
                cached_ranges in cached_ranges_strategy(),
                (query_start, query_end) in block_range_strategy()
            ) {
                let mut cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain_id, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain_id, from, to, query_start, query_end);

                // Build a set of all blocks that are either cached or in gaps
                let mut covered_blocks = std::collections::HashSet::new();

                // Add cached blocks (within query range)
                for (cached_start, cached_end) in &cached_ranges {
                    let start = (*cached_start).max(query_start);
                    let end = (*cached_end).min(query_end);
                    if start <= end {
                        for block in start..=end {
                            covered_blocks.insert(block);
                        }
                    }
                }

                // Add gap blocks
                for (gap_start, gap_end) in &gaps {
                    for block in *gap_start..=*gap_end {
                        covered_blocks.insert(block);
                    }
                }

                // Verify all blocks in query range are covered
                for block in query_start..=query_end {
                    prop_assert!(
                        covered_blocks.contains(&block),
                        "Block {block} in range [{query_start}, {query_end}] is not covered by cache or gaps"
                    );
                }
            }

            /// Property: Gaps should not overlap with each other
            #[test]
            fn test_gaps_dont_overlap_each_other(
                cached_ranges in cached_ranges_strategy(),
                (query_start, query_end) in block_range_strategy()
            ) {
                let mut cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain_id, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain_id, from, to, query_start, query_end);

                // Verify no gap overlaps with another gap
                for i in 0..gaps.len() {
                    for j in (i + 1)..gaps.len() {
                        let (gap_i_start, gap_i_end) = gaps[i];
                        let (gap_j_start, gap_j_end) = gaps[j];

                        let no_overlap = gap_i_end < gap_j_start || gap_j_end < gap_i_start;
                        prop_assert!(
                            no_overlap,
                            "Gap {i} [{gap_i_start}, {gap_i_end}] overlaps with gap {j} [{gap_j_start}, {gap_j_end}]"
                        );
                    }
                }
            }

            /// Property: When cache is empty, should return entire query range as gap
            #[test]
            fn test_empty_cache_returns_full_range(
                (query_start, query_end) in block_range_strategy()
            ) {
                let cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                let (result, gaps) = cache.calculate_gaps(chain_id, from, to, query_start, query_end);

                prop_assert!(result.is_none(), "Empty cache should return None result");
                prop_assert_eq!(gaps.len(), 1, "Empty cache should return exactly one gap");
                prop_assert_eq!(gaps[0], (query_start, query_end), "Gap should cover entire query range");
            }

            /// Property: When query range is fully cached, should return no gaps
            #[test]
            fn test_fully_cached_returns_no_gaps(
                (inner_start, inner_end) in block_range_strategy()
            ) {
                let mut cache = GasCache::default();
                let from = Address::ZERO;
                let to = Address::ZERO;
                let chain_id = 1u64;

                // Cache a range that fully covers the query (add padding)
                let cache_start = inner_start.saturating_sub(10);
                let cache_end = inner_end.saturating_add(10);

                cache.insert(from, to, cache_start, cache_end, create_test_result(chain_id, from, to, 1, 1000));

                let (result, gaps) = cache.calculate_gaps(chain_id, from, to, inner_start, inner_end);

                prop_assert!(result.is_some(), "Fully cached range should return result");
                prop_assert_eq!(gaps.len(), 0, "Fully cached range should return no gaps");
            }
        }
    }
}
