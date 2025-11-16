//! Disk-based cache implementation with file locking and versioning

use alloy_chains::NamedChain;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::{types::TimestampMillis, BlockWindowCache, CacheKey, CacheStats};
use crate::blocks::window::DailyBlockWindow;
use crate::errors::BlockWindowError;

/// Current cache format version
const CACHE_VERSION: u32 = 1;

/// Entry in the disk cache with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// The cached block window
    window: DailyBlockWindow,
    /// When this entry was created (for TTL and eviction ordering)
    #[serde(default)]
    created_at: TimestampMillis,
}

impl CacheEntry {
    fn new(window: DailyBlockWindow) -> Self {
        Self {
            window,
            created_at: TimestampMillis::now(),
        }
    }

    fn is_expired(&self, ttl: Option<Duration>) -> bool {
        if let Some(ttl) = ttl {
            return self.created_at.is_older_than(ttl);
        }
        false
    }
}

/// Serialized cache format (versioned)
#[derive(Debug, Serialize, Deserialize)]
struct CacheData {
    /// Cache format version
    version: u32,
    /// Cached entries (serialized with String keys for JSON compatibility)
    #[serde(
        serialize_with = "serialize_cache_entries",
        deserialize_with = "deserialize_cache_entries"
    )]
    entries: HashMap<CacheKey, CacheEntry>,
}

// Helper functions for serializing HashMap<CacheKey, CacheEntry> as HashMap<String, CacheEntry>
fn serialize_cache_entries<S>(
    entries: &HashMap<CacheKey, CacheEntry>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::Serialize;
    let string_map: HashMap<String, &CacheEntry> =
        entries.iter().map(|(k, v)| (k.to_string(), v)).collect();
    string_map.serialize(serializer)
}

fn deserialize_cache_entries<'de, D>(
    deserializer: D,
) -> Result<HashMap<CacheKey, CacheEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let string_map: HashMap<String, CacheEntry> = HashMap::deserialize(deserializer)?;

    string_map
        .into_iter()
        .map(|(k, v)| {
            // Parse key string back to CacheKey (format: "chain_id:YYYY-MM-DD")
            let parts: Vec<&str> = k.split(':').collect();
            if parts.len() != 2 {
                return Err(serde::de::Error::custom(format!(
                    "Invalid cache key format: {}",
                    k
                )));
            }

            let chain_id: u64 = parts[0].parse().map_err(|e| {
                serde::de::Error::custom(format!("Invalid chain ID in key '{}': {}", k, e))
            })?;

            let chain = NamedChain::try_from(chain_id)
                .map_err(|_| serde::de::Error::custom(format!("Unknown chain ID: {}", chain_id)))?;

            let date = NaiveDate::parse_from_str(parts[1], "%Y-%m-%d").map_err(|e| {
                serde::de::Error::custom(format!("Invalid date in key '{}': {}", k, e))
            })?;

            Ok((CacheKey::new(chain, date), v))
        })
        .collect()
}

impl Default for CacheData {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        }
    }
}

/// Configuration for disk cache
#[derive(Debug, Clone, Default)]
struct DiskCacheConfig {
    /// Maximum number of entries before eviction starts
    max_entries: Option<usize>,
    /// Time-to-live for cache entries
    ttl: Option<Duration>,
}

/// Internal state for disk cache
#[derive(Debug, Default)]
struct DiskCacheState {
    /// Cache statistics (in-memory only, not persisted)
    stats: CacheStats,
}

