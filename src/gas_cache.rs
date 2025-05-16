use std::cmp::{max, min};
use std::collections::HashMap;

use alloy_primitives::Address;

use crate::GasCostResult;

type CacheKey = (Address, u64, u64); // (signer_address, start_block, end_block)

/// Cache for storing gas cost calculation results to avoid redundant calculations
#[derive(Debug, Clone, Default)]
pub struct GasCache {
    cache: HashMap<CacheKey, GasCostResult>,
}

impl GasCache {
    /// Check if we have a result that fully contains the requested range
    pub fn get(
        &self,
        signer_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> Option<GasCostResult> {
        // First check for exact match
        if let Some(result) = self.cache.get(&(signer_address, start_block, end_block)) {
            return Some(result.clone());
        }

        // Check for cached results that fully cover our requested range
        for ((cached_signer, cached_start, cached_end), result) in &self.cache {
            if *cached_signer == signer_address
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
        signer_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> Vec<(CacheKey, &GasCostResult)> {
        let mut overlapping = Vec::new();

        for (key @ (cached_signer, cached_start, cached_end), result) in &self.cache {
            if *cached_signer == signer_address
                && !(*cached_end < start_block || *cached_start > end_block)
            {
                overlapping.push((*key, result));
            }
        }

        // Sort by start block to make merging easier
        overlapping.sort_by_key(|((_, start, _), _)| *start);

        overlapping
    }

    /// Insert a new result, potentially merging with existing results
    pub fn insert(
        &mut self,
        signer_address: Address,
        start_block: u64,
        end_block: u64,
        result: GasCostResult,
    ) {
        // Find overlapping results
        let overlapping = self.find_overlapping(signer_address, start_block, end_block);

        if overlapping.is_empty() {
            // No overlap, simple insert
            self.cache
                .insert((signer_address, start_block, end_block), result);
            return;
        }

        // There's overlap - we need to merge results
        let mut merged_result = result;
        let mut min_start = start_block;
        let mut max_end = end_block;

        // Collect keys to remove after merging
        let keys_to_remove: Vec<CacheKey> = overlapping.iter().map(|(key, _)| *key).collect();

        // Merge all overlapping results
        for ((_, cached_start, cached_end), cached_result) in overlapping {
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
            .insert((signer_address, min_start, max_end), merged_result);
    }

    /// Calculate which block ranges need to be processed by finding gaps in the cached data
    ///
    /// Returns:
    /// - Option<GasCostResult>: Any cached data that overlaps with the requested range
    /// - Vec<(u64, u64)>: Gaps in the cached data that need to be processed
    pub fn calculate_gaps(
        &self,
        chain_id: u64,
        signer_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> (Option<GasCostResult>, Vec<(u64, u64)>) {
        // First check for exact match or fully contained range
        if let Some(result) = self.get(signer_address, start_block, end_block) {
            return (Some(result), vec![]);
        }

        // Find overlapping results
        let overlapping = self.find_overlapping(signer_address, start_block, end_block);

        if overlapping.is_empty() {
            // No cached data, process the entire range
            return (None, vec![(start_block, end_block)]);
        }

        // Merge the overlapping results
        let mut merged_result = GasCostResult::new(chain_id, signer_address);
        for (_, result) in &overlapping {
            merged_result.merge(result);
        }

        // Identify gaps by tracking covered ranges
        let mut covered_ranges: Vec<(u64, u64)> = overlapping
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

    /// Clear all cached data for a specific signer
    pub fn clear_signer_data(&mut self, signer_address: Address) {
        self.cache
            .retain(|(address, _, _), _| *address != signer_address);
    }

    /// Clear all cached data for blocks below a certain height
    pub fn clear_old_blocks(&mut self, min_block: u64) {
        self.cache
            .retain(|(_, _, end_block), _| *end_block >= min_block);
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
        signer: Address,
        tx_count: usize,
        gas_cost: u64,
    ) -> GasCostResult {
        let mut result = GasCostResult::new(chain_id, signer);
        result.transaction_count = tx_count;
        result.total_gas_cost = U256::from(gas_cost);
        result
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = GasCache::default();
        let addr = Address::ZERO;

        // Insert a range
        let result = create_test_result(1, addr, 5, 100_000);
        cache.insert(addr, 100, 200, result.clone());

        // Exact match
        let cached = cache.get(addr, 100, 200);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().transaction_count, 5);

        // Smaller range (fully contained)
        let cached = cache.get(addr, 120, 180);
        assert!(cached.is_some());

        // Larger range (not fully covered)
        let cached = cache.get(addr, 50, 300);
        assert!(cached.is_none());
    }

    #[test]
    fn test_calculate_gaps() {
        let mut cache = GasCache::default();
        let addr = Address::ZERO;

        // Insert a few ranges with gaps
        cache.insert(addr, 100, 200, create_test_result(1, addr, 5, 100_000));
        cache.insert(addr, 300, 400, create_test_result(1, addr, 3, 60_000));
        cache.insert(addr, 600, 700, create_test_result(1, addr, 2, 40_000));

        // Calculate gaps for a range that covers all cached ranges
        let (result, gaps) = cache.calculate_gaps(1, addr, 50, 800);
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
        let addr = Address::ZERO;

        // Insert overlapping ranges
        cache.insert(addr, 100, 300, create_test_result(1, addr, 5, 100_000));
        cache.insert(addr, 250, 400, create_test_result(1, addr, 3, 60_000));

        // Should have merged the two entries
        assert_eq!(cache.len(), 1);

        // Get the merged range
        let cached = cache.get(addr, 100, 400);
        assert!(cached.is_some());

        let result = cached.unwrap();
        assert_eq!(result.transaction_count, 8);
        assert_eq!(result.total_gas_cost, U256::from(160_000u64));
    }
}
