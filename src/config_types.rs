//! Strong types for configuration values
//!
//! These types ensure configuration values are not confused with
//! blockchain values (block numbers, gas amounts, etc.).

use serde::{Deserialize, Serialize};

/// Maximum block range for RPC queries
///
/// This prevents overloading RPC nodes with queries that are too large.
/// Different chains have different limits based on their RPC infrastructure.
///
/// Typical values:
/// - Conservative: 2000 blocks (works on most chains)
/// - Moderate: 5000 blocks
/// - Generous: 10000 blocks (chains with robust RPC like Base)
///
/// # Examples
///
/// ```
/// use semioscan::MaxBlockRange;
///
/// let conservative = MaxBlockRange::DEFAULT;
/// assert_eq!(conservative.as_u64(), 2000);
///
/// let generous = MaxBlockRange::GENEROUS;
/// assert_eq!(generous.as_u64(), 10000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MaxBlockRange(u64);

impl MaxBlockRange {
    /// Conservative default (works on most chains)
    pub const DEFAULT: Self = Self(2000);

    /// Moderate range for chains with good RPC support
    pub const MODERATE: Self = Self(5000);

    /// For chains with generous RPC limits (e.g., Base)
    pub const GENEROUS: Self = Self(10000);

    /// Very conservative for rate-limited RPCs
    pub const CONSERVATIVE: Self = Self(1000);

    /// Create a new max block range
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::MaxBlockRange;
    ///
    /// let range = MaxBlockRange::new(3000);
    /// assert_eq!(range.as_u64(), 3000);
    /// ```
    pub const fn new(blocks: u64) -> Self {
        Self(blocks)
    }

    /// Get the inner u64 value
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// Calculate number of chunks needed to cover a range
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::MaxBlockRange;
    ///
    /// let range = MaxBlockRange::new(1000);
    /// let chunks = range.chunks_needed(0, 2500);
    /// assert_eq!(chunks, 3); // 0-999, 1000-1999, 2000-2500
    /// ```
    pub fn chunks_needed(&self, start: u64, end: u64) -> usize {
        if end < start {
            return 0;
        }
        let total_blocks = end - start + 1;
        total_blocks.div_ceil(self.0) as usize
    }

    /// Split a block range into chunks
    ///
    /// Returns an iterator of (start_block, end_block) tuples, where each
    /// chunk is at most `self.0` blocks in size.
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::MaxBlockRange;
    ///
    /// let range = MaxBlockRange::new(1000);
    /// let chunks: Vec<_> = range.chunk_range(0, 2500).collect();
    ///
    /// assert_eq!(chunks.len(), 3);
    /// assert_eq!(chunks[0], (0, 999));
    /// assert_eq!(chunks[1], (1000, 1999));
    /// assert_eq!(chunks[2], (2000, 2500));
    /// ```
    pub fn chunk_range(&self, start: u64, end: u64) -> ChunkIterator {
        ChunkIterator {
            current: start,
            end,
            chunk_size: self.0,
        }
    }
}

impl From<u64> for MaxBlockRange {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for MaxBlockRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} blocks", self.0)
    }
}

/// Iterator over block range chunks
///
/// Created by [`MaxBlockRange::chunk_range`]. Yields (start, end) tuples
/// representing block ranges.
#[derive(Debug, Clone)]
pub struct ChunkIterator {
    current: u64,
    end: u64,
    chunk_size: u64,
}

impl Iterator for ChunkIterator {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current > self.end {
            return None;
        }

        let chunk_start = self.current;
        let chunk_end = (self.current + self.chunk_size - 1).min(self.end);

        self.current = chunk_end + 1;

        Some((chunk_start, chunk_end))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.current > self.end {
            (0, Some(0))
        } else {
            let remaining_blocks = self.end - self.current + 1;
            let chunks = remaining_blocks.div_ceil(self.chunk_size) as usize;
            (chunks, Some(chunks))
        }
    }
}

