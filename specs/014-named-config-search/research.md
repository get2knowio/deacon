# Research: Named Config Folder Search

**Feature**: 014-named-config-search
**Date**: 2026-02-22

## Research Tasks

### RT-1: Upstream Spec Config Search Algorithm

**Question**: What is the exact algorithm for the three config search locations?

**Findings**: The upstream containers.dev spec (https://containers.dev/implementors/spec/) defines three search locations in priority order:
1. `.devcontainer/devcontainer.json` (highest priority)
2. `.devcontainer.json` (root level)
3. `.devcontainer/<folder>/devcontainer.json` for each direct child subdirectory of `.devcontainer/`

The spec notes: "It is valid that these files may exist in more than one location, so consider providing a mechanism for users to select one when appropriate."

Priority levels 1 and 2 short-circuit: if found, no subdirectory enumeration occurs. When multiple configs are found at level 3, the implementation should provide a selection mechanism (in our case: error with listing + `--config` flag).

### RT-2: Current Implementation Architecture

**Question**: How is config discovery currently structured?

**Findings**:
- `ConfigLoader::discover_config(workspace: &Path) -> Result<ConfigLocation>` in `crates/core/src/config.rs:1382-1416`
- Returns `ConfigLocation { path, exists }` — a single path with existence flag
- Two call sites:
  1. Shared `load_config()` in `crates/deacon/src/commands/shared/config_loader.rs:70` — used by 5 of 6 commands
  2. `down` command in `crates/deacon/src/commands/down.rs:62` — has custom fallback to auto-discovery from state
- Current behavior when no config found: returns the first search path (`.devcontainer/devcontainer.json`) with `exists: false`, which downstream code handles via fallback or `NotFound` error

### RT-3: Return Type Design for Multiple Configs

**Question**: Should `discover_config()` return a single path, multiple paths, or an enum?

**Findings**: The current `ConfigLocation` type cannot represent the "multiple configs found" case. Three options considered:

1. **Return `Vec<ConfigLocation>`**: Callers must check length. Loses the semantic distinction between "not found" and "ambiguous".
2. **Return new `DiscoveryResult` enum**: Explicitly models all three outcomes (single found, multiple found, none found). Best for type safety.
3. **Return error on multiple**: Put the error inside `discover_config()`. Works but moves policy into core where it may not belong.

**Decision**: Option 2 — new `DiscoveryResult` enum. This keeps core logic pure (discovery reports what it found) and lets callers (CLI layer) decide how to present the error. The enum variants clearly encode the three outcomes and carry the relevant paths.

### RT-4: File Extension Handling

**Question**: Should named config search check for both `.json` and `.jsonc`?

**Findings**: The spec and existing code both support JSONC (JSON with comments) via the `json5` parser. The existing search paths only check `devcontainer.json` (not `.jsonc`), but the shared `load_config()` validates that `--config` paths can be either `devcontainer.json` or `devcontainer.jsonc`. For consistency, named config enumeration should check for both `devcontainer.json` and `devcontainer.jsonc` in each subdirectory. If both exist in the same subdirectory, that subdirectory contributes one config (prefer `.json` over `.jsonc`).

**Decision**: Check both `devcontainer.json` and `devcontainer.jsonc` in each subdirectory. Within a single subdirectory, prefer `devcontainer.json` if both exist (consistent with `.json` being the primary extension).

Note: The existing priority-1 and priority-2 search locations also only check `.json`. Adding `.jsonc` support to those locations is a separate enhancement — this feature only adds `.jsonc` checking to the new named config search (priority 3) per FR-006. However, for consistency across the full discovery function, we should also check `.jsonc` at priority levels 1 and 2 in the same change. This avoids a situation where `.devcontainer/devcontainer.jsonc` exists but is not found while `.devcontainer/python/devcontainer.jsonc` is found.

**Revised Decision**: Check both `.json` and `.jsonc` at ALL three priority levels. Within each level, prefer `.json` if both exist. This is the most consistent behavior and aligns with the spec's intent that `.jsonc` is a valid config filename everywhere.

### RT-5: Error Message Format for Multiple Configs

**Question**: What should the error message look like when multiple named configs are found?

**Findings**: The upstream reference CLI (`@devcontainers/cli`) produces an error listing available configs. For Deacon, the error should be clear and actionable.

**Decision**: Use `ConfigError::MultipleConfigs` with a formatted message:
```
Multiple devcontainer configurations found. Use --config to specify one:
  .devcontainer/node/devcontainer.json
  .devcontainer/python/devcontainer.json
  .devcontainer/rust/devcontainer.json
```
Paths are listed alphabetically (sorted by subdirectory name per FR-005) and displayed relative to the workspace folder for readability.

### RT-6: Symlink and Special Character Handling

**Question**: How should symlinked directories and special characters in directory names be handled?

**Findings**: The spec says only direct subdirectories are searched, one level deep. `std::fs::read_dir()` follows symlinks by default on all platforms (it reads directory entries, and accessing metadata/existence of a symlink target is standard OS behavior). Special characters in directory names are handled naturally by `PathBuf` and the OS filesystem layer.

**Decision**: No special handling needed. Use standard `std::fs::read_dir()` and `PathBuf` operations. Symlinks are followed naturally. Unicode/special characters work via OS path handling. Only filter for entries that are directories (via `entry.file_type()?.is_dir()`).

### RT-7: Down Command Integration

**Question**: How should the `down` command's custom config handling adapt?

**Findings**: The `down` command calls `ConfigLoader::discover_config()` directly (not through `load_config()`), then has custom fallback logic: if no config found, it tries auto-discovery from saved state. With the new `DiscoveryResult`, `down` needs to handle the `Multiple` case as an error (same as other commands) and the `Single`/`None` cases as before.

**Decision**: Update `down` to match on `DiscoveryResult`. The `Multiple` case produces the same `MultipleConfigs` error. The `None` case preserves the existing auto-discovery-from-state fallback. This keeps `down`'s special behavior intact while adding multi-config awareness.

## Decisions Summary

| # | Decision | Rationale | Alternatives Rejected |
|---|----------|-----------|----------------------|
| D1 | New `DiscoveryResult` enum with `Single`, `Multiple`, `None` variants | Type-safe representation of all outcomes; keeps policy in CLI layer | Vec return (loses semantics), error-on-multiple (policy in core) |
| D2 | Check `.json` and `.jsonc` at all three priority levels | Consistency across discovery; prevents inconsistent behavior | Only add to priority 3 (inconsistent with spec intent) |
| D3 | Prefer `.json` over `.jsonc` when both exist in same location | `.json` is the primary/canonical extension | Error on both (too strict), use `.jsonc` first (unusual precedence) |
| D4 | Alphabetical sort of subdirectories by name | Deterministic cross-platform behavior per FR-005 | No sort (platform-dependent), sort by mtime (non-deterministic) |
| D5 | `ConfigError::MultipleConfigs` error variant with path list | Clear, actionable error message per FR-008 | Reuse `Validation` (loses specificity), return first (violates spec) |
| D6 | Standard `read_dir` for symlinks/special chars | OS handles naturally; no special code needed | Manual symlink detection (unnecessary complexity) |
| D7 | `down` command updated to match on `DiscoveryResult` | Preserves existing auto-discovery fallback while adding multi-config awareness | Force `down` through shared `load_config()` (loses state fallback) |
