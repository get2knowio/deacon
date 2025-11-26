# Implementation Plan: Env-Probe Cache Completion

**Branch**: `001-010-env-probe` | **Date**: 2025-11-23 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/workspaces/deacon/specs/001-010-env-probe/spec.md`

## Summary

Complete the partially-implemented env-probe caching feature for container environment probing. The core caching logic exists in `container_env_probe.rs` but has compilation errors preventing its use. This plan addresses:

1. **Compilation fixes**: Add missing `cache_folder` fields to struct initializers across multiple subcommands (`up`, `exec`, `run-user-commands`)
2. **Cross-cutting infrastructure**: Ensure `cache_folder` parameter flows through shared helpers (`resolve_env_and_user`, `ContainerLifecycleConfig`) consistently
3. **Observability**: Add DEBUG-level logging for cache operations (hit/miss/write) and WARN-level for errors
4. **Testing**: Enhance integration tests to validate cache behavior end-to-end
5. **Future-proofing**: Document cache folder support for upcoming subcommands (build, down, etc.) to maintain consistency

**Technical Approach**: Fix compilation errors, add logging statements to existing cache logic, verify test coverage, and document cross-cutting patterns for future subcommands.

## Technical Context

**Language/Version**: Rust 1.75+ (Edition 2021)  
**Primary Dependencies**: 
- `serde/serde_json` for cache serialization
- `tracing` for observability
- `std::fs` for file I/O

**Storage**: Filesystem-based JSON cache at `{cache_folder}/env_probe_{container_id}_{user}.json`  
**Testing**: 
- `cargo-nextest` for parallel test execution
- `make test-nextest-fast` for development loop (unit/bin/examples/doctests)
- `make test-nextest` for full suite before PR

**Target Platform**: Linux/macOS/Windows (devcontainer CLI tooling)  
**Project Type**: Single Rust workspace with binary (`deacon`) and library (`deacon_core`) crates  

**Performance Goals**: 
- Cache hit reduces `deacon up` latency by 50%+ (from seconds to milliseconds for env probe phase)
- Cache operations add <10ms overhead to first run (cache miss + write)

**Constraints**: 
- Best-effort caching (failures must not block operations)
- No time-based expiration (container ID change only)
- Cache folder is optional (must work when None)

**Scale/Scope**: 
- 3 subcommands affected (`up`, `exec`, `run-user-commands`)
- 1 core struct affected (`ContainerLifecycleConfig`)
- 1 shared helper affected (`resolve_env_and_user`)
- ~10 struct initializer sites to fix
- ~5 new logging statements to add

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Spec-Parity as Source of Truth ✅
- **Status**: PASS
- **Rationale**: This feature completes an existing partially-implemented capability. The caching logic already exists in `container_env_probe.rs` with correct cache key format (`{container_id}_{user}`), JSON serialization, and graceful fallback on errors. No new spec requirements; just finishing the wiring.

### Principle II: Keep the Build Green ✅
- **Status**: CURRENTLY FAILING (compilation errors), WILL PASS after Phase 1
- **Current State**: 6 compilation errors blocking build:
  - Missing `cache_folder` in `UpArgs::default()` (line 678)
  - Missing `cache_folder` in 2x `ContainerLifecycleConfig` initializers (lines 2323, 2777)
  - Missing `cache_folder` in `ExecArgs` test initializers (~4 sites)
  - Unused variable warning for `cache_folder` in run_user_commands.rs
- **Remediation Plan**: 
  1. Add `cache_folder: None,` to all struct initializers
  2. Remove `#[allow(dead_code)]` from `container_data_folder` fields that are now used
  3. Run `cargo fmt && cargo clippy` to verify zero warnings
  4. Run `make test-nextest-fast` to verify no regressions
  5. Run `make test-nextest` before PR