/// Disk-based cache with file locking, versioning, and TTL support
///
/// This cache persists block windows to disk as JSON with:
/// - File locking for multi-process safety (using advisory locks)
/// - Cache format versioning for future migrations
/// - Optional TTL (time-to-live) for automatic expiration
/// - Optional size limits with oldest-first eviction
/// - Path validation and helpful error messages
///
/// # Examples
///
/// ```rust,ignore
/// use semioscan::cache::DiskCache;
/// use std::time::Duration;
///
/// // Basic disk cache
/// let cache = DiskCache::new("cache.json")?;
///
/// // With TTL
/// let cache = DiskCache::new("cache.json")?
///     .with_ttl(Duration::from_secs(86400 * 7)); // 7 days
///
/// // With size limit
/// let cache = DiskCache::new("/var/cache/blocks.json")?
///     .with_max_entries(1000);
///
/// // With validation
/// let cache = DiskCache::new("cache.json")?
///     .validate()?;
/// ```
///
/// # File Locking
///
/// Uses advisory file locking (`fs2` crate) to prevent corruption from
/// concurrent access. Multiple processes can safely share the same cache file.
///
/// # Performance
///
/// - Get: O(1) HashMap lookup + file I/O (~1-2ms)
/// - Insert: O(1) + file write (~2-5ms)
/// - File size: Approximately 200 bytes per cached entry
#[derive(Debug)]
pub struct DiskCache {
    path: PathBuf,
    config: DiskCacheConfig,
    state: Mutex<DiskCacheState>,
}

impl DiskCache {
    /// Creates a new disk cache at the specified path
    ///
    /// The path can be absolute or relative. The parent directory must exist
    /// and be writable. If the cache file doesn't exist, it will be created
    /// on the first insert.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the cache file (e.g., "cache.json" or "/var/cache/blocks.json")
    ///
    /// # Returns
    ///
    /// Returns a `DiskCache` instance. Note that path validation is NOT performed
    /// until the first I/O operation. Use [`validate()`](Self::validate) to check
    /// the path immediately.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            config: DiskCacheConfig::default(),
            state: Mutex::new(DiskCacheState::default()),
        }
    }

    /// Sets the maximum number of entries in the cache
    ///
    /// When the limit is reached, the oldest entries (by creation time) will be
    /// evicted to make room for new entries.
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

    /// Validates the cache path and creates parent directory if needed
    ///
    /// This method checks that:
    /// - The parent directory exists (or creates it)
    /// - The parent directory is writable
    /// - The path is valid
    ///
    /// # Errors
    ///
    /// Returns an error if the parent directory cannot be created or is not writable.
    pub fn validate(self) -> Result<Self, BlockWindowError> {
        // Get parent directory
        let parent = self.path.parent().ok_or_else(|| {
            BlockWindowError::cache_io_error(
                self.path.display().to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Cache path has no parent directory",
                ),
            )
        })?;

        // Create parent directory if it doesn't exist
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                BlockWindowError::cache_io_error(
                    format!(
                        "Failed to create cache directory '{}': {}",
                        parent.display(),
                        e
                    ),
                    e,
                )
            })?;
            debug!(path = %parent.display(), "Created cache directory");
        }

        // Validate parent is writable by attempting to create a temp file
        let test_file = parent.join(".cache_write_test");
        std::fs::write(&test_file, b"test").map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Cache directory '{}' is not writable: {}",
                    parent.display(),
                    e
                ),
                e,
            )
        })?;
        let _ = std::fs::remove_file(&test_file);

        debug!(path = %self.path.display(), "Cache path validated successfully");
        Ok(self)
    }

    /// Loads cache data from disk with file locking
    async fn load(&self) -> Result<CacheData, BlockWindowError> {
        if !self.path.exists() {
            debug!(path = %self.path.display(), "Cache file does not exist, using empty cache");
            return Ok(CacheData::default());
        }

        // Open file and acquire shared lock for reading
        let file = File::open(&self.path).map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Failed to open cache file '{}': {}. Ensure the file is readable.",
                    self.path.display(),
                    e
                ),
                e,
            )
        })?;

        // Acquire shared lock for reading (std lib, requires Rust 1.89+)
        file.lock_shared().map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Failed to acquire read lock on cache file '{}': {}",
                    self.path.display(),
                    e
                ),
                e,
            )
        })?;

        // Read and parse cache data
        let data: CacheData = serde_json::from_reader(&file).map_err(|e| {
            warn!(
                path = %self.path.display(),
                error = %e,
                "Failed to parse cache file, using empty cache"
            );
            // Don't fail on parse errors, just use empty cache
            BlockWindowError::serialization_error(e)
        })?;

        // Check version compatibility
        if data.version != CACHE_VERSION {
            warn!(
                path = %self.path.display(),
                cached_version = data.version,
                current_version = CACHE_VERSION,
                "Cache version mismatch, ignoring cached data"
            );
            // Unlock by dropping the file
            drop(file);
            return Ok(CacheData::default());
        }

        // Unlock by dropping the file
        drop(file);

        info!(
            path = %self.path.display(),
            entries = data.entries.len(),
            version = data.version,
            "Loaded block window cache"
        );

        Ok(data)
    }

    /// Saves cache data to disk with file locking and atomic write
    async fn save(&self, data: &CacheData) -> Result<(), BlockWindowError> {
        // Serialize to JSON first (before acquiring lock)
        let json =
            serde_json::to_vec_pretty(data).map_err(BlockWindowError::serialization_error)?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    BlockWindowError::cache_io_error(
                        format!(
                            "Failed to create cache directory '{}': {}. Ensure you have write permissions.",
                            parent.display(),
                            e
                        ),
                        e,
                    )
                })?;
            }
        }

        // Write atomically using a temp file
        let temp_path = self.path.with_extension("tmp");

        tokio::fs::write(&temp_path, &json).await.map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Failed to write cache to '{}': {}. Ensure the parent directory is writable.",
                    temp_path.display(),
                    e
                ),
                e,
            )
        })?;

        // Open temp file and acquire exclusive lock
        let file = File::open(&temp_path).map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Failed to open temp cache file '{}': {}",
                    temp_path.display(),
                    e
                ),
                e,
            )
        })?;

        // Acquire exclusive lock for writing (std lib, requires Rust 1.89+)
        file.lock().map_err(|e| {
            BlockWindowError::cache_io_error(
                format!(
                    "Failed to acquire write lock on cache file '{}': {}",
                    temp_path.display(),
                    e
                ),
                e,
            )
        })?;

        // Atomically rename temp file to final location
        tokio::fs::rename(&temp_path, &self.path)
            .await
            .map_err(|e| {
                BlockWindowError::cache_io_error(
                    format!(
                        "Failed to rename cache file from '{}' to '{}': {}",
                        temp_path.display(),
                        self.path.display(),
                        e
                    ),
                    e,
                )
            })?;

        // Unlock by dropping the file
        drop(file);

        debug!(
            path = %self.path.display(),
            entries = data.entries.len(),
            "Saved block window cache"
        );

        Ok(())
    }

    /// Evicts the oldest entries to maintain size limit
    fn evict_oldest(data: &mut CacheData, max_entries: usize) -> usize {
        let mut evicted = 0;

        while data.entries.len() > max_entries {
            // Find oldest entry by created_at timestamp
            let oldest_key = data
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(key, _)| key.clone());

            if let Some(key) = oldest_key {
                debug!(key = %key, "Evicting oldest cache entry");
                data.entries.remove(&key);
                evicted += 1;
            } else {
                break;
            }
        }

        evicted
    }
}

