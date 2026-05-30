//! Disk-based cache implementation with TTL support

use super::{Cache, CacheStats, Result, TtlEntry, hash_key};
use crate::errors::CacheError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, trace, warn};

fn cache_io<S: Into<String>>(message: S) -> impl FnOnce(std::io::Error) -> CacheError {
    let message = message.into();
    move |source| CacheError::Io { message, source }
}

fn cache_serialize<S: Into<String>>(message: S) -> impl FnOnce(postcard::Error) -> CacheError {
    let message = message.into();
    move |source| CacheError::Serialization {
        message: format!("{}: {}", message, source),
    }
}

fn cache_serde_json<S: Into<String>>(message: S) -> impl FnOnce(serde_json::Error) -> CacheError {
    let message = message.into();
    move |source| CacheError::Serialization {
        message: format!("{}: {}", message, source),
    }
}

/// Disk-based cache that stores entries as files
pub struct DiskCache<K, V> {
    cache_dir: PathBuf,
    stats: CacheStats,
    /// In-memory index for faster lookups and stats tracking
    index: HashMap<String, CacheMetadata>,
    /// Phantom data to keep type parameters
    _phantom: PhantomData<(K, V)>,
}

/// Metadata for cache entries stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    /// Path to the actual data file
    data_file: PathBuf,
    /// Size of the data file in bytes
    size_bytes: usize,
    /// When the entry was created
    created_at: u64,
    /// TTL in seconds, if any
    ttl_seconds: Option<u64>,
}

