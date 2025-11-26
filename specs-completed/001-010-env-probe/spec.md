# Feature Specification: Env-Probe Cache Completion

**Feature Branch**: `001-010-env-probe`  
**Created**: 2025-11-23  
**Status**: Draft  
**Input**: User description: "Complete and fix env-probe caching for the up subcommand"

## Clarifications

### Session 2025-11-23

- Q: Cache expiration policy (time-based vs container-change-only) → A: No time-based expiration (container ID change only)
- Q: Observability - Cache hit/miss logging → A: Log at DEBUG level: cache hit, cache miss, cache write

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fast Container Startup with Cached Environment (Priority: P1)

A developer runs `deacon up` repeatedly during development. The first run probes the container's shell environment to capture PATH, nvm, and other shell-configured variables. Subsequent runs should reuse the cached probe results, avoiding slow shell initialization on every startup.

**Why this priority**: Core performance improvement - reduces `up` command latency from seconds to milliseconds for repeat invocations. This is the primary value of caching.

**Independent Test**: Can be fully tested by running `deacon up --container-data-folder=/tmp/cache` twice and measuring execution time. The second run should skip env probing and complete faster.

**Acceptance Scenarios**:

1. **Given** no cache exists, **When** user runs `deacon up --container-data-folder=/tmp/cache`, **Then** system probes container environment and persists cache to `/tmp/cache/env_probe_{container_id}_{user}.json`
2. **Given** valid cache exists, **When** user runs `deacon up --container-data-folder=/tmp/cache`, **Then** system loads cached env vars without executing shell probe
3. **Given** cache file exists, **When** user inspects it, **Then** file contains valid JSON with environment variable key-value pairs

---

### User Story 2 - Per-User Cache Isolation (Priority: P2)

A developer switches between different users in the same container (e.g., root vs. non-root user). Each user has different shell configurations and environment variables. The cache should isolate environments per user to avoid incorrect environment reuse.

**Why this priority**: Correctness requirement - prevents subtle bugs from mixed environments. Less critical than P1 because single-user workflows are more common.

**Independent Test**: Run `deacon up --remote-user=alice --container-data-folder=/tmp/cache` then `deacon up --remote-user=bob --container-data-folder=/tmp/cache`. Verify two separate cache files are created.

**Acceptance Scenarios**:

1. **Given** container running as user "alice", **When** env probe completes, **Then** cache is stored with key `{container_id}_alice`
2. **Given** container running as user "bob", **When** env probe completes, **Then** cache is stored with key `{container_id}_bob`
3. **Given** cached env for "alice" exists, **When** user runs as "bob", **Then** system does not reuse alice's cache

---

### User Story 3 - Cache Invalidation on Container Changes (Priority: P3)

A developer rebuilds their container with updated shell configuration (new .bashrc, different PATH). The cache from the old container should not be reused for the new container, even if the user is the same.

**Why this priority**: Data freshness requirement - prevents stale cache bugs. Lower priority because container ID changes naturally invalidate cache (cache key includes container ID).

**Independent Test**: Run `deacon up` with container A (cache created), then delete container and run `deacon up` again (new container B). Verify new cache file is created with different container ID.

**Acceptance Scenarios**:

