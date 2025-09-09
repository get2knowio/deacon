//! Multi-level cache implementation combining memory and disk caches

use super::disk::DiskCache;
use super::memory::InMemoryCache;
use super::{Cache, CacheStats};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::hash::Hash;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, trace};

/// Multi-level cache that combines in-memory and disk caches
///
/// This cache implementation first checks the in-memory cache for fast access,
/// then falls back to the disk cache for persistence across sessions.
pub struct MultiLevelCache<K, V> {
    memory_cache: InMemoryCache<K, V>,
    disk_cache: DiskCache<K, V>,
}

impl<K, V> MultiLevelCache<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Debug,
{
    /// Create a new multi-level cache
    ///
    /// # Arguments
    /// * `cache_dir` - Directory for disk cache storage
    /// * `memory_max_size_bytes` - Maximum size for in-memory cache in bytes
    pub fn new<P: AsRef<Path>>(cache_dir: P, memory_max_size_bytes: usize) -> Result<Self> {
        let memory_cache = InMemoryCache::new(memory_max_size_bytes);
        let disk_cache = DiskCache::<K, V>::new(cache_dir)?;

        Ok(Self {
            memory_cache,
            disk_cache,
        })
    }

    /// Create a new multi-level cache with default memory size (10MB)
    pub fn with_default_memory_size<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        Self::new(cache_dir, 10 * 1024 * 1024) // 10MB default
    }

    /// Set a value with TTL in both memory and disk caches
    pub fn set_with_ttl(&mut self, key: K, value: V, ttl: Option<Duration>) -> Result<()> {
        // Store in both caches
        // If memory cache fails due to size constraints, we still want disk storage
        if let Err(e) = self
            .memory_cache
            .set_with_ttl(key.clone(), value.clone(), ttl)
        {
            trace!(
                ?key,
                ?e,
                "Failed to store in memory cache, storing only on disk"
            );
        }

        self.disk_cache.set_with_ttl(key.clone(), value, ttl)?;

        trace!(?key, "Value stored in multi-level cache");
        Ok(())
    }

    /// Promote a value from disk cache to memory cache
    fn promote_to_memory(&mut self, key: &K, value: &V) {
        if let Err(e) = self.memory_cache.set(key.clone(), value.clone()) {
            trace!(?key, ?e, "Failed to promote value to memory cache");
        } else {
            trace!(?key, "Value promoted to memory cache");
        }
    }
}

impl<K, V> Cache<K, V> for MultiLevelCache<K, V>
where
    K: Hash + Eq + Clone + Debug,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Debug,
{
    fn set(&mut self, key: K, value: V) -> Result<()> {
        self.set_with_ttl(key, value, None)
    }

    fn get(&mut self, key: &K) -> Option<V> {
        // First try memory cache
        if let Some(value) = self.memory_cache.get(key) {
            trace!(?key, "Multi-level cache hit (memory)");
            return Some(value);
        }

        // Then try disk cache
        if let Some(value) = self.disk_cache.get(key) {
            // Promote to memory cache for faster future access
            self.promote_to_memory(key, &value);
            trace!(?key, "Multi-level cache hit (disk)");
            return Some(value);
        }

        trace!(?key, "Multi-level cache miss");
        None
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        // Remove from both caches, return the value from whichever had it
        let memory_value = self.memory_cache.remove(key);
        let disk_value = self.disk_cache.remove(key);

        let value = memory_value.or(disk_value);
        if value.is_some() {
            trace!(?key, "Value removed from multi-level cache");
        }
        value
    }

    fn clear(&mut self) {
        let memory_count = self.memory_cache.len();
        let disk_count = self.disk_cache.len();

        self.memory_cache.clear();
        self.disk_cache.clear();

        debug!(
            memory_entries_cleared = memory_count,
            disk_entries_cleared = disk_count,
            "Multi-level cache cleared"
        );
    }

    fn len(&self) -> usize {
        // Return the maximum count between memory and disk
        // (since some entries might exist in both)
        // This is an approximation but gives a sense of total cached data
        std::cmp::max(self.memory_cache.len(), self.disk_cache.len())
    }

    fn stats(&self) -> CacheStats {
        let memory_stats = self.memory_cache.stats();
        let disk_stats = self.disk_cache.stats();

        // Combine stats from both levels
        CacheStats {
            hits: memory_stats.hits + disk_stats.hits,
            misses: memory_stats.misses + disk_stats.misses,
            evictions: memory_stats.evictions + disk_stats.evictions,
            entries: self.len(),
            memory_usage_bytes: memory_stats.memory_usage_bytes + disk_stats.memory_usage_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 1024).unwrap();

        // Test set and get
        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

        // Test miss
        assert_eq!(cache.get(&"nonexistent".to_string()), None);

        // Test remove
        assert_eq!(
            cache.remove(&"key1".to_string()),
            Some("value1".to_string())
        );
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_memory_to_disk_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 100).unwrap(); // Very small memory cache

        // Store a value
        cache.set("key1".to_string(), "value1".to_string()).unwrap();

        // Fill memory cache to capacity to evict the first entry
        for i in 0..10 {
            cache
                .set(format!("key{}", i + 2), format!("value{}", i + 2))
                .unwrap();
        }

        // The original value should still be available from disk
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    }

    #[test]
    fn test_promotion_to_memory() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 1024).unwrap();

        // Store a value
        cache.set("key1".to_string(), "value1".to_string()).unwrap();

        // Clear memory cache but leave disk
        cache.memory_cache.clear();

        // Get should promote from disk to memory
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

        // Subsequent get should be from memory (faster)
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    }

    #[test]
    fn test_persistence_across_instances() {
        let temp_dir = TempDir::new().unwrap();

        // Create cache and add entry
        {
            let mut cache: MultiLevelCache<String, String> =
                MultiLevelCache::new(temp_dir.path(), 1024).unwrap();
            cache.set("key1".to_string(), "value1".to_string()).unwrap();
        }

        // Create new cache instance and verify entry persists
        {
            let mut cache: MultiLevelCache<String, String> =
                MultiLevelCache::new(temp_dir.path(), 1024).unwrap();
            assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        }
    }

    #[test]
    fn test_ttl() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 1024).unwrap();

        // Test that non-TTL entries don't expire
        cache
            .set("key_no_ttl".to_string(), "value".to_string())
            .unwrap();
        assert_eq!(
            cache.get(&"key_no_ttl".to_string()),
            Some("value".to_string())
        );

        // Test that TTL entries work for valid duration
        cache
            .set_with_ttl(
                "key_ttl".to_string(),
                "value_ttl".to_string(),
                Some(Duration::from_secs(3600)),
            )
            .unwrap();
        assert_eq!(
            cache.get(&"key_ttl".to_string()),
            Some("value_ttl".to_string())
        );
    }

    #[test]
    fn test_clear() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 1024).unwrap();

        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        cache.set("key2".to_string(), "value2".to_string()).unwrap();

        cache.clear();
        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.get(&"key2".to_string()), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_combined_stats() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: MultiLevelCache<String, String> =
            MultiLevelCache::new(temp_dir.path(), 1024).unwrap();

        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        cache.get(&"key1".to_string()); // hit
        cache.get(&"nonexistent".to_string()); // miss

        let stats = cache.stats();
        assert!(stats.hits > 0);
        assert!(stats.misses > 0);
        assert!(stats.entries > 0);
        assert!(stats.memory_usage_bytes > 0);
    }
}
