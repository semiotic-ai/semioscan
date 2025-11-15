//! Block window calculation for mapping UTC dates to blockchain block ranges
//!
//! This module provides tools for calculating which blockchain blocks correspond to
//! a specific UTC date. This is useful for analyzing blockchain data by date rather
//! than by block number.
//!
//! # Caching
//!
//! Block windows are automatically cached to disk to avoid repeated RPC calls for
//! the same date. The cache is stored as JSON and persists across program runs.
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::BlockWindowCalculator;
//! use alloy_provider::ProviderBuilder;
//! use alloy_chains::NamedChain;
//! use chrono::NaiveDate;
//!
//! let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
//! let calculator = BlockWindowCalculator::new(provider, "cache.json".to_string());
//!
//! let date = NaiveDate::from_ymd_opt(2025, 10, 15).unwrap();
//! let window = calculator.get_daily_window(NamedChain::Arbitrum, date).await?;
//!
//! println!("Blocks for {}: [{}, {}]", date, window.start_block, window.end_block);
//! ```

use alloy_chains::NamedChain;
use alloy_primitives::BlockNumber;
use alloy_provider::Provider;
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tracing::{debug, info};

use crate::tracing::spans;
use crate::types::config::BlockCount;

/// Unix timestamp in seconds (always UTC)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnixTimestamp(pub i64);

impl UnixTimestamp {
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self(dt.timestamp())
    }

    /// Creates a UnixTimestamp from a u64 value
    pub fn from_u64(ts: u64) -> Self {
        Self(ts as i64)
    }

    /// Converts to u64 for use with blockchain timestamps
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    /// Subtracts one second from the timestamp
    pub fn pred(&self) -> Self {
        Self(self.0 - 1)
    }
}

impl std::fmt::Display for UnixTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents an inclusive block range for a specific UTC day on a blockchain
///
/// A daily window captures:
/// - The first block produced on or after 00:00:00 UTC on the given date
/// - The last block produced at or before 23:59:59 UTC on the given date
/// - The exact UTC timestamps that define the day boundaries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailyBlockWindow {
    /// First block number in the window (inclusive)
    pub start_block: BlockNumber,

    /// Last block number in the window (inclusive)
    pub end_block: BlockNumber,

    /// UTC timestamp at start of day (00:00:00 UTC)
    pub start_ts: UnixTimestamp,

    /// UTC timestamp at start of next day (00:00:00 UTC next day) - exclusive boundary
    pub end_ts_exclusive: UnixTimestamp,
}

impl DailyBlockWindow {
    /// Creates a new daily block window
    pub fn new(
        start_block: BlockNumber,
        end_block: BlockNumber,
        start_ts: UnixTimestamp,
        end_ts_exclusive: UnixTimestamp,
    ) -> Result<Self> {
        if end_block < start_block {
            anyhow::bail!("Invalid block range: end_block={end_block} < start_block={start_block}");
        }
        if end_ts_exclusive.0 <= start_ts.0 {
            anyhow::bail!(
                "Invalid timestamp range: end_ts={} <= start_ts={}",
                end_ts_exclusive.0,
                start_ts.0
            );
        }
        Ok(Self {
            start_block,
            end_block,
            start_ts,
            end_ts_exclusive,
        })
    }

    /// Returns the number of blocks in this window (inclusive)
    pub fn block_count(&self) -> BlockCount {
        let count = self
            .end_block
            .saturating_sub(self.start_block)
            .saturating_add(1);
        BlockCount::new(count)
    }
}

/// Key for caching daily block windows
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct CacheKey {
    chain: NamedChain,
    date: NaiveDate,
}

impl CacheKey {
    fn new(chain: NamedChain, date: NaiveDate) -> Self {
        Self { chain, date }
    }
}

impl std::fmt::Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.chain as u64, self.date)
    }
}

/// Cache for daily block windows
///
/// This cache persists to disk to avoid repeated binary searches across
/// the blockchain when calculating block ranges for the same days.
#[derive(Debug, Default, Serialize, Deserialize)]
struct BlockWindowCache {
    /// Maps CacheKey (chain + date) to DailyBlockWindow
    windows: HashMap<CacheKey, DailyBlockWindow>,
}

impl BlockWindowCache {
    async fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            debug!(path = %path.display(), "Cache file does not exist, creating new cache");
            return Ok(Self::default());
        }

        let data = tokio::fs::read(path)
            .await
            .context("Failed to read cache file")?;

        let cache: Self =
            serde_json::from_slice(&data).context("Failed to deserialize cache file")?;

        info!(path = %path.display(), entries = cache.windows.len(), "Loaded block window cache");
        Ok(cache)
    }

    async fn save(&self, path: &Path) -> Result<()> {
        let data = serde_json::to_vec_pretty(self).context("Failed to serialize cache")?;

        tokio::fs::write(path, data)
            .await
            .context("Failed to write cache file")?;

        debug!(path = %path.display(), entries = self.windows.len(), "Saved block window cache");
        Ok(())
    }

    fn get(&self, key: &CacheKey) -> Option<&DailyBlockWindow> {
        self.windows.get(key)
    }

    fn insert(&mut self, key: CacheKey, window: DailyBlockWindow) {
        self.windows.insert(key, window);
    }
}

