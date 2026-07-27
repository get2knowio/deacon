//! Disk-based cache implementation with TTL support

use super::{Cache, CacheStats, Result, TtlEntry, hash_key};
use crate::errors::CacheError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{debug, trace, warn};

/// Sibling lock file guarding the whole `index.json` read-modify-write.
///
/// Mirrors `port_forward::registry`: an advisory `fs2` `flock` held across the
/// *sequence*, not just the publish. `flock` is attached to the open file
/// description, so independent `File::create` calls contend correctly both
/// across processes and across threads of one process, and the kernel releases
/// it if the holder dies.
const INDEX_LOCK_FILE: &str = "index.lock";

/// Name of the published index.
const INDEX_FILE: &str = "index.json";

/// How long [`DiskCache::lock_index`] waits for a contended index lock.
///
/// The wait is BOUNDED on purpose. A plain `flock(LOCK_EX)` blocks forever, and these
/// calls are reached from `async fn` command paths (`up`, `down`, `exec`) whose futures
/// run on tokio worker threads — a peer holding the lock would stall every other future
/// on that runtime, exactly the blocking-in-async pattern the constitution forbids. The
/// bound converts an indefinite stall into a loud, attributable error.
const INDEX_LOCK_TIMEOUT: Duration = Duration::from_secs(10);