### Principle III: No Silent Fallbacks — Fail Fast ✅
- **Status**: PASS
- **Rationale**: Cache implementation already fails fast on critical errors (invalid container ID returns error). Cache read/write failures are logged at WARN level and fall back to fresh probe (correct behavior per spec - caching is best-effort).

### Principle IV: Idiomatic, Safe Rust ✅
- **Status**: PASS
- **Rationale**: 
  - No `unsafe` code
  - Error handling via `Result` with `anyhow::Context`
  - Uses `tracing` for structured logging
  - Follows rustfmt conventions
  - Proper ownership (cache_folder passed as `Option<&Path>`)

### Principle V: Observability and Output Contracts ✅
- **Status**: PARTIAL (missing DEBUG logs), WILL PASS after Phase 1
- **Current State**: Existing code logs at INFO level for probe completion, but missing DEBUG logs for cache operations
- **Remediation Plan**: Add DEBUG-level logging for:
  - Cache hit: `debug!("Loaded cached env probe from {}", cache_path.display())`
  - Cache miss: `debug!("No cache found, executing fresh probe")`
  - Cache write: `debug!("Persisted env probe cache to {}", cache_path.display())`
- **Contract Compliance**: Cache operations don't affect stdout/stderr contracts (all logging goes to stderr via tracing)

### Principle VI: Testing Completeness ✅
- **Status**: PARTIAL (test skeleton exists), WILL PASS after Phase 1
- **Current State**: Integration test skeleton at `crates/core/tests/integration_env_probe_cache.rs` validates cache consistency but needs enhancement
- **Remediation Plan**: 
  - Verify existing tests cover cache hit/miss/write scenarios
  - Add test for per-user isolation (alice vs bob)
  - Add test for cache invalidation on container ID change
  - Add test for corrupted JSON fallback
  - Configure test in `.config/nextest.toml` with appropriate test group (likely `docker-shared`)

### Principle VII: Subcommand Consistency & Shared Abstractions ✅
- **Status**: PASS WITH CROSS-CUTTING REQUIREMENTS
- **Rationale**: This feature demonstrates proper shared abstraction use:
  - Cache folder parameter flows through shared `resolve_env_and_user` helper (not reimplemented per subcommand)
  - `ContainerLifecycleConfig` is the canonical struct for lifecycle execution (used by up/exec/run-user-commands)
  - CLI flags (`--container-data-folder`, etc.) are defined once in `cli.rs` and shared
- **Cross-Cutting Impact**: 
  - **Immediate**: Fix `up`, `exec`, and `run-user-commands` compilation errors
  - **Future**: Document cache folder pattern for upcoming subcommands (build, down, stop) to maintain consistency
  - **Backlog Item**: Record "cache folder support" as shared pattern for all container-interacting subcommands

### Principle VIII: Executable & Self-Verifying Examples ✅
- **Status**: PASS (no new examples required)
- **Rationale**: This is an internal performance optimization. Existing examples will benefit from caching but don't need new `exec.sh` scripts. Cache behavior is transparent to users and tested via integration tests.

### Summary
**Overall Gate Status**: ⚠️ BLOCKED by compilation errors (Principle II violation)  
**Action Required**: Complete Phase 1 implementation fixes before proceeding  
**Expected Outcome**: All gates PASS after struct initializer fixes + logging additions

---

## Project Structure

### Documentation (this feature)

```text
specs/001-010-env-probe/
├── spec.md              # Feature specification (DONE)
├── plan.md              # This file (IN PROGRESS)
├── research.md          # Phase 0 output (minimal - most research done in spec)
├── data-model.md        # Phase 1 output (cache data structures)
├── quickstart.md        # Phase 1 output (developer guide for cache folder usage)
└── contracts/           # Phase 1 output (cache file schema)
    └── cache-schema.json
```

### Source Code (repository root)

