//! Cache backends for block window calculations
//!
//! This module provides different caching strategies for storing block window data:
//!
//! - [`DiskCache`]: Persistent JSON-based cache with file locking (default)
//! - [`MemoryCache`]: In-memory cache with optional size limits
//! - [`NoOpCache`]: Disables caching entirely (for testing or specific use cases)
//!
//! # Examples
//!
//! ```rust,ignore
//! use semioscan::cache::{DiskCache, MemoryCache, NoOpCache};
//! use semioscan::BlockWindowCalculator;
//! use std::time::Duration;
//!
//! // Disk cache with TTL and size limits
//! let cache = DiskCache::new("cache.json")
//!     .with_ttl(Duration::from_secs(86400 * 7)) // 7 days
//!     .with_max_entries(1000)
//!     .validate()?;
//! let calculator = BlockWindowCalculator::new(provider, Box::new(cache));
//!
//! // Memory cache (no persistence)
//! let cache = MemoryCache::new()
//!     .with_max_entries(500);
//! let calculator = BlockWindowCalculator::new(provider, Box::new(cache));
//!
//! // No cache (always compute)
//! let cache = NoOpCache;
//! let calculator = BlockWindowCalculator::new(provider, Box::new(cache));
//! ```

use alloy_chains::NamedChain;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::blocks::window::DailyBlockWindow;
use crate::errors::BlockWindowError;

mod disk;
mod memory;
mod noop;
pub mod types;

pub use disk::DiskCache;
pub use memory::MemoryCache;
pub use noop::NoOpCache;

/// Key for caching daily block windows
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub(crate) chain: NamedChain,
    pub(crate) date: NaiveDate,
}

impl CacheKey {
    /// Creates a new cache key for a specific chain and date
    pub fn new(chain: NamedChain, date: NaiveDate) -> Self {
        Self { chain, date }
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.chain as u64, self.date)
    }
}

/// Statistics about cache performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cache hits (successful retrievals)
    pub hits: u64,
    /// Number of cache misses (key not found)
    pub misses: u64,
    /// Number of entries evicted due to size limits
    pub evictions: u64,
    /// Number of entries expired due to TTL
    pub expirations: u64,
    /// Current number of entries in the cache
    pub entries: usize,
}

impl CacheStats {
    /// Calculates the cache hit rate as a percentage (0.0 to 100.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }
}

impl fmt::Display for CacheStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "hits={}, misses={}, evictions={}, expirations={}, entries={}, hit_rate={:.1}%",
            self.hits,
            self.misses,
            self.evictions,
            self.expirations,
            self.entries,
            self.hit_rate()
        )
    }
}

/// Trait for block window cache backends
///
/// Implementations provide different storage strategies for caching block windows.
/// All cache operations are async to support both in-memory and disk-based backends.
///
/// # Thread Safety
///
/// Implementations must be thread-safe and support concurrent access. Use interior
/// mutability (e.g., `Mutex`, `RwLock`) as needed.
///
/// # Error Handling
///
/// Cache operations should not fail the entire operation. If a cache read/write fails,
/// implementations should log the error and continue (treating failures as cache misses).
#[async_trait]
pub trait BlockWindowCache: Send + Sync {
    /// Retrieves a cached block window for the given key
    ///
    /// Returns `None` if:
    /// - The key is not in the cache
    /// - The cached entry has expired (if TTL is enabled)
    /// - A cache read error occurred (logged internally)
    async fn get(&self, key: &CacheKey) -> Option<DailyBlockWindow>;

    /// Inserts a block window into the cache
    ///
    /// If the cache has size limits and is full, this may evict older entries.
    /// Cache write errors are logged but do not cause failures.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the entry was cached successfully, or `Err` if caching failed.
    /// Callers should typically ignore errors (caching is best-effort).
    async fn insert(&self, key: CacheKey, window: DailyBlockWindow)
        -> Result<(), BlockWindowError>;

    /// Clears all entries from the cache
    ///
    /// Used for testing and cache management. Not all backends may support this.
    async fn clear(&self) -> Result<(), BlockWindowError>;

    /// Returns current cache statistics
    ///
    /// Statistics include hits, misses, evictions, and current size.
    async fn stats(&self) -> CacheStats;

    /// Returns a human-readable name for this cache backend
    ///
    /// Used for logging and debugging.
    fn name(&self) -> &'static str;
}
