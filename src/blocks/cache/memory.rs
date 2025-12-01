// SPDX-FileCopyrightText: 2025 Semiotic AI, Inc.
//
// SPDX-License-Identifier: Apache-2.0

//! In-memory cache implementation with optional TTL and size limits

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::debug;

use super::{
    types::{AccessSequence, TimestampMillis},
    BlockWindowCache, CacheKey, CacheStats,
};
use crate::blocks::window::DailyBlockWindow;
use crate::errors::BlockWindowError;

/// Entry in the memory cache with metadata
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached block window
    window: DailyBlockWindow,
    /// When this entry was created
    created_at: TimestampMillis,
    /// When this entry was last accessed (for LRU eviction)
    last_accessed: TimestampMillis,
    /// Sequence number for deterministic LRU ordering when timestamps are equal
    access_seq: AccessSequence,
}

impl CacheEntry {
    fn new(window: DailyBlockWindow, access_seq: AccessSequence) -> Self {
        let now = TimestampMillis::now();
        Self {
            window,
            created_at: now,
            last_accessed: now,
            access_seq,
        }
    }

    fn is_expired(&self, ttl: Option<Duration>) -> bool {
        if let Some(ttl) = ttl {
            return self.created_at.is_older_than(ttl);
        }
        false
    }

    fn touch(&mut self, access_seq: AccessSequence) {
        self.last_accessed = TimestampMillis::now();
        self.access_seq = access_seq;
    }
}

/// Configuration for memory cache
#[derive(Debug, Clone, Default)]
struct MemoryCacheConfig {
    /// Maximum number of entries before eviction starts
    max_entries: Option<usize>,
    /// Time-to-live for cache entries
    ttl: Option<Duration>,
}

/// Internal state for memory cache
#[derive(Debug, Default)]
struct MemoryCacheState {
    /// The cache entries
    entries: HashMap<CacheKey, CacheEntry>,
    /// Cache statistics
    stats: CacheStats,
    /// Sequence counter for deterministic LRU ordering
    next_seq: AccessSequence,
}

/// In-memory cache with optional TTL and size limits
///
/// This cache stores block windows in memory using a HashMap. It supports:
/// - Optional TTL (time-to-live) for automatic expiration
/// - Optional size limits with LRU (least recently used) eviction
/// - Thread-safe concurrent access
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::cache::MemoryCache;
/// use std::time::Duration;
///
/// // Unbounded cache (no limits)
/// let cache = MemoryCache::new();
///
/// // Cache with size limit
/// let cache = MemoryCache::new()
///     .with_max_entries(1000);
///
/// // Cache with TTL
/// let cache = MemoryCache::new()
///     .with_ttl(Duration::from_secs(86400)); // 24 hours
///
/// // Cache with both limits
/// let cache = MemoryCache::new()
///     .with_max_entries(500)
///     .with_ttl(Duration::from_secs(86400 * 7)); // 7 days
/// ```
///
/// # Performance
///
/// - Get: O(1) average case (HashMap lookup)
/// - Insert: O(1) without eviction, O(n) with eviction (finds LRU)
/// - Memory: Approximately 200 bytes per cached entry
#[derive(Debug)]
pub struct MemoryCache {
    config: MemoryCacheConfig,
    state: Mutex<MemoryCacheState>,
}

impl MemoryCache {
    /// Creates a new memory cache with no limits
    pub fn new() -> Self {
        Self {
            config: MemoryCacheConfig::default(),
            state: Mutex::new(MemoryCacheState::default()),
        }
    }

    /// Sets the maximum number of entries in the cache
    ///
    /// When the limit is reached, the least recently used (LRU) entry will be evicted
    /// to make room for new entries.
    pub fn with_max_entries(mut self, max_entries: usize) -> Self {
        self.config.max_entries = Some(max_entries);
        self
    }