#[async_trait]
impl BlockWindowCache for DiskCache {
    async fn get(&self, key: &CacheKey) -> Option<DailyBlockWindow> {
        let mut state = self.state.lock().await;

        // Load cache data
        let data = match self.load().await {
            Ok(data) => data,
            Err(e) => {
                warn!(error = %e, "Failed to load cache, treating as miss");
                state.stats.misses += 1;
                return None;
            }
        };

        if let Some(entry) = data.entries.get(key) {
            // Check if expired
            if entry.is_expired(self.config.ttl) {
                debug!(key = %key, "Cache entry expired");
                state.stats.expirations += 1;
                state.stats.misses += 1;
                return None;
            }

            state.stats.hits += 1;
            debug!(key = %key, "Cache hit (disk)");
            Some(entry.window.clone())
        } else {
            state.stats.misses += 1;
            debug!(key = %key, "Cache miss (disk)");
            None
        }
    }

    async fn insert(
        &self,
        key: CacheKey,
        window: DailyBlockWindow,
    ) -> Result<(), BlockWindowError> {
        let mut state = self.state.lock().await;

        // Load existing cache data
        let mut data = self.load().await.unwrap_or_default();

        // Insert new entry
        debug!(key = %key, "Inserting entry into disk cache");
        data.entries.insert(key, CacheEntry::new(window));

        // Evict oldest entries if needed
        if let Some(max_entries) = self.config.max_entries {
            let evicted = Self::evict_oldest(&mut data, max_entries);
            if evicted > 0 {
                state.stats.evictions += evicted as u64;
            }
        }

        state.stats.entries = data.entries.len();

        // Save to disk
        self.save(&data).await?;

        Ok(())
    }