impl ExactSizeIterator for ChunkIterator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_block_range_creation() {
        let range = MaxBlockRange::new(2000);
        assert_eq!(range.as_u64(), 2000);
    }

    #[test]
    fn test_max_block_range_constants() {
        assert_eq!(MaxBlockRange::CONSERVATIVE.as_u64(), 1000);
        assert_eq!(MaxBlockRange::DEFAULT.as_u64(), 2000);
        assert_eq!(MaxBlockRange::MODERATE.as_u64(), 5000);
        assert_eq!(MaxBlockRange::GENEROUS.as_u64(), 10000);
    }

    #[test]
    fn test_chunks_needed() {
        let range = MaxBlockRange::new(1000);

        // Exactly one chunk
        assert_eq!(range.chunks_needed(0, 999), 1);

        // Two chunks
        assert_eq!(range.chunks_needed(0, 1000), 2);

        // Three chunks with partial last chunk
        assert_eq!(range.chunks_needed(0, 2500), 3);

        // Empty range
        assert_eq!(range.chunks_needed(100, 50), 0);

        // Single block
        assert_eq!(range.chunks_needed(100, 100), 1);
    }

    #[test]
    fn test_chunk_range_exact_multiple() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(0, 2999).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], (0, 999));
        assert_eq!(chunks[1], (1000, 1999));
        assert_eq!(chunks[2], (2000, 2999));
    }

    #[test]
    fn test_chunk_range_partial_last_chunk() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(0, 2500).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], (0, 999));
        assert_eq!(chunks[1], (1000, 1999));
        assert_eq!(chunks[2], (2000, 2500));
    }

    #[test]
    fn test_chunk_range_single_chunk() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(0, 500).collect();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], (0, 500));
    }

    #[test]
    fn test_chunk_range_single_block() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(100, 100).collect();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], (100, 100));
    }

    #[test]
    fn test_chunk_range_empty() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(100, 50).collect();

        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_range_non_zero_start() {
        let range = MaxBlockRange::new(1000);
        let chunks: Vec<_> = range.chunk_range(5000, 7500).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], (5000, 5999));
        assert_eq!(chunks[1], (6000, 6999));
        assert_eq!(chunks[2], (7000, 7500));
    }

    #[test]
    fn test_chunk_iterator_size_hint() {
        let range = MaxBlockRange::new(1000);
        let mut iter = range.chunk_range(0, 2500);

        assert_eq!(iter.size_hint(), (3, Some(3)));

        iter.next();
        assert_eq!(iter.size_hint(), (2, Some(2)));

        iter.next();
        assert_eq!(iter.size_hint(), (1, Some(1)));

        iter.next();
        assert_eq!(iter.size_hint(), (0, Some(0)));
    }

    #[test]
    fn test_display() {
        let range = MaxBlockRange::new(2000);
        assert_eq!(format!("{}", range), "2000 blocks");
    }

    #[test]
    fn test_serialization() {
        let range = MaxBlockRange::new(2000);
        let json = serde_json::to_string(&range).unwrap();
        let deserialized: MaxBlockRange = serde_json::from_str(&json).unwrap();
        assert_eq!(range, deserialized);
    }

    #[test]
    fn test_conversions() {
        let u64_val = 2000u64;
        let range: MaxBlockRange = u64_val.into();
        let back: u64 = range.as_u64();
        assert_eq!(u64_val, back);
    }

    #[test]
    fn test_ordering() {
        let small = MaxBlockRange::CONSERVATIVE;
        let medium = MaxBlockRange::DEFAULT;
        let large = MaxBlockRange::GENEROUS;

        assert!(small < medium);
        assert!(medium < large);
        assert!(small < large);
    }

    #[test]
    fn test_real_world_scenario() {
        // Simulate scanning 1 day of blocks on Arbitrum (≈7200 blocks per hour × 24 hours)
        let daily_blocks = 7200 * 24; // ≈172,800 blocks
        let range = MaxBlockRange::MODERATE; // 5000 blocks per query

        let chunks: Vec<_> = range.chunk_range(1000000, 1000000 + daily_blocks).collect();

        // Should need about 35 chunks (172800 / 5000 ≈ 34.56)
        assert_eq!(chunks.len(), 35);

        // Verify first and last chunk
        assert_eq!(chunks[0].0, 1000000);
        assert_eq!(chunks[34].1, 1000000 + daily_blocks);

        // Verify no gaps between chunks
        for i in 0..chunks.len() - 1 {
            assert_eq!(chunks[i].1 + 1, chunks[i + 1].0);
        }
    }
}
