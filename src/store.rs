// use std::cmp::{max, min};
// use std::collections::HashMap;

// use alloy_primitives::Address;

// use crate::TokenPriceResult;

// type CacheKey = (Address, u64, u64); // (token_address, start_timestamp, end_timestamp)

// struct PriceCache {
//     cache: HashMap<CacheKey, TokenPriceResult>,
// }

// impl PriceCache {
//     fn new() -> Self {
//         Self {
//             cache: HashMap::new(),
//         }
//     }

//     // Check if we have a result that fully contains the requested range
//     fn get(&self, token_address: Address, start: u64, end: u64) -> Option<&TokenPriceResult> {
//         // First check for exact match
//         if let Some(result) = self.cache.get(&(token_address, start, end)) {
//             return Some(result);
//         }

//         // Check for cached results that fully cover our requested range
//         for ((cached_token, cached_start, cached_end), result) in &self.cache {
//             if *cached_token == token_address && *cached_start <= start && *cached_end >= end {
//                 return Some(result);
//             }
//         }

//         None
//     }

//     // Find all cached results that overlap with the requested range
//     fn find_overlapping(
//         &self,
//         token_address: Address,
//         start: u64,
//         end: u64,
//     ) -> Vec<(CacheKey, &TokenPriceResult)> {
//         let mut overlapping = Vec::new();

//         for (key @ (cached_token, cached_start, cached_end), result) in &self.cache {
//             if *cached_token == token_address && !(*cached_end < start || *cached_start > end) {
//                 overlapping.push((*key, result));
//             }
//         }

//         // Sort by start time to make merging easier
//         overlapping.sort_by_key(|((_, start, _), _)| *start);

//         overlapping
//     }

//     // Insert a new result, potentially merging with existing results
//     fn insert(&mut self, token_address: Address, start: u64, end: u64, result: TokenPriceResult) {
//         // Find overlapping results
//         let overlapping = self.find_overlapping(token_address, start, end);

//         if overlapping.is_empty() {
//             // No overlap, simple insert
//             self.cache.insert((token_address, start, end), result);
//             return;
//         }

//         // There's overlap - we need to merge results
//         let mut merged_result = result;
//         let mut min_start = start;
//         let mut max_end = end;

//         // Collect keys to remove after merging
//         let keys_to_remove: Vec<CacheKey> = overlapping.iter().map(|(key, _)| *key).collect();

//         // Merge all overlapping results
//         for ((_, cached_start, cached_end), cached_result) in overlapping {
//             min_start = min(min_start, cached_start);
//             max_end = max(max_end, cached_end);

//             // Merge the data from cached result
//             *merged_result.total_token_amount_mut() += cached_result.total_token_amount();
//             *merged_result.total_usdc_amount_mut() += cached_result.total_usdc_amount();
//             *merged_result.transaction_count_mut() += cached_result.transaction_count();
//             *merged_result.first_timestamp_mut() = min(
//                 merged_result.first_timestamp(),
//                 cached_result.first_timestamp(),
//             );
//             *merged_result.last_timestamp_mut() = max(
//                 merged_result.last_timestamp(),
//                 cached_result.last_timestamp(),
//             );
//         }

//         // Remove old entries
//         for key in keys_to_remove {
//             self.cache.remove(&key);
//         }

//         // Insert the merged result
//         self.cache
//             .insert((token_address, min_start, max_end), merged_result);
//     }
// }
