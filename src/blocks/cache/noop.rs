//! No-operation cache that disables caching entirely

use async_trait::async_trait;

use super::{BlockWindowCache, CacheKey, CacheStats};
use crate::blocks::window::DailyBlockWindow;
use crate::errors::BlockWindowError;

/// A no-operation cache that disables caching entirely
///
/// This cache backend always returns `None` for reads and ignores writes.
/// Use this when you want to disable caching for testing or specific scenarios
/// where caching is not desired.
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::cache::NoOpCache;
/// use semioscan::BlockWindowCalculator;
///
/// let cache = NoOpCache;
/// let calculator = BlockWindowCalculator::new(provider, Box::new(cache));
/// ```
///
/// # Performance
///
/// This cache has near-zero overhead (<0.01ms per operation). Every call to
/// `get_daily_window()` will perform RPC queries to calculate the block window.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpCache;

#[async_trait]
impl BlockWindowCache for NoOpCache {
    async fn get(&self, _key: &CacheKey) -> Option<DailyBlockWindow> {
        // Always return None (cache miss)
        None
    }

    async fn insert(
        &self,
        _key: CacheKey,
        _window: DailyBlockWindow,
    ) -> Result<(), BlockWindowError> {
        // Ignore writes
        Ok(())
    }

    async fn clear(&self) -> Result<(), BlockWindowError> {
        // Nothing to clear
        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        // No statistics to track
        CacheStats::default()
    }

    fn name(&self) -> &'static str {
        "NoOpCache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use chrono::NaiveDate;

    #[tokio::test]
    async fn test_noop_cache_always_misses() {
        let cache = NoOpCache;
        let key = CacheKey::new(
            NamedChain::Arbitrum,
            NaiveDate::from_ymd_opt(2025, 10, 15).unwrap(),
        );

        // Should always return None
        assert!(cache.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn test_noop_cache_ignores_writes() {
        let cache = NoOpCache;
        let key = CacheKey::new(
            NamedChain::Arbitrum,
            NaiveDate::from_ymd_opt(2025, 10, 15).unwrap(),
        );

        let window = DailyBlockWindow {
            start_block: 1000,
            end_block: 2000,
            start_ts: crate::blocks::window::UnixTimestamp(1728518400),
            end_ts_exclusive: crate::blocks::window::UnixTimestamp(1728604800),
        };

        // Insert should succeed but do nothing
        assert!(cache.insert(key.clone(), window).await.is_ok());

        // Should still return None after insert
        assert!(cache.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn test_noop_cache_stats() {
        let cache = NoOpCache;
        let stats = cache.stats().await;

        // All stats should be zero
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.entries, 0);
    }

    #[tokio::test]
    async fn test_noop_cache_clear() {
        let cache = NoOpCache;
        // Clear should succeed (no-op)
        assert!(cache.clear().await.is_ok());
    }
}
