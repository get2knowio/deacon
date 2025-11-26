# Quickstart: Using Env-Probe Cache

**Feature**: 001-010-env-probe  
**Audience**: Deacon developers implementing or extending cache functionality

## Overview

This guide explains how to use and extend the env-probe caching feature for container environment probing. The cache dramatically reduces `deacon up` latency by storing container shell environments on disk.

---

## For End Users

### Basic Usage

Enable caching by providing the `--container-data-folder` flag:

```bash
# First run: Creates cache
deacon up --container-data-folder=/tmp/deacon-cache

# Second run: Uses cache (50%+ faster)
deacon up --container-data-folder=/tmp/deacon-cache
```

### Cache Location

Cache files are stored at:
```
{cache_folder}/env_probe_{container_id}_{user}.json
```

Example:
```
/tmp/deacon-cache/env_probe_abc123def456_vscode.json
```

### Inspecting Cache

View cache contents:
```bash
cat /tmp/deacon-cache/env_probe_*.json | jq
```

### Clearing Cache

Delete cache files manually:
```bash
rm -rf /tmp/deacon-cache/env_probe_*.json
```

Or delete entire cache folder:
```bash
rm -rf /tmp/deacon-cache
```

### Debugging Cache Behavior

Enable DEBUG logging to see cache operations:
```bash
RUST_LOG=debug deacon up --container-data-folder=/tmp/cache
```

Look for log messages:
- `Loaded cached env probe` - Cache hit
- `Cache miss: executing fresh probe` - Cache miss
- `Persisted env probe cache` - Cache written
- `Failed to read cache file` - Cache error

---

## For Deacon Developers

### Architecture Overview

```
CLI Flag (--container-data-folder)
  ↓
UpArgs/ExecArgs.cache_folder: Option<PathBuf>
  ↓
resolve_env_and_user(cache_folder: Option<&Path>)
  ↓
probe_container_environment(cache_folder: Option<&Path>)
  ↓
[Check cache] → [Load OR Execute] → [Write cache]
  ↓
ContainerProbeResult { env_vars, shell_used, var_count }
```

### Adding Cache Support to New Subcommand

**Step 1**: Add cache_folder field to Args struct

```rust
pub struct MySubcommandArgs {
    // ... existing fields
    pub cache_folder: Option<PathBuf>,
}
```

**Step 2**: Add CLI flag (if not inherited from global)

```rust
#[arg(long, value_name = "PATH")]
pub container_data_folder: Option<PathBuf>,
```

**Step 3**: Pass to shared helper

```rust
let env_user = resolve_env_and_user(
    &docker_client,
    &container_id,
    cli_user,
    config_remote_user,
    probe_mode,
    config_remote_env,
    &cli_env_map,
    args.cache_folder.as_deref(),  // ← Pass cache folder
).await;
```

**Step 4**: Add to ContainerLifecycleConfig (if using lifecycle)

```rust
let lifecycle_config = ContainerLifecycleConfig {
    container_id: container_id.clone(),
    user: effective_user.clone(),
    // ... other fields
    cache_folder: args.cache_folder.clone(),  // ← Add this
};
```

**Step 5**: Update Default impl

```rust
impl Default for MySubcommandArgs {
    fn default() -> Self {
        Self {
            // ... existing fields
            cache_folder: None,  // ← Add this
        }
    }
}
```

### Testing Cache Behavior

**Integration Test Template**:

```rust
#[tokio::test]
async fn test_cache_hit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let cache_folder = temp_dir.path();
    
    let prober = ContainerEnvironmentProber::new();
    
    // First probe: cache miss
    let result1 = prober.probe_container_environment(
        &docker,
        "test_container",
        ContainerProbeMode::LoginShell,
        Some("vscode"),
        Some(cache_folder),
    ).await.unwrap();
    
    assert_eq!(result1.shell_used, "/bin/bash");  // Fresh probe
    
    // Second probe: cache hit
    let result2 = prober.probe_container_environment(
        &docker,
        "test_container",
        ContainerProbeMode::LoginShell,
        Some("vscode"),
        Some(cache_folder),
    ).await.unwrap();
    
    assert_eq!(result2.shell_used, "cache");  // Loaded from cache
    assert_eq!(result1.env_vars, result2.env_vars);  // Same data
}
```