impl<K, V> DiskCache<K, V>
where
    K: Debug + Clone,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Debug,
{
    /// Create a new disk cache in the specified directory
    pub fn new<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();

        // Create cache directory if it doesn't exist
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).map_err(cache_io(format!(
                "Failed to create cache directory: {:?}",
                cache_dir
            )))?;
        }

        let mut cache = Self {
            cache_dir,
            stats: CacheStats::new(),
            index: HashMap::new(),
            _phantom: PhantomData,
        };

        // Load existing metadata
        cache.load_index()?;

        Ok(cache)
    }

    /// Load the index of existing cache entries
    fn load_index(&mut self) -> Result<()> {
        let metadata_file = self.cache_dir.join("index.json");

        if metadata_file.exists() {
            let content = fs::read_to_string(&metadata_file).map_err(cache_io(format!(
                "Failed to read cache index: {:?}",
                metadata_file
            )))?;

            self.index = serde_json::from_str(&content)
                .map_err(cache_serde_json("Failed to parse cache index"))?;

            // Remove expired entries and clean up orphaned files
            self.cleanup_expired_entries()?;

            debug!(entries = self.index.len(), "Loaded cache index");
        }

        Ok(())
    }

    /// Save the current index to disk.
    ///
    /// Writes to a uniquely-named temp file in the same directory and renames it
    /// into place, so the write is atomic. A plain `fs::write` truncates then
    /// streams the new bytes; when two writers (or processes sharing a cache
    /// dir) race, a shorter payload landing over a longer file leaves trailing
    /// bytes — surfacing later as "trailing characters" JSON parse errors. The
    /// rename makes each publish all-or-nothing (last writer wins, always valid).
    fn save_index(&self) -> Result<()> {
        let metadata_file = self.cache_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.index)
            .map_err(cache_serde_json("Failed to serialize cache index"))?;

        // Unique temp name (pid + monotonic counter) so concurrent writers don't
        // clobber each other's staging file before the rename.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_file =
            self.cache_dir
                .join(format!("index.json.tmp.{}.{}", std::process::id(), seq));

        fs::write(&tmp_file, content).map_err(cache_io(format!(
            "Failed to write cache index temp file: {:?}",
            tmp_file
        )))?;

        fs::rename(&tmp_file, &metadata_file).map_err(cache_io(format!(
            "Failed to publish cache index: {:?}",
            metadata_file
        )))?;

        Ok(())
    }

    /// Remove expired entries from disk and index
    fn cleanup_expired_entries(&mut self) -> Result<()> {
        let mut expired_keys = Vec::new();

        for (key_hash, metadata) in &self.index {
            if self.is_metadata_expired(metadata) {
                expired_keys.push(key_hash.clone());
            }
        }

        for key_hash in expired_keys {
            if let Some(metadata) = self.index.remove(&key_hash) {
                // Remove the data file
                if metadata.data_file.exists() {
                    if let Err(e) = fs::remove_file(&metadata.data_file) {
                        warn!(?e, file = ?metadata.data_file, "Failed to remove expired cache file");
                    }
                }
                trace!(key_hash, "Removed expired cache entry");
            }
        }

        Ok(())
    }

    /// Check if metadata indicates an expired entry
    fn is_metadata_expired(&self, metadata: &CacheMetadata) -> bool {
        match metadata.ttl_seconds {
            Some(ttl) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                now > metadata.created_at + ttl
            }
            None => false,
        }
    }

    /// Get the file path for a given key hash
    fn get_data_file_path(&self, key_hash: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.bincode", key_hash))
    }

    /// Set a value with TTL
    pub fn set_with_ttl(&mut self, key: K, value: V, ttl: Option<Duration>) -> Result<()> {
        let key_hash = hash_key(&key);
        let data_file = self.get_data_file_path(&key_hash);

        // Serialize and write the TTL entry
        let ttl_entry = TtlEntry::new(value, ttl);
        let serialized = postcard::to_allocvec(&ttl_entry)
            .map_err(cache_serialize("Failed to serialize cache entry"))?;

        fs::write(&data_file, &serialized).map_err(cache_io(format!(
            "Failed to write cache file: {:?}",
            data_file
        )))?;

        // Update metadata
        let metadata = CacheMetadata {
            data_file: data_file.clone(),
            size_bytes: serialized.len(),
            created_at: ttl_entry.created_at,
            ttl_seconds: ttl_entry.ttl_seconds,
        };

        self.index.insert(key_hash.clone(), metadata);
        self.save_index()?;

        trace!(?key, key_hash, file = ?data_file, size_bytes = serialized.len(), "Cache entry stored to disk");
        Ok(())
    }

    /// Load and deserialize an entry from disk
    fn load_entry(&self, key_hash: &str) -> Result<Option<TtlEntry<V>>> {
        let metadata = match self.index.get(key_hash) {
            Some(meta) => meta,
            None => return Ok(None),
        };

        if self.is_metadata_expired(metadata) {
            return Ok(None);
        }

        if !metadata.data_file.exists() {
            warn!(file = ?metadata.data_file, "Cache data file missing");
            return Ok(None);
        }

        let serialized = fs::read(&metadata.data_file).map_err(cache_io(format!(
            "Failed to read cache file: {:?}",
            metadata.data_file
        )))?;

        let entry: TtlEntry<V> = postcard::from_bytes(&serialized)
            .map_err(cache_serialize("Failed to deserialize cache entry"))?;

        if entry.is_expired() {
            return Ok(None);
        }

        Ok(Some(entry))
    }
}