```text
crates/
├── core/
│   ├── src/
│   │   ├── container_env_probe.rs         # [MODIFY] Add DEBUG logging for cache ops
│   │   └── container_lifecycle.rs         # [VERIFY] ContainerLifecycleConfig.cache_folder field exists
│   └── tests/
│       └── integration_env_probe_cache.rs # [ENHANCE] Add cache hit/miss/isolation tests
│
└── deacon/
    ├── src/
    │   ├── cli.rs                          # [VERIFY] --container-data-folder flag exists
    │   └── commands/
    │       ├── up.rs                       # [FIX] Add cache_folder to 3 struct initializers
    │       ├── exec.rs                     # [FIX] Add cache_folder to 4+ struct initializers
    │       ├── run_user_commands.rs        # [FIX] Add cache_folder field + remove dead_code attr
    │       └── shared/
    │           └── env_user.rs             # [VERIFY] resolve_env_and_user passes cache_folder
    └── tests/
        ├── integration_exec_env.rs         # [VERIFY] Tests still pass after fixes
        └── parity_env_probe_flag.rs        # [VERIFY] Parity test still passes

.config/
└── nextest.toml                            # [UPDATE] Configure integration_env_probe_cache test group
```

**Structure Decision**: Single Rust workspace with clear separation between `core` (library with caching logic) and `deacon` (binary with CLI/subcommands). This aligns with existing project structure and Principle VII (shared abstractions in core, command orchestration in binary).

---

## Phase 0: Outline & Research

### Unknowns from Technical Context

✅ **All unknowns resolved during spec creation**:
- Cache expiration policy → Answered: No time-based expiration (container ID change only)
- Observability requirements → Answered: DEBUG-level logging for cache ops, WARN for errors
- Cross-cutting impact → Identified: 3 subcommands + 1 core struct + shared helper

### Research Tasks (Minimal)

No agent-dispatched research required. All technical decisions already made:

1. **Cache file format**: JSON via `serde_json` (already implemented)
2. **Cache key algorithm**: `{container_id}_{user}` (already implemented)
3. **Error handling strategy**: Best-effort with graceful fallback (already implemented)
4. **Logging patterns**: Use existing `tracing` infrastructure with `debug!()` macro
5. **Test strategy**: Enhance existing `integration_env_probe_cache.rs` test

### Findings Consolidation

See `research.md` for detailed findings (to be generated).

