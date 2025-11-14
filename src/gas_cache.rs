//! In-memory cache for gas cost calculations with gap detection
//!
//! This module provides intelligent caching for gas cost calculations that supports:
//! - Automatic merging of overlapping block ranges
//! - Gap detection to identify uncached regions
//! - Cache invalidation by address or block height
//!
//! # Use Cases
//!
//! - **Avoid redundant RPC calls**: Cache gas calculations to prevent re-scanning the same blocks
//! - **Incremental updates**: Add new block ranges and automatically merge with existing data
//! - **Gap filling**: Identify precisely which block ranges still need to be scanned
//!
//! # Example: Basic caching
//!
//! ```rust
//! use semioscan::{GasCache, GasCostResult};
//! use alloy_chains::NamedChain;
//! use alloy_primitives::{Address, U256};
//!
//! let mut cache = GasCache::default();
//! let from = Address::ZERO;
//! let to = Address::ZERO;
//!
//! // Insert a result for blocks 100-200
//! let mut result = GasCostResult::new(NamedChain::Mainnet, from, to);
//! result.total_gas_cost = U256::from(1_000_000u64);
//! cache.insert(from, to, 100, 200, result);
//!
//! // Retrieve it
//! let cached = cache.get(from, to, 100, 200);
//! assert!(cached.is_some());
//! ```
//!
//! # Example: Gap detection
//!
//! ```rust
//! use semioscan::{GasCache, GasCostResult};
//! use alloy_chains::NamedChain;
//! use alloy_primitives::{Address, U256};
//!
//! let mut cache = GasCache::default();
//! let from = Address::ZERO;
//! let to = Address::ZERO;
//!
//! // Cache blocks 100-200 and 300-400
//! cache.insert(from, to, 100, 200, GasCostResult::new(NamedChain::Mainnet, from, to));
//! cache.insert(from, to, 300, 400, GasCostResult::new(NamedChain::Mainnet, from, to));
//!
//! // Find gaps in range 50-500
//! let (cached, gaps) = cache.calculate_gaps(NamedChain::Mainnet, from, to, 50, 500);
//!
//! // Gaps: [50, 99], [201, 299], [401, 500]
//! assert_eq!(gaps.len(), 3);
//! assert_eq!(gaps[0], (50, 99));
//! assert_eq!(gaps[1], (201, 299));
//! assert_eq!(gaps[2], (401, 500));
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::{Address, BlockNumber};

use crate::block_range_cache::{BlockRangeCache, Mergeable};
use crate::GasCostResult;

// Implement Mergeable for GasCostResult
impl Mergeable for GasCostResult {
    fn merge(&mut self, other: &Self) {
        self.total_gas_cost = self.total_gas_cost.saturating_add(other.total_gas_cost);
        self.transaction_count += other.transaction_count;
    }
}

/// In-memory cache for gas cost calculation results
///
/// Stores gas cost data keyed by `(from, to, start_block, end_block)` and provides
/// intelligent features like automatic range merging and gap detection.
///
/// # Features
///
/// - **Range queries**: Retrieve cached data that fully contains a requested range
/// - **Auto-merging**: Overlapping inserts are automatically merged
/// - **Gap detection**: Calculate precisely which blocks are not yet cached
/// - **Cache management**: Clear by address or block height
///
/// # Example
///
/// ```rust
/// use semioscan::{GasCache, GasCostResult};
/// use alloy_chains::NamedChain;
/// use alloy_primitives::Address;
///
/// let mut cache = GasCache::default();
/// let from = Address::ZERO;
/// let to = Address::ZERO;
///
/// // Insert results for different block ranges
/// cache.insert(from, to, 100, 200, GasCostResult::new(NamedChain::Mainnet, from, to));
/// cache.insert(from, to, 150, 250, GasCostResult::new(NamedChain::Mainnet, from, to));
///
/// // Overlapping ranges are merged automatically
/// assert_eq!(cache.len(), 1);
/// ```
#[derive(Debug, Clone, Default)]
pub struct GasCache {
    inner: BlockRangeCache<(Address, Address), GasCostResult>,
}

