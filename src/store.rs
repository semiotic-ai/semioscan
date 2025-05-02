use std::cmp::{max, min};
use std::collections::HashMap;

use alloy_primitives::Address;

use crate::TokenPriceResult;

type CacheKey = (Address, u64, u64); // (token_address, start_block, end_block)

#[derive(Debug, Clone, Default)]
pub struct PriceCache {
    cache: HashMap<CacheKey, TokenPriceResult>,
}

impl PriceCache {
    // Check if we have a result that fully contains the requested range
    pub fn get(
        &self,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> Option<TokenPriceResult> {
        // First check for exact match
        if let Some(result) = self.cache.get(&(token_address, start_block, end_block)) {
            return Some(result.clone());
        }

        // Check for cached results that fully cover our requested range
        for ((cached_token, cached_start, cached_end), result) in &self.cache {
            if *cached_token == token_address
                && *cached_start <= start_block
                && *cached_end >= end_block
            {
                return Some(result.clone());
            }
        }

        None
    }

    // Find all cached results that overlap with the requested range
    fn find_overlapping(
        &self,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> Vec<(CacheKey, &TokenPriceResult)> {
        let mut overlapping = Vec::new();

        for (key @ (cached_token, cached_start, cached_end), result) in &self.cache {
            if *cached_token == token_address
                && !(*cached_end < start_block || *cached_start > end_block)
            {
                overlapping.push((*key, result));
            }
        }

        // Sort by start block to make merging easier
        overlapping.sort_by_key(|((_, start, _), _)| *start);

        overlapping
    }

    // Insert a new result, potentially merging with existing results
    pub fn insert(
        &mut self,
        token_address: Address,
        start_block: u64,
        end_block: u64,
        result: TokenPriceResult,
    ) {
        // Find overlapping results
        let overlapping = self.find_overlapping(token_address, start_block, end_block);

        if overlapping.is_empty() {
            // No overlap, simple insert
            self.cache
                .insert((token_address, start_block, end_block), result);
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
            .insert((token_address, min_start, max_end), merged_result);
    }

    // Calculate which block ranges need to be processed by finding gaps in the cached data
    pub fn calculate_gaps(
        &self,
        token_address: Address,
        start_block: u64,
        end_block: u64,
    ) -> (Option<TokenPriceResult>, Vec<(u64, u64)>) {
        // First check for exact match or fully contained range
        if let Some(result) = self.get(token_address, start_block, end_block) {
            return (Some(result), vec![]);
        }

        // Find overlapping results
        let overlapping = self.find_overlapping(token_address, start_block, end_block);

        if overlapping.is_empty() {
            // No cached data, process the entire range
            return (None, vec![(start_block, end_block)]);
        }

        // Merge the overlapping results
        let mut merged_result = TokenPriceResult::new(token_address);
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
            current = std::cmp::max(current, range_end + 1);
        }

        // Check if there's a gap after the last range
        if current <= end_block {
            gaps.push((current, end_block));
        }

        (Some(merged_result), gaps)
    }
}