**Key Decisions**:
- **Decision**: Use filesystem JSON cache (not in-memory, not database)
- **Rationale**: Simple, portable, aligns with devcontainer CLI patterns. No need for persistence layer complexity.
- **Alternatives Considered**: In-memory cache (discarded: doesn't survive process restart), SQLite (discarded: overkill for simple key-value store)

---

## Phase 1: Design & Contracts

### Data Model

See `data-model.md` for complete data model documentation (to be generated).

**Core Entities**:

1. **CacheKey**: `String` - Composite identifier `{container_id}_{user}`
   - Format: Container ID + underscore + username (or "root" if None)
   - Example: `abc123def456_vscode` or `abc123def456_root`
   - Validation: Container ID must be non-empty (enforced by probe function)

2. **CacheFile**: JSON file on disk
   - Location: `{cache_folder}/env_probe_{cache_key}.json`
   - Schema: `HashMap<String, String>` serialized via `serde_json`
   - Example content:
     ```json
     {
       "PATH": "/usr/local/bin:/usr/bin:/bin",
       "HOME": "/home/vscode",
       "SHELL": "/bin/bash"
     }
     ```

3. **ContainerProbeResult**: Struct in `container_env_probe.rs`
   - Fields:
     - `env_vars: HashMap<String, String>` - Environment variables (from cache or fresh probe)
     - `shell_used: String` - Shell used for probing (or "cache" if loaded from disk)
     - `var_count: usize` - Number of variables captured
   - Source: Can be loaded from cache OR generated by executing shell probe

### API Contracts

See `contracts/cache-schema.json` for JSON schema (to be generated).

**Internal API** (no user-facing REST/GraphQL endpoints):

1. **Function**: `ContainerEnvironmentProber::probe_container_environment()`
   - **Input**: 
     - `container_id: &str` - Target container
     - `mode: ContainerProbeMode` - Probe mode (None/LoginShell/LoginInteractiveShell)
     - `user: Option<&str>` - User to probe as
     - `cache_folder: Option<&Path>` - Optional cache directory
   - **Output**: `Result<ContainerProbeResult>`
   - **Behavior**:
     - If `cache_folder` is None → Always execute fresh probe
     - If `cache_folder` is Some:
       1. Check if cache file exists
       2. If exists + valid JSON → Load from cache (log DEBUG)
       3. If not exists / invalid JSON → Execute fresh probe + persist to cache (log DEBUG)
       4. On any error → Log WARN + fall back to fresh probe

2. **Function**: `resolve_env_and_user()`
   - **Input**: 
     - `cache_folder: Option<&Path>` - Passed through to `probe_container_environment`
     - [other existing params]
   - **Output**: `EnvUserResolution` - Contains probed env vars + effective user
   - **Behavior**: Unchanged except passes `cache_folder` to probe function

3. **Struct Field**: `ContainerLifecycleConfig::cache_folder`
   - **Type**: `Option<PathBuf>`
   - **Usage**: Stored in lifecycle config, used when executing lifecycle commands with env probing
   - **Default**: `None` (no caching)

### Compilation Fixes Required

**File: `crates/deacon/src/commands/up.rs`**

1. Line 678: Add `cache_folder: None,` to `UpArgs::default()`
2. Line 2323: Add `cache_folder: None,` to `ContainerLifecycleConfig` initializer (prebuild phase)
3. Line 2777: Add `cache_folder: None,` to `ContainerLifecycleConfig` initializer (lifecycle execution)

**File: `crates/deacon/src/commands/exec.rs`**

1. Multiple test helper functions (~4 sites): Add `cache_folder: None,` to `ExecArgs` test initializers
2. Verify `args.container_data_folder.as_deref()` is passed to `resolve_env_and_user` (already correct)

**File: `crates/deacon/src/commands/run_user_commands.rs`**

1. Add `pub container_data_folder: Option<PathBuf>` field to `RunUserCommandsArgs` struct (if not present)
2. Remove `#[allow(dead_code)]` from `container_data_folder` field once used
3. Pass `args.container_data_folder.as_deref()` to lifecycle execution

### Logging Additions Required

**File: `crates/core/src/container_env_probe.rs`**

**Location: Line ~152** (after cache load success):
```rust
debug!(
    cache_path = %cache_path.display(),
    var_count = env_vars.len(),
    "Loaded cached env probe"
);
```

**Location: Line ~164** (when cache doesn't exist, before fresh probe):
```rust
debug!(
    container_id = %container_id,
    user = ?user,
    "Cache miss: executing fresh probe"
);
```

**Location: Line ~191** (after cache write success):
```rust
debug!(
    cache_path = %cache_path.display(),
    var_count = env_vars.len(),
    "Persisted env probe cache"
);
```

**Location: Line ~152** (replace existing silent cache read error with):
```rust
warn!(
    cache_path = %cache_path.display(),
    error = %e,
    "Failed to read cache file, falling back to fresh probe"
);
```

### Testing Enhancements Required

**File: `crates/core/tests/integration_env_probe_cache.rs`**

Add test scenarios:

1. **Test: Cache hit** - Probe twice with same container/user, verify second is faster + doesn't execute shell
2. **Test: Cache miss** - Probe with no cache, verify cache file created
3. **Test: Per-user isolation** - Probe as user A, then user B, verify separate cache files
4. **Test: Container ID invalidation** - Probe container A, delete container, probe container B, verify new cache
5. **Test: Corrupted JSON fallback** - Write invalid JSON to cache file, verify fallback to fresh probe
6. **Test: Cache folder creation** - Probe with non-existent cache folder, verify folder created
7. **Test: No caching when folder is None** - Probe with cache_folder=None, verify no cache file created

**Nextest Configuration**:
```toml
[[profile.default.overrides]]
filter = 'test(integration_env_probe_cache)'
test-group = 'docker-shared'  # Safe to run in parallel with other Docker tests
```

Add to all profiles: `default`, `dev-fast`, `full`, `ci`

---

## Phase 2: Implementation Tasks (Out of Scope for /speckit.plan)

Phase 2 task breakdown will be generated by `/speckit.tasks` command. This plan ends after Phase 1 design completion.

---

## Cross-Cutting Patterns for Future Subcommands

**Pattern**: Cache folder support for container env probing

**When to apply**: Any subcommand that:
1. Executes commands inside a container
2. Needs to resolve container environment variables
3. Calls `resolve_env_and_user()` helper

**Implementation checklist**:
1. Add `container_data_folder: Option<PathBuf>` to subcommand Args struct
2. Add CLI flag `--container-data-folder <PATH>` (if not inherited from global CLI)
3. Pass `args.container_data_folder.as_deref()` to `resolve_env_and_user()`
4. Add `cache_folder` field to `ContainerLifecycleConfig` initializer (if using lifecycle)
5. Update subcommand's `--help` text to mention caching behavior

**Affected future subcommands**:
- `deacon build` (if it probes env during build)
- `deacon down` (if it needs env for cleanup hooks)
- `deacon stop` (if it needs env for shutdown hooks)
- Any new lifecycle-executing command

**Documentation location**: Add to `docs/ARCHITECTURE.md` (or create if missing)

---

## Validation Checklist

Before marking this feature complete:

- [ ] All compilation errors resolved (`cargo build` succeeds)
- [ ] Zero clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] Code formatted (`cargo fmt --all -- --check`)
- [ ] Fast tests pass (`make test-nextest-fast`)
- [ ] Full tests pass (`make test-nextest`)
- [ ] Integration tests cover all cache scenarios (7 tests listed above)
- [ ] Logging verified manually with `RUST_LOG=debug deacon up --container-data-folder=/tmp/cache`
- [ ] Manual test: Second `deacon up` run is 50%+ faster
- [ ] Documentation updated (quickstart.md, cache schema in contracts/)
- [ ] Cross-cutting pattern documented for future maintainers

---

## Appendix: Compilation Error Details

```
error[E0063]: missing field `cache_folder` in initializer of `UpArgs`
  --> crates/deacon/src/commands/up.rs:678:9
   |
678|         Self {
   |         ^^^^ missing `cache_folder`

error[E0063]: missing field `cache_folder` in initializer of `ContainerLifecycleConfig`
   --> crates/deacon/src/commands/up.rs:2323:28
    |
2323|     let lifecycle_config = deacon_core::container_lifecycle::ContainerLifecycleConfig {
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ missing `cache_folder`

error[E0063]: missing field `cache_folder` in initializer of `ContainerLifecycleConfig`
   --> crates/deacon/src/commands/up.rs:2777:28
    |
2777|     let lifecycle_config = ContainerLifecycleConfig {
    |                            ^^^^^^^^^^^^^^^^^^^^^^^^ missing `cache_folder`

warning: unused variable: `cache_folder`
   --> crates/deacon/src/commands/run_user_commands.rs:135:9
    |
135|         cache_folder
    |         ^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_cache_folder`
```

Total: 3 errors, 1 warning across 2 files

---

**Next Steps**: 
1. Generate `research.md` (minimal - most research done)
2. Generate `data-model.md` (cache structures documented above)
3. Generate `quickstart.md` (developer guide for using cache folder)
4. Generate `contracts/cache-schema.json` (JSON schema for cache file)
5. Run `/speckit.tasks` to break down implementation into atomic tasks
