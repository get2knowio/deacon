//! Unified caching abstraction for features, config, and probe results
//!
//! This module provides a multi-level cache implementation supporting both
//! in-memory and disk-based caching with TTL and LRU eviction policies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod disk;
mod keys;
mod memory;
mod multilevel;

pub use disk::DiskCache;
pub use keys::{ConfigCacheKey, FeatureCacheKey, ProbeCacheKey};
pub use memory::InMemoryCache;
pub use multilevel::MultiLevelCache;

/// Generic cache trait for storing and retrieving values
pub trait Cache<K, V> {
    /// Store a value in the cache with the given key
    fn set(&mut self, key: K, value: V) -> Result<()>;

    /// Retrieve a value from the cache by key
    fn get(&mut self, key: &K) -> Option<V>;

    /// Remove a value from the cache by key
    fn remove(&mut self, key: &K) -> Option<V>;

    /// Clear all entries from the cache
    fn clear(&mut self);

    /// Get the number of entries in the cache
    fn len(&self) -> usize;

    /// Check if the cache is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get cache statistics for debugging
    fn stats(&self) -> CacheStats;
}

/// Cache statistics for monitoring and debugging
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entries: usize,
    pub memory_usage_bytes: usize,
}

impl CacheStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }
}

/// TTL (Time To Live) wrapper for cache entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlEntry<V> {
    pub value: V,
    pub created_at: u64,
    pub ttl_seconds: Option<u64>,
}

impl<V> TtlEntry<V> {
    pub fn new(value: V, ttl: Option<Duration>) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ttl_seconds = ttl.map(|d| d.as_secs());

        Self {
            value,
            created_at,
            ttl_seconds,
        }
    }

    pub fn is_expired(&self) -> bool {
        match self.ttl_seconds {
            Some(ttl) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now > self.created_at + ttl
            }
            None => false,
        }
    }
}

/// Generates a SHA256 hash of the given key for use as a file name
pub(crate) fn hash_key<K: Debug>(key: &K) -> String {
    let key_str = format!("{:?}", key);
    let mut hasher = Sha256::new();
    hasher.update(key_str.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_ttl_entry_creation() {
        let entry = TtlEntry::new("test_value", Some(Duration::from_secs(60)));
        assert_eq!(entry.value, "test_value");
        assert!(entry.ttl_seconds.is_some());
        assert_eq!(entry.ttl_seconds.unwrap(), 60);
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_ttl_entry_no_expiration() {
        let entry = TtlEntry::new("test_value", None);
        assert_eq!(entry.value, "test_value");
        assert!(entry.ttl_seconds.is_none());
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_ttl_entry_expired() {
        let mut entry = TtlEntry::new("test_value", Some(Duration::from_secs(1)));
        // Manually set created_at to past time to simulate expiration
        entry.created_at = 0;
        assert!(entry.is_expired());
    }

    #[test]
    fn test_hash_key() {
        let key1 = "test_key";
        let key2 = "test_key";
        let key3 = "different_key";

        let hash1 = hash_key(&key1);
        let hash2 = hash_key(&key2);
        let hash3 = hash_key(&key3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA256 hex string length
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats::new();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate(), 0.0);

        let stats = CacheStats {
            hits: 7,
            misses: 3,
            evictions: 1,
            entries: 5,
            memory_usage_bytes: 1024,
        };
        assert_eq!(stats.hit_rate(), 0.7);
    }
}