impl GasCache {
    /// Retrieve cached result that fully contains the requested range
    ///
    /// Returns a cached result if there exists an entry that completely covers
    /// the requested block range. Checks both exact matches and larger ranges
    /// that encompass the request.
    ///
    /// # Arguments
    ///
    /// * `from` - Source address
    /// * `to` - Destination address
    /// * `start_block` - Start of requested range (inclusive)
    /// * `end_block` - End of requested range (inclusive)
    ///
    /// # Returns
    ///
    /// - `Some(result)`: Cached data that covers `[start_block, end_block]`
    /// - `None`: No cached entry fully contains this range
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::Address;
    ///
    /// let mut cache = GasCache::default();
    /// let from = Address::ZERO;
    /// let to = Address::ZERO;
    ///
    /// // Cache blocks 100-300
    /// cache.insert(from, to, 100, 300, GasCostResult::new(NamedChain::Mainnet, from, to));
    ///
    /// // Query for subset [150, 250] - returns cached data
    /// assert!(cache.get(from, to, 150, 250).is_some());
    ///
    /// // Query for [50, 350] - returns None (not fully covered)
    /// assert!(cache.get(from, to, 50, 350).is_none());
    /// ```
    pub fn get(
        &self,
        from: Address,
        to: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> Option<GasCostResult> {
        self.inner.get(&(from, to), start_block, end_block)
    }

    /// Insert a result and automatically merge with overlapping entries
    ///
    /// When inserting a result that overlaps with existing cached data, this method:
    /// 1. Finds all overlapping entries
    /// 2. Merges their gas costs and transaction counts
    /// 3. Extends the block range to cover all overlapping entries
    /// 4. Removes the old entries and stores the merged result
    ///
    /// # Arguments
    ///
    /// * `from` - Source address
    /// * `to` - Destination address
    /// * `start_block` - Start of block range (inclusive)
    /// * `end_block` - End of block range (inclusive)
    /// * `result` - Gas cost data for this range
    ///
    /// # Example: Auto-merging
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::{Address, U256};
    ///
    /// let mut cache = GasCache::default();
    /// let from = Address::ZERO;
    /// let to = Address::ZERO;
    ///
    /// // Insert blocks 100-200 with 5 transactions
    /// let mut result1 = GasCostResult::new(NamedChain::Mainnet, from, to);
    /// result1.transaction_count = 5;
    /// cache.insert(from, to, 100, 200, result1);
    ///
    /// // Insert overlapping blocks 150-250 with 3 transactions
    /// let mut result2 = GasCostResult::new(NamedChain::Mainnet, from, to);
    /// result2.transaction_count = 3;
    /// cache.insert(from, to, 150, 250, result2);
    ///
    /// // Results are merged: 1 entry covering 100-250 with 8 transactions
    /// assert_eq!(cache.len(), 1);
    /// let merged = cache.get(from, to, 100, 250).unwrap();
    /// assert_eq!(merged.transaction_count, 8);
    /// ```
    pub fn insert(
        &mut self,
        from: Address,
        to: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
        result: GasCostResult,
    ) {
        self.inner
            .insert((from, to), start_block, end_block, result);
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
    /// * `chain` - Chain (used when creating merged result)
    /// * `from` - Source address
    /// * `to` - Destination address
    /// * `start_block` - Start of requested range (inclusive)
    /// * `end_block` - End of requested range (inclusive)
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `Option<GasCostResult>`: Merged data from all overlapping cached entries
    /// - `Vec<(BlockNumber, BlockNumber)>`: Sorted list of uncached ranges (gaps) to scan
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::Address;
    ///
    /// let mut cache = GasCache::default();
    /// let from = Address::ZERO;
    /// let to = Address::ZERO;
    ///
    /// // Cache two ranges with a gap
    /// cache.insert(from, to, 100, 200, GasCostResult::new(NamedChain::Mainnet, from, to));
    /// cache.insert(from, to, 300, 400, GasCostResult::new(NamedChain::Mainnet, from, to));
    ///
    /// // Request range [50, 500]
    /// let (cached, gaps) = cache.calculate_gaps(NamedChain::Mainnet, from, to, 50, 500);
    ///
    /// // We get cached data and three gaps to fill
    /// assert!(cached.is_some());
    /// assert_eq!(gaps, vec![
    ///     (50, 99),    // Before first cached range
    ///     (201, 299),  // Between cached ranges
    ///     (401, 500),  // After last cached range
    /// ]);
    /// ```
    pub fn calculate_gaps(
        &self,
        chain: NamedChain,
        from: Address,
        to: Address,
        start_block: BlockNumber,
        end_block: BlockNumber,
    ) -> (Option<GasCostResult>, Vec<(BlockNumber, BlockNumber)>) {
        self.inner
            .calculate_gaps(&(from, to), start_block, end_block, || {
                GasCostResult::new(chain, from, to)
            })
    }

    /// Clear all cached data for a specific address pair
    ///
    /// Removes all entries where transactions were sent from `from` to `to`.
    /// Useful when you want to invalidate cached data for a specific route.
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::{Address, address};
    ///
    /// let mut cache = GasCache::default();
    /// let addr1 = address!("0x1111111111111111111111111111111111111111");
    /// let addr2 = address!("0x2222222222222222222222222222222222222222");
    ///
    /// cache.insert(addr1, addr2, 100, 200, GasCostResult::new(NamedChain::Mainnet, addr1, addr2));
    /// assert_eq!(cache.len(), 1);
    ///
    /// cache.clear_signer_data(addr1, addr2);
    /// assert_eq!(cache.len(), 0);
    /// ```
    pub fn clear_signer_data(&mut self, from: Address, to: Address) {
        self.inner
            .retain(|(cached_from, cached_to), _, _| *cached_from != from || *cached_to != to);
    }

    /// Clear all cached entries that end before a minimum block height
    ///
    /// Useful for invalidating old data when you know earlier blocks
    /// are no longer relevant (e.g., after a blockchain reorganization).
    ///
    /// # Arguments
    ///
    /// * `min_block` - Minimum block height to keep (entries ending before this are removed)
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::Address;
    ///
    /// let mut cache = GasCache::default();
    /// let from = Address::ZERO;
    /// let to = Address::ZERO;
    ///
    /// cache.insert(from, to, 100, 200, GasCostResult::new(NamedChain::Mainnet, from, to));
    /// cache.insert(from, to, 500, 600, GasCostResult::new(NamedChain::Mainnet, from, to));
    /// assert_eq!(cache.len(), 2);
    ///
    /// // Clear entries ending before block 300
    /// cache.clear_old_blocks(300);
    /// assert_eq!(cache.len(), 1); // Only [500, 600] remains
    /// ```
    pub fn clear_old_blocks(&mut self, min_block: BlockNumber) {
        self.inner.retain(|_, _, end_block| end_block >= min_block);
    }

    /// Get the total number of cached entries
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_chains::NamedChain;
    /// use alloy_primitives::Address;
    ///
    /// let mut cache = GasCache::default();
    /// assert_eq!(cache.len(), 0);
    ///
    /// cache.insert(Address::ZERO, Address::ZERO, 100, 200, GasCostResult::new(NamedChain::Mainnet, Address::ZERO, Address::ZERO));
    /// assert_eq!(cache.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if the cache contains no entries
    ///
    /// # Example
    ///
    /// ```rust
    /// use semioscan::{GasCache, GasCostResult};
    /// use alloy_primitives::Address;
    ///
    /// let cache = GasCache::default();
    /// assert!(cache.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use alloy_primitives::{Address, U256};

    fn create_test_result(
        chain: NamedChain,
        from: Address,
        to: Address,
        tx_count: usize,
        gas_cost: u64,
    ) -> GasCostResult {
        let mut result = GasCostResult::new(chain, from, to);
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
        let result = create_test_result(NamedChain::Mainnet, from, to, 5, 100_000);
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
            create_test_result(NamedChain::Mainnet, from, to, 5, 100_000),
        );
        cache.insert(
            from,
            to,
            300,
            400,
            create_test_result(NamedChain::Mainnet, from, to, 3, 60_000),
        );
        cache.insert(
            from,
            to,
            600,
            700,
            create_test_result(NamedChain::Mainnet, from, to, 2, 40_000),
        );

        // Calculate gaps for a range that covers all cached ranges
        let (result, gaps) = cache.calculate_gaps(NamedChain::Mainnet, from, to, 50, 800);
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
            create_test_result(NamedChain::Mainnet, from, to, 5, 100_000),
        );
        cache.insert(
            from,
            to,
            250,
            400,
            create_test_result(NamedChain::Mainnet, from, to, 3, 60_000),
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
        fn block_range_strategy() -> impl Strategy<Value = (BlockNumber, BlockNumber)> {
            (0u64..100_000u64)
                .prop_flat_map(|start| (Just(start), start..start.saturating_add(10_000)))
        }

        /// Strategy for generating multiple non-overlapping cached ranges
        fn cached_ranges_strategy() -> impl Strategy<Value = Vec<(BlockNumber, BlockNumber)>> {
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
                let chain = NamedChain::Mainnet;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain, from, to, query_start, query_end);

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
                let chain = NamedChain::Mainnet;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain, from, to, query_start, query_end);

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
                let chain = NamedChain::Mainnet;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain, from, to, query_start, query_end);

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
                let chain = NamedChain::Mainnet;

                // Insert cached ranges
                for (start, end) in &cached_ranges {
                    cache.insert(from, to, *start, *end, create_test_result(chain, from, to, 1, 1000));
                }

                // Calculate gaps
                let (_, gaps) = cache.calculate_gaps(chain, from, to, query_start, query_end);

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
                let chain = NamedChain::Mainnet;

                let (result, gaps) = cache.calculate_gaps(chain, from, to, query_start, query_end);

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
                let chain = NamedChain::Mainnet;

                // Cache a range that fully covers the query (add padding)
                let cache_start = inner_start.saturating_sub(10);
                let cache_end = inner_end.saturating_add(10);

                cache.insert(from, to, cache_start, cache_end, create_test_result(chain, from, to, 1, 1000));

                let (result, gaps) = cache.calculate_gaps(chain, from, to, inner_start, inner_end);

                prop_assert!(result.is_some(), "Fully cached range should return result");
                prop_assert_eq!(gaps.len(), 0, "Fully cached range should return no gaps");
            }
        }
    }
}