    /// Sets the time-to-live for cache entries
    ///
    /// Entries older than the TTL will be automatically expired when accessed.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.config.ttl = Some(ttl);
        self
    }

    /// Evicts the least recently used entry from the cache
    fn evict_lru(state: &mut MemoryCacheState) {
        if state.entries.is_empty() {
            return;
        }

        // Find the least recently used entry (by timestamp, then by sequence number)
        let lru_key = state
            .entries
            .iter()
            .min_by_key(|(_, entry)| (entry.last_accessed, entry.access_seq))
            .map(|(key, _)| key.clone());

        if let Some(key) = lru_key {
            debug!(key = %key, "Evicting LRU cache entry");
            state.entries.remove(&key);
            state.stats.evictions += 1;
        }
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlockWindowCache for MemoryCache {
    async fn get(&self, key: &CacheKey) -> Option<DailyBlockWindow> {
        let mut state = self.state.lock().await;

        // Get sequence number before borrowing entries
        let seq = state.next_seq;

        // Check if entry exists and is not expired
        let (result, should_increment_seq) = if let Some(entry) = state.entries.get_mut(key) {
            // Check if expired
            if entry.is_expired(self.config.ttl) {
                debug!(key = %key, "Cache entry expired");
                state.entries.remove(key);
                state.stats.expirations += 1;
                state.stats.misses += 1;
                (None, false)
            } else {
                // Update access time with sequence number
                entry.touch(seq);
                let window = entry.window.clone();
                (Some(window), true)
            }
        } else {
            (None, false)
        };

        // Increment sequence counter if we accessed an entry
        if should_increment_seq {
            state.next_seq = state.next_seq.next();
        }

        // Update stats after releasing the entry borrow
        if result.is_some() {
            state.stats.hits += 1;
            debug!(key = %key, "Cache hit (memory)");
        } else if state.entries.contains_key(key) {
            // Entry existed but was expired (already counted in expirations)
        } else {
            state.stats.misses += 1;
            debug!(key = %key, "Cache miss (memory)");
        }

        result
    }

    async fn insert(
        &self,
        key: CacheKey,
        window: DailyBlockWindow,
    ) -> Result<(), BlockWindowError> {
        let mut state = self.state.lock().await;

        // Check if we need to evict before inserting
        if let Some(max_entries) = self.config.max_entries {
            while state.entries.len() >= max_entries {
                Self::evict_lru(&mut state);
            }
        }

        debug!(key = %key, "Inserting entry into memory cache");
        let seq = state.next_seq;
        state.next_seq = state.next_seq.next();
        state.entries.insert(key, CacheEntry::new(window, seq));
        state.stats.entries = state.entries.len();

        Ok(())
    }

    async fn clear(&self) -> Result<(), BlockWindowError> {
        let mut state = self.state.lock().await;
        debug!(entries = state.entries.len(), "Clearing memory cache");
        state.entries.clear();
        state.stats.entries = 0;
        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        let state = self.state.lock().await;
        state.stats.clone()
    }

    fn name(&self) -> &'static str {
        "MemoryCache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use chrono::NaiveDate;

    fn create_test_window(start_block: u64, end_block: u64) -> DailyBlockWindow {
        DailyBlockWindow {
            start_block,
            end_block,
            start_ts: crate::blocks::window::UnixTimestamp(1728518400),
            end_ts_exclusive: crate::blocks::window::UnixTimestamp(1728604800),
        }
    }

    fn create_test_key(day: u32) -> CacheKey {
        CacheKey::new(
            NamedChain::Arbitrum,
            NaiveDate::from_ymd_opt(2025, 10, day).unwrap(),
        )
    }

    #[tokio::test]
    async fn test_memory_cache_basic_operations() {
        let cache = MemoryCache::new();
        let key = create_test_key(15);
        let window = create_test_window(1000, 2000);

        // Cache miss initially
        assert!(cache.get(&key).await.is_none());

        // Insert and verify
        assert!(cache.insert(key.clone(), window.clone()).await.is_ok());
        let retrieved = cache.get(&key).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().start_block, 1000);

        // Stats should show 1 hit, 1 miss
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
    }

    #[tokio::test]
    async fn test_memory_cache_size_limit() {
        let cache = MemoryCache::new().with_max_entries(3);

        // Insert 3 entries (fill the cache)
        for day in 1..=3 {
            let key = create_test_key(day);
            let window = create_test_window(day as u64 * 1000, day as u64 * 2000);
            cache.insert(key, window).await.unwrap();
        }

        // All 3 should be present
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 3);

        // Access day 1 to make it recently used
        let key1 = create_test_key(1);
        assert!(cache.get(&key1).await.is_some());

        // Insert day 4 - should evict day 2 (least recently used)
        let key4 = create_test_key(4);
        let window4 = create_test_window(4000, 8000);
        cache.insert(key4.clone(), window4).await.unwrap();

        // Cache should still have 3 entries
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 3);
        assert_eq!(stats.evictions, 1);

        // Day 1 and 3 should still be present (recently used)
        assert!(cache.get(&create_test_key(1)).await.is_some());
        assert!(cache.get(&create_test_key(3)).await.is_some());
        assert!(cache.get(&key4).await.is_some());

        // Day 2 should have been evicted
        assert!(cache.get(&create_test_key(2)).await.is_none());
    }

    #[tokio::test]
    async fn test_memory_cache_ttl() {
        let cache = MemoryCache::new().with_ttl(Duration::from_millis(50));

        let key = create_test_key(15);
        let window = create_test_window(1000, 2000);

        // Insert and verify immediately
        cache.insert(key.clone(), window).await.unwrap();
        assert!(cache.get(&key).await.is_some());

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be expired now
        assert!(cache.get(&key).await.is_none());

        // Stats should show expiration
        let stats = cache.stats().await;
        assert_eq!(stats.expirations, 1);
    }

    #[tokio::test]
    async fn test_memory_cache_clear() {
        let cache = MemoryCache::new();

        // Insert multiple entries
        for day in 1..=5 {
            let key = create_test_key(day);
            let window = create_test_window(day as u64 * 1000, day as u64 * 2000);
            cache.insert(key, window).await.unwrap();
        }

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 5);

        // Clear cache
        cache.clear().await.unwrap();

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 0);

        // All entries should be gone
        for day in 1..=5 {
            assert!(cache.get(&create_test_key(day)).await.is_none());
        }
    }

    #[tokio::test]
    async fn test_memory_cache_hit_rate() {
        let cache = MemoryCache::new();
        let key = create_test_key(15);
        let window = create_test_window(1000, 2000);

        // 1 miss
        cache.get(&key).await;

        // Insert
        cache.insert(key.clone(), window).await.unwrap();

        // 3 hits
        cache.get(&key).await;
        cache.get(&key).await;
        cache.get(&key).await;

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate(), 75.0);
    }
}