### Debugging Common Issues

**Issue**: Cache not being written

**Diagnosis**:
```bash
RUST_LOG=debug deacon up --container-data-folder=/tmp/cache 2>&1 | grep cache
```

**Possible causes**:
- Cache folder permissions (chmod 755)
- Disk full
- cache_folder parameter not passed through call chain

**Fix**: Check logs for WARN messages, verify file system permissions

---

**Issue**: Cache not being loaded

**Diagnosis**:
```bash
ls -la /tmp/cache/env_probe_*.json
cat /tmp/cache/env_probe_*.json | jq  # Verify valid JSON
```

**Possible causes**:
- Container ID changed (rebuild invalidates cache)
- User changed (cache is per-user)
- Corrupted JSON file

**Fix**: Delete corrupted cache file, verify container ID matches

---

**Issue**: Compilation error: missing cache_folder field

**Example**:
```
error[E0063]: missing field `cache_folder` in initializer of `UpArgs`
```

**Fix**: Add `cache_folder: None,` to struct initializer:
```rust
Self {
    // ... existing fields
    cache_folder: None,  // ← Add this line
}
```

---

### Code References

**Core caching logic**:
- `crates/core/src/container_env_probe.rs` - Cache read/write logic
- Lines 147-164: Cache loading
- Lines 186-194: Cache persistence

**Shared helper**:
- `crates/deacon/src/commands/shared/env_user.rs` - `resolve_env_and_user()`

**Example subcommands**:
- `crates/deacon/src/commands/up.rs` - Complete implementation
- `crates/deacon/src/commands/exec.rs` - Complete implementation

**Tests**:
- `crates/core/tests/integration_env_probe_cache.rs` - Integration tests

---

## Performance Benchmarking

### Measuring Cache Impact

**Test script**:
```bash
#!/bin/bash

CACHE_DIR="/tmp/deacon-cache"
rm -rf $CACHE_DIR  # Clear cache

# Measure first run (cache miss)
time deacon up --container-data-folder=$CACHE_DIR

# Measure second run (cache hit)
time deacon up --container-data-folder=$CACHE_DIR
```

**Expected results**:
- First run: 2-5 seconds (includes env probe ~500ms)
- Second run: 0.5-2 seconds (cache read ~10ms)
- Speedup: 50-75% faster

### Profiling Cache Operations

**Enable tracing**:
```bash
RUST_LOG=trace deacon up --container-data-folder=/tmp/cache
```

**Look for timing spans**:
- `probe_container_environment` - Total probe time
- `detect_container_shell` - Shell detection time
- `execute_probe_in_container` - Shell execution time

---

## Best Practices

### ✅ DO

- Use cache for repeated development iterations
- Clean cache when environment changes significantly
- Enable DEBUG logging when investigating cache issues
- Pass `cache_folder` through shared helpers (don't reimplement)

### ❌ DON'T

- Don't rely on cache for CI/CD (use fresh probe)
- Don't share cache folders across different container images
- Don't manually edit cache files (invalidation is automatic)
- Don't implement per-subcommand cache logic (use shared helper)

---

## FAQ

**Q: When is cache invalidated?**  
A: Only when container ID changes (rebuild). No time-based expiration.

**Q: Can I disable caching?**  
A: Yes, omit `--container-data-folder` flag.

**Q: What happens if cache is corrupted?**  
A: System logs warning and falls back to fresh probe automatically.

**Q: Can multiple users share cache folder?**  
A: Yes, each user gets separate cache file (`{container_id}_{user}.json`).

**Q: How do I clean stale cache files?**  
A: Delete manually with `rm`. No automatic cleanup (future enhancement).

**Q: Does cache work across container restarts?**  
A: No, container restart changes container ID, invalidating cache.

**Q: Does cache work across container rebuilds?**  
A: No, rebuild changes container ID, creating new cache entry.

---

## Next Steps

1. **Implement fixes**: See `plan.md` for struct initializer fixes
2. **Add logging**: See `plan.md` for DEBUG log statements
3. **Enhance tests**: See `plan.md` for test scenarios
4. **Run validation**: `make test-nextest-fast` then `make test-nextest`

**Questions?** See `spec.md` for requirements or `data-model.md` for technical details.