/// Poll interval while the index lock is contended.
const INDEX_LOCK_POLL: Duration = Duration::from_millis(20);

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

    /// Acquire the advisory lock guarding the index read-modify-write, waiting at most
    /// [`INDEX_LOCK_TIMEOUT`].
    ///
    /// The lock auto-releases when the returned handle is dropped (or the holder
    /// dies), which is the crash-safety a host-shared index needs.
    ///
    /// Deliberately `try_lock` + bounded poll rather than the blocking
    /// `FileExt::lock_exclusive`: see [`INDEX_LOCK_TIMEOUT`].
    fn lock_index(&self) -> Result<fs::File> {
        let lock_path = self.cache_dir.join(INDEX_LOCK_FILE);
        // `create_new(false)` + `truncate(false)`: the lock file carries no content, and
        // truncating it would be a pointless write on a directory that may be shared.
        let file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(cache_io(format!(
                "Failed to create cache index lock: {:?}",
                lock_path
            )))?;

        let deadline = std::time::Instant::now() + INDEX_LOCK_TIMEOUT;
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(file),
                Err(e) if e.kind() == fs2::lock_contended_error().kind() => {
                    if std::time::Instant::now() >= deadline {
                        return Err(CacheError::Io {
                            message: format!(
                                "Timed out after {:?} waiting for the cache index lock: {:?}",
                                INDEX_LOCK_TIMEOUT, lock_path
                            ),
                            source: std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "cache index lock contended",
                            ),
                        });
                    }
                    std::thread::sleep(INDEX_LOCK_POLL);
                }
                Err(e) => {
                    return Err(cache_io(format!(
                        "Failed to acquire cache index lock: {:?}",
                        lock_path
                    ))(e));
                }
            }
        }
    }

    /// Read the published index from disk. Absent (or empty) means "no entries".
    fn read_index(&self) -> Result<HashMap<String, CacheMetadata>> {
        let metadata_file = self.cache_dir.join(INDEX_FILE);
        match fs::read_to_string(&metadata_file) {
            Ok(content) if content.trim().is_empty() => Ok(HashMap::new()),
            Ok(content) => serde_json::from_str(&content)
                .map_err(cache_serde_json("Failed to parse cache index")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(cache_io(format!(
                "Failed to read cache index: {:?}",
                metadata_file
            ))(e)),
        }
    }

    /// Adopt the on-disk index, discarding this instance's in-memory copy.
    ///
    /// SHOULD be called while holding [`Self::lock_index`]. Every mutation re-reads
    /// first, because the in-memory map is a snapshot taken at construction and
    /// republishing it wholesale would delete every key another process has
    /// written since. Re-reading is what makes the read-modify-write correct; the lock
    /// only makes it *atomic*, so a caller that cannot take the lock still re-reads
    /// (a narrowed race beats a guaranteed lost update).
    ///
    /// Note this does NOT sweep expired entries: the sweep is a whole-map pass that
    /// stats and unlinks files, and paying it on every `set` would serialize N
    /// independent cache writes behind N full sweeps. It runs once, at construction.
    fn adopt_index(&mut self) -> Result<()> {
        self.index = self.read_index()?;
        Ok(())
    }

    /// Load the index of existing cache entries, sweeping expired ones.
    ///
    /// The lock is best-effort here, unlike on the mutation paths: a cache directory we
    /// cannot create a lock file in (read-only mount, another uid's `~/.deacon/state`)
    /// is a legitimate read-only cache, and failing construction would take `up`/`down`/
    /// `exec` down with it. Reads are safe unlocked regardless — [`Self::save_index`]
    /// publishes by rename, so a reader always observes a whole index. Only the
    /// destructive sweep needs the lock, and it is skipped when the lock is unavailable.
    fn load_index(&mut self) -> Result<()> {
        match self.lock_index() {
            Ok(_guard) => {
                self.adopt_index()?;
                // Publish under the SAME lock that performed the sweep. The sweep
                // unlinks each expired entry's data file; leaving the index naming
                // those files makes every later reader log "Cache data file missing"
                // until some unrelated `set` happens to republish.
                if self.cleanup_expired_entries()? {
                    self.save_index()?;
                }
            }
            Err(e) => {
                debug!(
                    ?e,
                    "Cache index lock unavailable; loading read-only without the expiry sweep"
                );
                self.adopt_index()?;
            }
        }
        debug!(entries = self.index.len(), "Loaded cache index");
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
    ///
    /// Atomicity of the *publish* is necessary but not sufficient: the caller MUST
    /// hold [`Self::lock_index`] and have re-read the index under it, or a
    /// concurrent writer's entries are lost to the read-modify-write.
    fn save_index(&self) -> Result<()> {
        let metadata_file = self.cache_dir.join(INDEX_FILE);
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

    /// Remove expired entries from disk and index.
    ///
    /// Returns whether anything was removed, so the caller can republish the index it
    /// just invalidated (the sweep unlinks data files; an unpublished sweep leaves the
    /// index naming paths that no longer exist).
    fn cleanup_expired_entries(&mut self) -> Result<bool> {
        let mut expired_keys = Vec::new();

        for (key_hash, metadata) in &self.index {
            if self.is_metadata_expired(metadata) {
                expired_keys.push(key_hash.clone());
            }
        }

        let swept = !expired_keys.is_empty();
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

        Ok(swept)
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

        // The index is shared by every process using this cache directory, so the
        // whole read-modify-write is serialized: take the lock, adopt what other
        // writers have published, apply only OUR key, republish. Mutating the
        // in-memory snapshot instead would silently drop every entry written since
        // this instance was constructed.
        let _guard = self.lock_index()?;
        self.adopt_index()?;
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

        // Serialize the whole read-modify-write (see `set_with_ttl`). The `Cache` trait
        // has no fallible removal, so a lock failure degrades rather than aborting —
        // but it degrades to an UNLOCKED re-read, never to the construction-time
        // in-memory snapshot. Skipping the re-read and still reaching `save_index`
        // below would republish that snapshot over the live index, deleting every key
        // another process has written since: the exact lost update the lock exists to
        // prevent, on the failure path.
        let _guard = match self.lock_index() {
            Ok(g) => Some(g),
            Err(e) => {
                warn!(
                    ?e,
                    "Failed to lock cache index for removal; proceeding unlocked"
                );
                None
            }
        };
        if let Err(e) = self.adopt_index() {
            warn!(?e, "Failed to re-read cache index before removal");
        }

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
        // Clearing must see every writer's entries, or the data files another
        // process published outlive the index that named them. As in `remove`, a lock
        // failure degrades to an unlocked re-read — never to republishing the stale
        // in-memory snapshot.
        let _guard = match self.lock_index() {
            Ok(g) => Some(g),
            Err(e) => {
                warn!(
                    ?e,
                    "Failed to lock cache index for clear; proceeding unlocked"
                );
                None
            }
        };
        if let Err(e) = self.adopt_index() {
            warn!(?e, "Failed to re-read cache index before clear");
        }

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
    fn test_concurrent_writers_do_not_lose_each_others_entries() {
        // Regression: the index is a read-modify-write over a SHARED file. Each
        // `DiskCache` loads `index.json` into memory at construction and republishes
        // the WHOLE map on every mutation. Without a lock held across the whole
        // sequence, a losing writer's entry is silently clobbered (classic lost
        // update) even though each individual publish is atomic — which is how a
        // `deacon down` for one workspace could report "no containers found" after a
        // concurrent `up` for a different workspace overwrote its entry.
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path().to_path_buf();
        const THREADS: usize = 8;
        const PER_THREAD: usize = 25;

        let handles: Vec<_> = (0..THREADS)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    for i in 0..PER_THREAD {
                        // A fresh cache per write mirrors separate `deacon`
                        // PROCESSES, each of which loads the index, mutates one
                        // key and exits.
                        let mut cache: DiskCache<String, String> = DiskCache::new(&dir).unwrap();
                        cache.set(format!("t{t}-k{i}"), format!("v{i}")).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let mut cache: DiskCache<String, String> = DiskCache::new(&dir).unwrap();
        let missing: Vec<String> = (0..THREADS)
            .flat_map(|t| (0..PER_THREAD).map(move |i| format!("t{t}-k{i}")))
            .filter(|k| cache.get(k).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "{} of {} entries were lost to the index read-modify-write race: {:?}",
            missing.len(),
            THREADS * PER_THREAD,
            &missing[..missing.len().min(10)]
        );
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

    /// A cache directory we cannot create the lock file in is a legitimate READ-ONLY
    /// cache (a read-only mount, another uid's `~/.deacon/state`), not a fatal error.
    /// `lock_index` unconditionally created the lock file, so construction failed and
    /// took `up`/`down`/`exec` down with it via `StateManager::new()?`.
    #[cfg(unix)]
    #[test]
    fn read_only_cache_dir_still_constructs_and_reads() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        {
            let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
            cache.set("key1".to_string(), "value1".to_string()).unwrap();
        }
        // Drop write permission on the directory: the lock file can no longer be created.
        let mut perms = fs::metadata(temp_dir.path()).unwrap().permissions();
        let original = perms.mode();
        perms.set_mode(0o555);
        fs::set_permissions(temp_dir.path(), perms).unwrap();

        let constructed: Result<DiskCache<String, String>> = DiskCache::new(temp_dir.path());

        // Restore before asserting so the TempDir can always clean itself up.
        let mut perms = fs::metadata(temp_dir.path()).unwrap().permissions();
        perms.set_mode(original);
        fs::set_permissions(temp_dir.path(), perms).unwrap();

        let mut cache = constructed.expect("a read-only cache dir must not fail construction");
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
    }

    /// The construction-time sweep unlinks expired data files; leaving the index naming
    /// them made every later reader log "Cache data file missing" until some unrelated
    /// `set` happened to republish. The sweep must be published under the same lock that
    /// performed it.
    #[test]
    fn expiry_sweep_is_published_to_the_index() {
        let temp_dir = TempDir::new().unwrap();
        {
            let mut cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
            cache
                .set_with_ttl(
                    "gone".to_string(),
                    "value".to_string(),
                    Some(Duration::from_secs(1)),
                )
                .unwrap();
            cache.set("kept".to_string(), "value".to_string()).unwrap();
        }
        // Age the TTL'd entry deterministically rather than sleeping past it: TTLs have
        // one-second resolution, so a real sleep would have to exceed a second to be
        // reliable.
        {
            let index_path = temp_dir.path().join(INDEX_FILE);
            let mut index: HashMap<String, CacheMetadata> =
                serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
            for metadata in index.values_mut() {
                if metadata.ttl_seconds.is_some() {
                    metadata.created_at -= 3600;
                }
            }
            fs::write(&index_path, serde_json::to_string(&index).unwrap()).unwrap();
        }

        // Constructing sweeps the expired entry...
        let _cache: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();

        // ...and the PUBLISHED index must no longer name it.
        let published: HashMap<String, CacheMetadata> =
            serde_json::from_str(&fs::read_to_string(temp_dir.path().join(INDEX_FILE)).unwrap())
                .unwrap();
        assert_eq!(
            published.len(),
            1,
            "the expired entry must be swept from the published index, not just from memory"
        );
    }

    /// `remove`/`clear` degrade to an UNLOCKED re-read, never to republishing the
    /// construction-time in-memory snapshot. Skipping the re-read and still saving would
    /// delete every key another writer published since — the exact lost update the lock
    /// exists to prevent, on the failure path.
    #[test]
    fn remove_preserves_entries_another_writer_published() {
        let temp_dir = TempDir::new().unwrap();
        let mut first: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
        first.set("a".to_string(), "1".to_string()).unwrap();

        // A second handle (standing in for another process) publishes a key the first
        // handle's in-memory snapshot has never seen.
        let mut second: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
        second.set("b".to_string(), "2".to_string()).unwrap();

        first.remove(&"a".to_string());

        let mut reader: DiskCache<String, String> = DiskCache::new(temp_dir.path()).unwrap();
        assert_eq!(reader.get(&"a".to_string()), None, "the removal must apply");
        assert_eq!(
            reader.get(&"b".to_string()),
            Some("2".to_string()),
            "the concurrent writer's key must survive the removal"
        );
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