1. **Given** cache exists for container_id="abc123", **When** new container with id="def456" starts, **Then** system creates new cache entry (does not reuse abc123's cache)
2. **Given** cache folder contains stale entries, **When** user manually deletes cache files, **Then** next `deacon up` regenerates cache correctly

---

### Edge Cases

- What happens when cache folder doesn't exist? System should create it (implemented via `std::fs::create_dir_all`).
- What happens when cache file is corrupted JSON? System should ignore cache and re-probe (current implementation falls back gracefully).
- What happens when `--container-data-folder` is not provided? System should skip caching and probe on every invocation (current behavior).
- What happens when container is running as root (user=None)? System should use "root" as the cache key component.
- What happens to old cache files after container rebuild? Stale cache files accumulate in cache folder; system does not auto-clean them (user responsibility or future enhancement).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST persist env probe results to disk at `{cache_folder}/env_probe_{container_id}_{user}.json` when `container_data_folder` flag is provided
- **FR-002**: System MUST load cached env probe results from disk before executing shell probe, if cache file exists and is valid JSON
- **FR-003**: System MUST isolate cache entries by container ID and user (cache key: `{container_id}_{user}`). Cache has no time-based expiration; invalidation occurs only when container ID changes.
- **FR-004**: System MUST create cache directory if it doesn't exist (via `create_dir_all`)
- **FR-005**: System MUST gracefully handle cache read failures (corrupted JSON, permission errors) by falling back to fresh probe
- **FR-006**: System MUST pass `cache_folder` parameter through the entire call chain: CLI → UpArgs → resolve_env_and_user → probe_container_environment
- **FR-007**: System MUST compile without errors (fix missing `cache_folder` fields in struct initializers)
- **FR-008**: System MUST work correctly when `cache_folder` is None (no caching behavior)
- **FR-009**: System MUST log cache operations at DEBUG level: cache hit (loaded from disk), cache miss (fresh probe required), cache write (persisted to disk)

### Key Entities

- **CacheKey**: Composite identifier `{container_id}_{user}` used for cache file naming
- **CacheFile**: JSON file at `{cache_folder}/env_probe_{cache_key}.json` containing serialized `HashMap<String, String>` of environment variables
- **ContainerProbeResult**: Return type containing `env_vars`, `shell_used`, `var_count` - represents both cached and fresh probe results

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All tests pass with `make test-nextest-fast` (compilation succeeds, no regressions)
- **SC-002**: `make fmt` and `make clippy` pass with zero warnings
- **SC-003**: Integration test `integration_env_probe_cache.rs` validates cache read/write behavior
- **SC-004**: Manual test: Second `deacon up` run completes 50%+ faster than first run when cache is enabled
- **SC-005**: Documentation: `deacon up --help` shows `--container-data-folder` flag with description of caching behavior

## Current Implementation Status

### Already Implemented ✅

1. **Core caching logic** in `crates/core/src/container_env_probe.rs`:
   - Cache loading logic (lines 147-164)
   - Cache persistence logic (lines 186-194)
   - Cache key generation using `{container_id}_{user}` format

2. **CLI flags** in `crates/deacon/src/cli.rs`:
   - `--container-data-folder`: Container-side data folder
   - `--container-system-data-folder`: System data folder
   - `--user-data-folder`: User-specific data folder
   - `--container-session-data-folder`: Session-specific data folder

3. **Function signature** for `resolve_env_and_user`:
   - Accepts `cache_folder: Option<&std::path::Path>` parameter
   - Passes cache_folder to `probe_container_environment`

4. **Integration test** skeleton at `crates/core/tests/integration_env_probe_cache.rs`

### Compilation Errors to Fix ❌

1. **Missing `cache_folder` in `UpArgs::default()`** (line 678):
   ```rust
   // Missing: cache_folder: None,
   ```

2. **Missing `cache_folder` in `ContainerLifecycleConfig` initializer** (line 2323):
   ```rust
   let lifecycle_config = deacon_core::container_lifecycle::ContainerLifecycleConfig {
       // Missing: cache_folder: None,
   ```

3. **Missing `cache_folder` in second `ContainerLifecycleConfig` initializer** (line 2777):
   ```rust
   let lifecycle_config = ContainerLifecycleConfig {
       // Missing: cache_folder: None,
   ```

4. **Unused variable warning** for `cache_folder` parameter in some function (needs verification)

### Implementation Tasks

1. Add `cache_folder: None,` to all struct initializers that are missing it
2. Verify `cache_folder` is properly threaded through the `up` command call chain
3. Run `cargo fmt` to fix formatting
4. Run `cargo clippy` to catch any remaining issues
5. Run `make test-nextest-fast` to verify all tests pass
6. Add integration test scenarios for cache hit/miss behavior
7. Update documentation/help text to mention caching behavior

## Technical Design Notes

### Cache File Format

```json
{
  "PATH": "/usr/local/bin:/usr/bin:/bin",
  "HOME": "/home/vscode",
  "SHELL": "/bin/bash",
  ...
}
```

### Cache Key Algorithm

```
cache_key = format!("{}_{}", container_id, user.unwrap_or("root"))
cache_path = cache_folder.join(format!("env_probe_{}.json", cache_key))
```

### Call Chain

```
CLI (--container-data-folder)
  → UpArgs { cache_folder }
  → resolve_env_and_user(cache_folder)
  → probe_container_environment(cache_folder)
    → [if cache exists] load from disk
    → [else] execute shell probe + persist to disk
```

### Error Handling

- Cache read failure → Log warning, fall back to fresh probe
- Cache write failure → Log warning, continue (caching is best-effort)
- Invalid JSON in cache → Parse error, fall back to fresh probe

### Observability

- **Cache hit**: Log at DEBUG level with cache file path
- **Cache miss**: Log at DEBUG level indicating fresh probe will execute
- **Cache write**: Log at DEBUG level with cache file path
- **Cache errors**: Log at WARN level with error details (read/write/parse failures)

## References

- Core implementation: `crates/core/src/container_env_probe.rs`
- CLI flags: `crates/deacon/src/cli.rs`
- Up command: `crates/deacon/src/commands/up.rs`
- Integration test: `crates/core/tests/integration_env_probe_cache.rs`
- Shared helpers: `crates/deacon/src/commands/shared/env_user.rs`

## Open Questions

None - implementation is clear from existing code structure.
