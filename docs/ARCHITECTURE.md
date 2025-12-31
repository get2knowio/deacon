# Deacon Architecture Documentation

This document describes the architectural patterns and cross-cutting concerns in the Deacon codebase.

---

## Cross-Cutting Patterns

### Cache Folder Support for Container Environment Probing

**Pattern**: Cache folder support for container env probing

**Purpose**: Enable performance optimization through caching of container environment variables, reducing `deacon` command latency by 50%+ on repeated invocations.

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

**Affected subcommands** (implemented):
- `deacon up` - Caching enabled for container startup
- `deacon exec` - Caching enabled for exec operations
- `deacon run-user-commands` - Caching enabled for user command execution

**Future subcommands** (to implement this pattern):
- `deacon build` (if it probes env during build)
- `deacon down` (if it needs env for cleanup hooks)
- `deacon stop` (if it needs env for shutdown hooks)
- Any new lifecycle-executing command

**Architecture Overview**:

```
CLI Flag (--container-data-folder)
  ↓
SubcommandArgs.container_data_folder: Option<PathBuf>
  ↓
resolve_env_and_user(cache_folder: Option<&Path>)
  ↓
probe_container_environment(cache_folder: Option<&Path>)
  ↓
[Check cache] → [Load OR Execute] → [Write cache]
  ↓
ContainerProbeResult { env_vars, shell_used, var_count }
```

**Cache Storage**:
- **Location**: `{cache_folder}/env_probe_{container_id}_{user}.json`
- **Format**: JSON serialized `HashMap<String, String>`
- **Key Pattern**: Composite key `{container_id}_{user}`
- **Invalidation**: Automatic on container ID change (no time-based expiration)
- **Error Handling**: Best-effort with graceful fallback (failures don't block operations)

**Code References**:
- **Core Logic**: `crates/core/src/container_env_probe.rs` - Cache read/write implementation
- **Shared Helper**: `crates/deacon/src/commands/shared/env_user.rs` - `resolve_env_and_user()`
- **Example Usage**: `crates/deacon/src/commands/up.rs`, `exec.rs`, `run_user_commands.rs`
- **Tests**: `crates/core/tests/integration_env_probe_cache.rs`

**Design Rationale**:
- **Shared Abstraction**: All subcommands use the same `resolve_env_and_user` helper, ensuring consistent cache behavior
- **DRY Principle**: Cache logic implemented once in core library, not per-subcommand
- **Principle VII Compliance**: Follows "Subcommand Consistency & Shared Abstractions" from project constitution
- **Performance**: 10-50x speedup on cache hit (90-98% latency reduction for env probe phase)

**Implementation Example**:

```rust
// Step 1: Add field to Args struct
pub struct MySubcommandArgs {
    // ... existing fields
    pub container_data_folder: Option<PathBuf>,
}

// Step 2: Add CLI flag (if needed)
#[arg(long, value_name = "PATH")]
pub container_data_folder: Option<PathBuf>,

// Step 3: Pass to shared helper
let env_user = resolve_env_and_user(
    &docker_client,
    &container_id,
    cli_user,
    config_remote_user,
    probe_mode,
    config_remote_env,
    &cli_env_map,
    args.container_data_folder.as_deref(),  // ← Pass cache folder
).await?;

// Step 4: Add to ContainerLifecycleConfig (if using lifecycle)
let lifecycle_config = ContainerLifecycleConfig {
    container_id: container_id.clone(),
    user: effective_user.clone(),
    // ... other fields
    cache_folder: args.container_data_folder.clone(),  // ← Add this
};

// Step 5: Update Default impl
impl Default for MySubcommandArgs {
    fn default() -> Self {
        Self {
            // ... existing fields
            container_data_folder: None,  // ← Add this
        }
    }
}
```

**Testing Checklist**:
- [ ] Integration test for cache hit scenario
- [ ] Integration test for cache miss scenario
- [ ] Integration test for per-user isolation
- [ ] Integration test for container ID invalidation
- [ ] Integration test for corrupted cache fallback
- [ ] Integration test for cache folder creation
- [ ] Integration test for no caching when folder is None
- [ ] Manual verification with `RUST_LOG=debug` logging

**Related Documentation**:
- Feature Spec: `/workspaces/deacon/specs/001-010-env-probe/spec.md`
- Implementation Plan: `/workspaces/deacon/specs/001-010-env-probe/plan.md`
- Developer Guide: `/workspaces/deacon/specs/001-010-env-probe/quickstart.md`
- Data Model: `/workspaces/deacon/specs/001-010-env-probe/data-model.md`
- Cache Schema: `/workspaces/deacon/specs/001-010-env-probe/contracts/cache-schema.json`

---

## Future Patterns

As new cross-cutting patterns emerge in the codebase, they should be documented here following the same structure:
- Pattern name and purpose
- When to apply
- Implementation checklist
- Code references
- Design rationale
- Implementation example

This ensures consistency across subcommands and helps maintainers understand architectural decisions.
