# Research: Env-Probe Cache Completion

**Feature**: 001-010-env-probe  
**Date**: 2025-11-23

## Overview

This document consolidates research findings for completing the env-probe caching feature. Most technical decisions were made during spec creation; this document formalizes them for implementation reference.

---

## Research Question 1: Cache Storage Mechanism

**Decision**: Filesystem-based JSON cache

**Rationale**:
- **Simplicity**: Single JSON file per cache key (container+user), no database required
- **Portability**: Works across all platforms (Linux/macOS/Windows) without dependencies
- **Transparency**: Users can inspect/delete cache files manually
- **Alignment**: Matches devcontainer CLI patterns (uses filesystem for state)
- **Performance**: Negligible overhead (<10ms for read/write operations)

**Alternatives Considered**:
1. **In-memory cache**: Discarded - doesn't survive process restarts, negating performance benefit
2. **SQLite database**: Discarded - overkill for simple key-value store, adds dependency complexity
3. **Redis/external cache**: Discarded - requires external service, unsuitable for CLI tool

**Implementation Details**:
- Location: `{cache_folder}/env_probe_{container_id}_{user}.json`
- Format: `serde_json` serialization of `HashMap<String, String>`
- Error handling: Best-effort (failures don't block operations)

---

## Research Question 2: Cache Key Design

**Decision**: Composite key `{container_id}_{user}`

**Rationale**:
- **Container isolation**: Different containers have different environments (PATH, nvm, etc.)
- **User isolation**: Same container, different users have different shell configs
- **Natural invalidation**: Container rebuild changes ID, auto-invalidates cache
- **Collision avoidance**: Underscore separator prevents `container_a_user_b` vs `container_a_userb` collision

**Alternatives Considered**:
1. **Container ID only**: Discarded - would mix root and non-root user environments
2. **Hash-based key**: Discarded - harder to debug, users can't identify cache files
3. **Include probe mode in key**: Discarded - adds complexity without clear benefit (probe mode rarely changes)

**Implementation Details**:
- Format: `format!("{}_{}", container_id, user.unwrap_or("root"))`
- Example: `abc123def456_vscode` or `abc123def456_root`
- Validation: Container ID must be non-empty (enforced by probe function)

---

## Research Question 3: Cache Expiration Policy

**Decision**: No time-based expiration (container ID change only)

**Rationale**:
- **Simplicity**: Avoids timestamp tracking, TTL management, background cleanup
- **Correctness**: Container ID change = environment may have changed, cache MUST be invalidated
- **Common case**: Devcontainers are typically short-lived (hours/days), not weeks/months
- **User control**: Users can manually delete cache folder if needed

**Alternatives Considered**:
1. **24-hour TTL**: Discarded - adds complexity, forces re-probing for long-lived containers
2. **Timestamp warning**: Discarded - confusing UX, unclear action for user
3. **Manual invalidation command**: Discarded - can be added later if needed (user can delete files now)

**Implementation Details**:
- Cache persists until container ID changes
- Stale cache files accumulate (user responsibility to clean)
- Future enhancement: `deacon cache clean` command

---

## Research Question 4: Logging Strategy

**Decision**: DEBUG-level for operations, WARN-level for errors

**Rationale**:
- **INFO level too noisy**: Cache hit occurs on every `deacon up` invocation
- **DEBUG appropriate**: Developers can enable with `RUST_LOG=debug` when investigating cache behavior
- **WARN for errors**: Cache failures are not critical but worth surfacing
- **Structured logging**: Use `tracing` fields for cache path, container ID, user

**Alternatives Considered**:
1. **INFO for all operations**: Discarded - pollutes logs for normal users
2. **Silent on success**: Discarded - makes debugging cache issues difficult
3. **Separate cache log file**: Discarded - unnecessary complexity

**Implementation Details**:
- Cache hit: `debug!(cache_path = %path, var_count, "Loaded cached env probe")`
- Cache miss: `debug!(container_id, user, "Cache miss: executing fresh probe")`
- Cache write: `debug!(cache_path = %path, var_count, "Persisted env probe cache")`
- Cache errors: `warn!(cache_path = %path, error = %e, "Failed to read cache, falling back")`

---

## Research Question 5: Error Handling Strategy

**Decision**: Best-effort with graceful fallback

**Rationale**:
- **Caching is optimization**: Failures must not block core functionality
- **Fail gracefully**: Log warning + execute fresh probe (same as no cache)
- **Three failure modes**:
  1. Cache read fails (permissions, I/O error) → Fallback to fresh probe
  2. Cache parse fails (corrupted JSON) → Fallback to fresh probe
  3. Cache write fails (permissions, disk full) → Continue without caching

**Alternatives Considered**:
1. **Hard fail on cache errors**: Discarded - breaks caching contract (must be optional/transparent)
2. **Silent failures**: Discarded - makes debugging impossible
3. **Retry logic**: Discarded - adds complexity without clear benefit (cache is best-effort)

**Implementation Details**:
- Use `Result<T>` internally but convert errors to warnings at call site
- Existing implementation already handles this correctly
- No changes needed to error handling logic

---

## Research Question 6: Cross-Cutting Integration

**Decision**: Thread `cache_folder` through shared `resolve_env_and_user` helper

**Rationale**:
- **DRY principle**: All subcommands use same helper, get caching for free
- **Consistency**: Cache behavior identical across `up`, `exec`, `run-user-commands`
- **Shared abstraction**: Aligns with Constitution Principle VII (no per-subcommand reimplementation)

**Impact Analysis**:
- **Immediate**: 3 subcommands benefit (`up`, `exec`, `run-user-commands`)
- **Future**: New subcommands automatically get caching if they use shared helper
- **Migration**: Fix compilation errors by adding `cache_folder` to struct initializers

**Implementation Pattern**:
```rust
// CLI flag (already exists)
--container-data-folder <PATH>

// Subcommand args struct
pub struct SubcommandArgs {
    pub container_data_folder: Option<PathBuf>,
    // ... other fields
}

// Pass to shared helper
resolve_env_and_user(
    // ... other params
    args.container_data_folder.as_deref(),  // Option<&Path>
)

// Helper passes to probe function
prober.probe_container_environment(
    // ... other params
    cache_folder,  // Option<&Path>
)
```

---

## Research Question 7: Testing Strategy

**Decision**: Integration tests with real Docker containers + mocked failure scenarios

**Rationale**:
- **End-to-end validation**: Tests actual cache file creation/reading
- **Docker integration**: Uses real Docker daemon (safe to run in parallel with `docker-shared` group)
- **Deterministic**: No network, pinned container images
- **Hermetic**: Tests clean up cache files after completion

**Test Scenarios** (7 required):
1. Cache hit - Probe twice, verify second is faster
2. Cache miss - Probe with no cache, verify file created
3. Per-user isolation - Probe as user A, then user B, verify separate files
4. Container ID invalidation - Rebuild container, verify new cache created
5. Corrupted JSON fallback - Write invalid JSON, verify fallback to fresh probe
6. Cache folder creation - Probe with non-existent folder, verify folder created
7. No caching when None - Probe with cache_folder=None, verify no file created

**Nextest Configuration**:
```toml
[[profile.*.overrides]]
filter = 'test(integration_env_probe_cache)'
test-group = 'docker-shared'  # Safe to parallelize
```

---

## Best Practices from Codebase

### Tracing Patterns
- Use structured fields: `debug!(field = %value, "message")`
- Use `%` for Display trait, `?` for Debug trait
- Instrument functions: `#[instrument(skip(docker))]`

### Error Context
- Add context with `.context("operation description")`
- Propagate with `?` operator
- Log warnings with `warn!(error = %e, "description")`

### File I/O
- Use `std::fs::create_dir_all` for directory creation (idempotent)
- Use `std::fs::read_to_string` for small files (<1MB)
- Use `std::fs::write` for atomic write operations

### Testing
- Use `make test-nextest-fast` for development loop
- Use `make test-nextest` before PR
- Configure test groups in `.config/nextest.toml`

---

## Summary

All research questions resolved with clear decisions:
1. **Storage**: Filesystem JSON cache
2. **Cache key**: `{container_id}_{user}`
3. **Expiration**: Container ID change only (no TTL)
4. **Logging**: DEBUG for ops, WARN for errors
5. **Error handling**: Best-effort with graceful fallback
6. **Integration**: Thread through shared `resolve_env_and_user` helper
7. **Testing**: Integration tests with `docker-shared` group

No blocking unknowns. Ready for Phase 1 implementation.
