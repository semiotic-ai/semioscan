//! Strong types for cache metadata
//!
//! This module provides type-safe wrappers for cache-related metadata:
//!
//! - [`TimestampMillis`]: Unix timestamp in milliseconds for high-precision cache ordering
//! - [`AccessSequence`]: Monotonic sequence number for deterministic LRU ordering

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Unix timestamp in milliseconds for high-precision cache ordering
///
/// Uses milliseconds instead of seconds to ensure unique ordering even for
/// entries created in rapid succession. This is particularly important for
/// disk cache eviction where we need to reliably identify the oldest entry.
///
/// # Examples
///
/// ```
/// use semioscan::TimestampMillis;
/// use std::time::Duration;
///
/// let ts = TimestampMillis::now();
/// std::thread::sleep(Duration::from_millis(10));
/// let age = ts.age_since_now();
/// assert!(age >= Duration::from_millis(10));
/// assert!(age < Duration::from_secs(1));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampMillis(u128);

impl TimestampMillis {
    /// Creates a new timestamp representing the current time
    pub fn now() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self(millis)
    }

    /// Creates a timestamp from a raw millisecond value
    ///
    /// This is primarily used for deserialization and testing.
    #[cfg(test)]
    pub(crate) fn from_millis(millis: u128) -> Self {
        Self(millis)
    }

    /// Calculates the age of this timestamp relative to now
    ///
    /// Returns the duration between this timestamp and the current time.
    /// If this timestamp is in the future, returns zero duration.
    pub fn age_since_now(&self) -> Duration {
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let age_millis = now_millis.saturating_sub(self.0);
        Duration::from_millis(age_millis as u64)
    }

    /// Checks if this timestamp is older than the given duration
    ///
    /// # Examples
    ///
    /// ```
    /// use semioscan::TimestampMillis;
    /// use std::time::Duration;
    ///
    /// let ts = TimestampMillis::now();
    /// std::thread::sleep(Duration::from_millis(10));
    /// assert!(ts.is_older_than(Duration::from_millis(5)));
    /// assert!(!ts.is_older_than(Duration::from_secs(10)));
    /// ```
    pub fn is_older_than(&self, duration: Duration) -> bool {
        self.age_since_now() > duration
    }
}

impl Default for TimestampMillis {
    fn default() -> Self {
        Self::now()
    }
}

/// Monotonic sequence number for deterministic LRU ordering
///
/// When multiple cache entries have the same timestamp (e.g., created in the
/// same millisecond), this sequence number provides a deterministic tie-breaker
/// for LRU eviction. Lower sequence numbers are considered older.
///
/// # Examples
///
/// ```
/// use semioscan::AccessSequence;
///
/// let seq1 = AccessSequence::default();
/// let seq2 = seq1.next();
/// assert!(seq1 < seq2);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct AccessSequence(u64);

impl AccessSequence {
    /// Returns the next sequence number
    ///
    /// This is used by cache implementations to generate monotonically
    /// increasing sequence numbers for access tracking.
    pub fn next(&self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_millis_ordering() {
        let t1 = TimestampMillis::from_millis(1000);
        let t2 = TimestampMillis::from_millis(2000);
        assert!(t1 < t2);
        assert!(t2 > t1);
        assert_eq!(t1, t1);
    }

    #[test]
    fn timestamp_millis_age() {
        let past = TimestampMillis::from_millis(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                - 5000,
        );

        let age = past.age_since_now();
        assert!(age >= Duration::from_millis(5000));
        assert!(age < Duration::from_millis(6000));
    }

    #[test]
    fn timestamp_millis_age_future() {
        let future = TimestampMillis::from_millis(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                + 5000,
        );

        // Future timestamps should return zero age (saturating_sub behavior)
        let age = future.age_since_now();
        assert_eq!(age, Duration::ZERO);
    }

    #[test]
    fn timestamp_millis_is_older_than() {
        let past = TimestampMillis::from_millis(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                - 5000,
        );

        assert!(past.is_older_than(Duration::from_millis(4000)));
        assert!(!past.is_older_than(Duration::from_millis(6000)));
    }

    #[test]
    fn timestamp_millis_now() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let ts = TimestampMillis::now();

        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        assert!(ts.0 >= before);
        assert!(ts.0 <= after);
    }

    #[test]
    fn access_sequence_ordering() {
        let seq1 = AccessSequence(100);
        let seq2 = AccessSequence(200);
        assert!(seq1 < seq2);
        assert!(seq2 > seq1);
        assert_eq!(seq1, seq1);
    }

    #[test]
    fn access_sequence_next() {
        let seq = AccessSequence(5);
        let next = seq.next();
        assert_eq!(next.0, 6);
    }

    #[test]
    fn access_sequence_next_saturating() {
        let seq = AccessSequence(u64::MAX);
        let next = seq.next();
        assert_eq!(next.0, u64::MAX); // Should saturate, not overflow
    }

    #[test]
    fn access_sequence_default() {
        let seq = AccessSequence::default();
        assert_eq!(seq.0, 0);
    }

    #[test]
    fn timestamp_millis_default() {
        let ts = TimestampMillis::default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        // Default should be close to now (within 1 second)
        let diff = ts.0.abs_diff(now);
        assert!(diff < 1000);
    }

    #[test]
    fn timestamp_millis_serialization() {
        let ts = TimestampMillis::from_millis(1234567890);
        let json = serde_json::to_string(&ts).unwrap();
        assert_eq!(json, "1234567890");

        let deserialized: TimestampMillis = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, ts);
    }
}