/// Calculates and caches daily block windows for blockchain queries
pub struct BlockWindowCalculator<P> {
    provider: P,
    cache_path: Box<Path>,
}

impl<P: Provider> BlockWindowCalculator<P> {
    /// Creates a new calculator with the given provider and cache file path
    pub fn new(provider: P, cache_path: impl AsRef<Path>) -> Self {
        Self {
            provider,
            cache_path: cache_path.as_ref().into(),
        }
    }

    /// Fetches the timestamp of a specific block
    async fn get_block_timestamp(&self, block_number: BlockNumber) -> Result<UnixTimestamp> {
        let span = spans::get_block_timestamp(block_number);
        let _guard = span.enter();

        let block = self
            .provider
            .get_block_by_number(block_number.into())
            .await
            .context("Failed to fetch block")?
            .context("Block not found")?;

        Ok(UnixTimestamp::from_u64(block.header.timestamp))
    }

    /// Binary search to find the first block at or after the target timestamp
    ///
    /// Returns the block number of the first block with timestamp >= target_ts
    async fn find_first_block_at_or_after(
        &self,
        target_ts: UnixTimestamp,
        latest_block: BlockNumber,
    ) -> Result<BlockNumber> {
        let span = spans::find_first_block_at_or_after(target_ts.as_u64(), latest_block);
        let _guard = span.enter();

        let mut lo = 0u64;
        let mut hi = latest_block;
        let mut result = latest_block;

        while lo <= hi {
            let mid = (lo + hi) / 2;
            let ts = self.get_block_timestamp(mid).await?;

            if ts >= target_ts {
                result = mid;
                if mid == 0 {
                    break;
                }
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }

        debug!(target_ts = %target_ts, result, "Found first block at or after timestamp");
        Ok(result)
    }

    /// Binary search to find the last block at or before the target timestamp
    ///
    /// Returns the block number of the last block with timestamp <= target_ts
    async fn find_last_block_at_or_before(
        &self,
        target_ts: UnixTimestamp,
        latest_block: BlockNumber,
    ) -> Result<BlockNumber> {
        let span = spans::find_last_block_at_or_before(target_ts.as_u64(), latest_block);
        let _guard = span.enter();

        let mut lo = 0u64;
        let mut hi = latest_block;
        let mut result = 0u64;

        while lo <= hi {
            let mid = (lo + hi) / 2;
            let ts = self.get_block_timestamp(mid).await?;

            if ts <= target_ts {
                result = mid;
                lo = mid + 1;
            } else {
                if mid == 0 {
                    break;
                }
                hi = mid - 1;
            }
        }

        debug!(target_ts = %target_ts, result, "Found last block at or before timestamp");
        Ok(result)
    }

    /// Gets (or computes and caches) the daily block window for a specific chain and date
    ///
    /// This method:
    /// 1. Checks the cache for an existing window
    /// 2. If not found, performs binary searches to find the block range
    /// 3. Saves the result to the cache for future use
    ///
    /// # Arguments
    /// * `chain` - The named chain for which to calculate the block window
    /// * `date` - The UTC date for which to calculate the block window
    ///
    /// # Returns
    /// A `DailyBlockWindow` containing the start/end blocks and timestamps
    pub async fn get_daily_window(
        &self,
        chain: NamedChain,
        date: NaiveDate,
    ) -> Result<DailyBlockWindow> {
        let span = spans::get_daily_window(chain, date);
        let _guard = span.enter();

        let mut cache = BlockWindowCache::load(&self.cache_path).await?;
        let key = CacheKey::new(chain, date);

        // Check cache first
        if let Some(window) = cache.get(&key) {
            info!(chain = %chain, date = %date, cached = true, "Retrieved daily block window from cache");
            return Ok(window.clone());
        }

        // Calculate UTC day boundaries
        let start_dt = Utc
            .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
            .single()
            .context("Invalid date for UTC conversion")?;

        let end_dt = start_dt
            .checked_add_signed(chrono::TimeDelta::days(1))
            .context("Date overflow when adding 1 day")?;

        let start_ts = UnixTimestamp::from_datetime(start_dt);
        let end_ts_exclusive = UnixTimestamp::from_datetime(end_dt);

        // Get latest block number
        let latest_block = self
            .provider
            .get_block_number()
            .await
            .context("Failed to get latest block number")?;

        info!(
            chain = %chain,
            date = %date,
            start_ts = %start_ts,
            end_ts_exclusive = %end_ts_exclusive,
            latest_block,
            "Computing daily block window"
        );

        // Binary search for block boundaries
        let start_block = self
            .find_first_block_at_or_after(start_ts, latest_block)
            .await?;

        let end_block = self
            .find_last_block_at_or_before(end_ts_exclusive.pred(), latest_block)
            .await?;

        let window = DailyBlockWindow::new(start_block, end_block, start_ts, end_ts_exclusive)?;

        info!(
            chain = %chain,
            date = %date,
            start_block = window.start_block,
            end_block = window.end_block,
            block_count = window.block_count().as_u64(),
            "Computed daily block window"
        );

        // Save to cache
        cache.insert(key, window.clone());
        cache.save(&self.cache_path).await?;

        Ok(window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_display() {
        let key = CacheKey::new(
            NamedChain::Arbitrum,
            NaiveDate::from_ymd_opt(2025, 10, 10).unwrap(),
        );
        let serialized = key.to_string();
        assert_eq!(serialized, "42161:2025-10-10");
    }

    #[test]
    fn test_daily_block_window_validation() {
        let start_ts = UnixTimestamp(1728518400);
        let end_ts = UnixTimestamp(1728604800);

        // Valid window
        let window = DailyBlockWindow::new(1000, 2000, start_ts, end_ts);
        assert!(window.is_ok());
        assert_eq!(window.unwrap().block_count().as_u64(), 1001);

        // Invalid: end_block < start_block
        let invalid = DailyBlockWindow::new(2000, 1000, start_ts, end_ts);
        assert!(invalid.is_err());

        // Invalid: end_ts <= start_ts
        let invalid = DailyBlockWindow::new(1000, 2000, end_ts, start_ts);
        assert!(invalid.is_err());
    }

    #[test]
    fn test_block_window_edge_cases() {
        // Test edge cases for block window calculations

        // Single block window
        let single = DailyBlockWindow {
            start_block: 1000,
            end_block: 1000,
            start_ts: UnixTimestamp(1697328000),
            end_ts_exclusive: UnixTimestamp(1697414400),
        };
        // Single block: [1000, 1000] contains 1 block
        assert_eq!(single.block_count().as_u64(), 1);

        // Large block range (e.g., Arbitrum produces ~40k blocks per day)
        let large = DailyBlockWindow {
            start_block: 100_000_000,
            end_block: 100_040_000,
            start_ts: UnixTimestamp(1697328000),
            end_ts_exclusive: UnixTimestamp(1697414400),
        };
        // Inclusive: [100M, 100M+40k] contains 40,001 blocks
        assert_eq!(large.block_count().as_u64(), 40_001);

        // Standard range
        let window = DailyBlockWindow {
            start_block: 1000,
            end_block: 2000,
            start_ts: UnixTimestamp(1697328000),
            end_ts_exclusive: UnixTimestamp(1697414400),
        };
        // Inclusive count: [1000, 2000] contains 1001 blocks
        assert_eq!(window.block_count().as_u64(), 1001);
    }

    #[test]
    fn test_block_window_validation_errors() {
        // Test all validation error cases
        let start_ts = UnixTimestamp(1728518400);
        let end_ts = UnixTimestamp(1728604800);

        // Error: end_block < start_block
        let result = DailyBlockWindow::new(2000, 1000, start_ts, end_ts);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid block range"));

        // Error: end_ts <= start_ts (equal)
        let result = DailyBlockWindow::new(1000, 2000, start_ts, start_ts);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid timestamp range"));

        // Error: end_ts < start_ts (reversed)
        let result = DailyBlockWindow::new(1000, 2000, end_ts, start_ts);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid timestamp range"));
    }

    #[test]
    fn test_block_window_zero_values() {
        // Test edge case: block numbers starting at 0
        let start_ts = UnixTimestamp(1728518400);
        let end_ts = UnixTimestamp(1728604800);

        // Valid: blocks 0 to 100
        let window = DailyBlockWindow::new(0, 100, start_ts, end_ts);
        assert!(window.is_ok());
        assert_eq!(window.unwrap().block_count().as_u64(), 101);

        // Valid: single block at 0
        let window = DailyBlockWindow::new(0, 0, start_ts, end_ts);
        assert!(window.is_ok());
        assert_eq!(window.unwrap().block_count().as_u64(), 1);
    }

    #[test]
    fn test_block_window_large_values() {
        // Test with very large block numbers (real-world Arbitrum has blocks > 100M)
        let start_ts = UnixTimestamp(1728518400);
        let end_ts = UnixTimestamp(1728604800);

        // Arbitrum-scale block numbers
        let window = DailyBlockWindow::new(100_000_000, 100_040_000, start_ts, end_ts);
        assert!(window.is_ok());
        assert_eq!(window.unwrap().block_count().as_u64(), 40_001);

        // Very large range
        let window = DailyBlockWindow::new(1_000_000_000, 1_001_000_000, start_ts, end_ts);
        assert!(window.is_ok());
        assert_eq!(window.unwrap().block_count().as_u64(), 1_000_001);
    }

    #[test]
    fn test_block_window_count_overflow_protection() {
        // Test that block_count() handles near-overflow cases safely
        let start_ts = UnixTimestamp(1728518400);
        let end_ts = UnixTimestamp(1728604800);

        // Near u64::MAX (should use saturating arithmetic)
        let window = DailyBlockWindow::new(u64::MAX - 100, u64::MAX, start_ts, end_ts);
        assert!(window.is_ok());
        // Should saturate rather than wrap
        let count = window.unwrap().block_count();
        assert_eq!(count.as_u64(), 101);
    }
}