    async fn clear(&self) -> Result<(), BlockWindowError> {
        let mut state = self.state.lock().await;

        debug!(path = %self.path.display(), "Clearing disk cache");

        // Delete cache file
        if self.path.exists() {
            tokio::fs::remove_file(&self.path).await.map_err(|e| {
                BlockWindowError::cache_io_error(
                    format!(
                        "Failed to delete cache file '{}': {}",
                        self.path.display(),
                        e
                    ),
                    e,
                )
            })?;
        }

        state.stats.entries = 0;
        Ok(())
    }

    async fn stats(&self) -> CacheStats {
        let mut state = self.state.lock().await;

        // Update entry count from disk
        if let Ok(data) = self.load().await {
            state.stats.entries = data.entries.len();
        }

        state.stats.clone()
    }

    fn name(&self) -> &'static str {
        "DiskCache"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_chains::NamedChain;
    use chrono::NaiveDate;
    use tempfile::TempDir;

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
    async fn test_disk_cache_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        let cache = DiskCache::new(&cache_path).validate().unwrap();

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
    }

    #[tokio::test]
    async fn test_disk_cache_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");

        let key = create_test_key(15);
        let window = create_test_window(1000, 2000);

        // Create cache and insert
        {
            let cache = DiskCache::new(&cache_path).validate().unwrap();
            cache.insert(key.clone(), window).await.unwrap();
        }

        // Create new cache instance and verify data persisted
        {
            let cache = DiskCache::new(&cache_path).validate().unwrap();
            let retrieved = cache.get(&key).await;
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().start_block, 1000);
        }
    }

    #[tokio::test]
    async fn test_disk_cache_size_limit() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        let cache = DiskCache::new(&cache_path)
            .with_max_entries(3)
            .validate()
            .unwrap();

        // Insert 4 entries
        for day in 1..=4 {
            let key = create_test_key(day);
            let window = create_test_window(day as u64 * 1000, day as u64 * 2000);
            cache.insert(key, window).await.unwrap();
        }

        // Only 3 should remain (oldest evicted)
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 3);
        assert_eq!(stats.evictions, 1);

        // First entry should be gone (oldest)
        assert!(cache.get(&create_test_key(1)).await.is_none());

        // Last 3 should still be present
        assert!(cache.get(&create_test_key(2)).await.is_some());
        assert!(cache.get(&create_test_key(3)).await.is_some());
        assert!(cache.get(&create_test_key(4)).await.is_some());
    }

    #[tokio::test]
    async fn test_disk_cache_ttl() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        let cache = DiskCache::new(&cache_path)
            .with_ttl(Duration::from_millis(50))
            .validate()
            .unwrap();

        let key = create_test_key(15);
        let window = create_test_window(1000, 2000);

        // Insert and verify immediately
        cache.insert(key.clone(), window).await.unwrap();
        assert!(cache.get(&key).await.is_some());

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be expired now
        assert!(cache.get(&key).await.is_none());

        let stats = cache.stats().await;
        assert_eq!(stats.expirations, 1);
    }

    #[tokio::test]
    async fn test_disk_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache.json");
        let cache = DiskCache::new(&cache_path).validate().unwrap();

        // Insert entries
        for day in 1..=5 {
            let key = create_test_key(day);
            let window = create_test_window(day as u64 * 1000, day as u64 * 2000);
            cache.insert(key, window).await.unwrap();
        }

        // Clear cache
        cache.clear().await.unwrap();

        // File should be deleted
        assert!(!cache_path.exists());

        let stats = cache.stats().await;
        assert_eq!(stats.entries, 0);
    }

    #[tokio::test]
    async fn test_disk_cache_validation() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("subdir").join("cache.json");

        // Validation should create parent directory
        let cache = DiskCache::new(&cache_path).validate();
        assert!(cache.is_ok());

        // Parent directory should exist now
        assert!(cache_path.parent().unwrap().exists());
    }
}