impl<K, V> Cache<K, V> for DiskCache<K, V>
where
    K: Debug + Clone,
    V: Serialize + for<'de> Deserialize<'de> + Clone + Debug,
{
    fn set(&mut self, key: K, value: V) -> Result<()> {
        self.set_with_ttl(key, value, None)
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let key_hash = hash_key(key);

        match self.load_entry(&key_hash) {
            Ok(Some(entry)) => {
                self.stats.hits += 1;
                trace!(?key, key_hash, "Disk cache hit");
                Some(entry.value)
            }
            Ok(None) => {
                self.stats.misses += 1;
                trace!(?key, key_hash, "Disk cache miss");
                None
            }
            Err(e) => {
                warn!(?e, ?key, key_hash, "Failed to load cache entry");
                self.stats.misses += 1;
                None
            }
        }
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        let key_hash = hash_key(key);

        // Get the value before removal
        let value = match self.load_entry(&key_hash) {
            Ok(Some(entry)) => Some(entry.value),
            _ => None,
        };

        // Remove from index and disk
        if let Some(metadata) = self.index.remove(&key_hash) {
            if metadata.data_file.exists() {
                if let Err(e) = fs::remove_file(&metadata.data_file) {
                    warn!(?e, file = ?metadata.data_file, "Failed to remove cache file");
                }
            }

            if let Err(e) = self.save_index() {
                warn!(?e, "Failed to save index after removal");
            }

            trace!(?key, key_hash, "Cache entry removed from disk");
        }

        value
    }

    fn clear(&mut self) {
        let count = self.index.len();

        // Remove all data files
        for metadata in self.index.values() {
            if metadata.data_file.exists() {
                if let Err(e) = fs::remove_file(&metadata.data_file) {
                    warn!(?e, file = ?metadata.data_file, "Failed to remove cache file during clear");
                }
            }
        }

        // Clear index
        self.index.clear();

        // Save empty index
        if let Err(e) = self.save_index() {
            warn!(?e, "Failed to save empty index after clear");
        }

        debug!(entries_cleared = count, "Disk cache cleared");
    }

    fn len(&self) -> usize {
        // Clean up expired entries during len check
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.index
            .values()
            .filter(|metadata| match metadata.ttl_seconds {
                Some(ttl) => current_time <= metadata.created_at + ttl,
                None => true,
            })
            .count()
    }

    fn stats(&self) -> CacheStats {
        let total_size: usize = self
            .index
            .values()
            .map(|metadata| metadata.size_bytes)
            .sum();

        CacheStats {
            hits: self.stats.hits,
            misses: self.stats.misses,
            evictions: self.stats.evictions,
            entries: self.len(),
            memory_usage_bytes: total_size,
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
        let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();

        // Test set and get
        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(cache.len(), 1);

        // Test miss
        assert_eq!(cache.get(&"nonexistent".to_string()), None);

        // Test remove
        assert_eq!(
            cache.remove(&"key1".to_string()),
            Some("value1".to_string())
        );
        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_save_index_is_atomic_and_leaves_no_temp_files() {
        let temp_dir = TempDir::new().unwrap();
        {
            let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
            for i in 0..20 {
                cache.set(format!("key{i}"), format!("value{i}")).unwrap();
            }
        }
        // The published index must be valid JSON and the staging temp files must
        // have been renamed away (no `index.json.tmp.*` left behind).
        let index = temp_dir.path().join("index.json");
        let content = std::fs::read_to_string(&index).unwrap();
        serde_json::from_str::<serde_json::Value>(&content)
            .expect("published index.json must be valid JSON");
        let leftover_tmp: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("index.json.tmp.")
            })
            .collect();
        assert!(
            leftover_tmp.is_empty(),
            "no staging temp files should remain, found: {:?}",
            leftover_tmp
        );
    }

    #[test]
    fn test_concurrent_writers_keep_index_parseable() {
        // Reproduces the "trailing characters" corruption: multiple writers
        // sharing a cache dir must never leave a half-overwritten index.json.
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path().to_path_buf();
        let handles: Vec<_> = (0..8)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    let mut cache: DiskCache<String, String> = DiskCache::new(&dir).unwrap();
                    for i in 0..50 {
                        cache.set(format!("t{t}-k{i}"), format!("v{i}")).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // After all the racing writers, the index must still parse cleanly and a
        // fresh cache must load without a serialization error.
        let content = std::fs::read_to_string(dir.join("index.json")).unwrap();
        serde_json::from_str::<serde_json::Value>(&content)
            .expect("index.json must remain valid JSON under concurrent writers");
        let _cache: DiskCache<String, String> =
            DiskCache::new(&dir).expect("reload must not fail to parse the index");
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();

        // Create cache and add entry
        {
            let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
            cache.set("key1".to_string(), "value1".to_string()).unwrap();
        }

        // Create new cache instance and verify entry persists
        {
            let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
            assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
            assert_eq!(cache.len(), 1);
        }
    }

    #[test]
    fn test_ttl() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();

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
        let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();

        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        cache.set("key2".to_string(), "value2".to_string()).unwrap();
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.get(&"key2".to_string()), None);
    }

    #[test]
    fn test_stats() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.entries, 0);

        cache.set("key1".to_string(), "value1".to_string()).unwrap();
        cache.get(&"key1".to_string()); // hit
        cache.get(&"nonexistent".to_string()); // miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entries, 1);
        assert!(stats.memory_usage_bytes > 0);
        assert_eq!(stats.hit_rate(), 0.5);
    }
}
