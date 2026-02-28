# Contract: Config Discovery

**Feature**: 014-named-config-search
**Date**: 2026-02-22

## Interface: `ConfigLoader::discover_config`

### Signature

```rust
pub fn discover_config(workspace: &Path) -> Result<DiscoveryResult>
```

### Preconditions

- `workspace` is a path to an existing directory
- If `workspace` does not exist, returns `Err(ConfigError::NotFound)`

### Postconditions

Returns one of three variants:

| Variant | Condition | Payload |
|---------|-----------|---------|
| `DiscoveryResult::Single(path)` | Exactly one config file found across all 3 priority levels | Absolute path to the config file; file exists |
| `DiscoveryResult::Multiple(paths)` | Multiple named configs found at priority 3 (no higher-priority match) | Vec of absolute paths, sorted alphabetically by subdirectory name, length >= 2 |
| `DiscoveryResult::None(default)` | No config file found at any level | Default path (`.devcontainer/devcontainer.json`); file does NOT exist |

### Search Algorithm

```
1. Check priority 1: .devcontainer/devcontainer.json, then .devcontainer/devcontainer.jsonc
   → If found: return Single(path)

2. Check priority 2: .devcontainer.json, then .devcontainer.jsonc
   → If found: return Single(path)

3. Enumerate priority 3: for each direct subdirectory of .devcontainer/ (sorted alphabetically):
   a. Check for devcontainer.json in subdirectory
   b. If not found, check for devcontainer.jsonc in subdirectory
   c. If either found, add to candidates list

4. If candidates.len() == 1: return Single(candidates[0])
5. If candidates.len() > 1: return Multiple(candidates)
6. return None(default_path)
```

### Error Cases

| Error | Condition |
|-------|-----------|
| `ConfigError::NotFound { path }` | Workspace directory does not exist |
| `ConfigError::Io(e)` | Filesystem error during `read_dir` or `file_type` |

### Tracing

- Span: `config.discover` (existing)
- Debug logs for each search step
- Info log when named config enumeration produces results

---

## Interface: `load_config` (shared CLI helper)

### Signature

```rust
pub fn load_config(args: ConfigLoadArgs<'_>) -> Result<ConfigLoadResult>
```

### Behavior Change

When `args.config_path` is `None` (no explicit `--config`):
- Calls `ConfigLoader::discover_config()`
- On `DiscoveryResult::Single(path)` → proceeds with loading (existing behavior)
- On `DiscoveryResult::Multiple(paths)` → returns `Err(ConfigError::MultipleConfigs { paths })`
- On `DiscoveryResult::None(default)` → falls back to override or returns `NotFound` (existing behavior)

When `args.config_path` is `Some(path)`:
- Uses that path directly (existing behavior, unchanged)

---

## Interface: `down` command discovery

### Behavior Change

The `down` command calls `discover_config()` directly. Updated handling:
- `DiscoveryResult::Single(path)` → load config (existing behavior)
- `DiscoveryResult::Multiple(paths)` → return `Err(ConfigError::MultipleConfigs { paths })`
- `DiscoveryResult::None(_)` → fall back to auto-discovery from state (existing behavior)

---

## Error Message Contract

### MultipleConfigs Error Format

```
Multiple devcontainer configurations found. Use --config to specify one:
  .devcontainer/node/devcontainer.json
  .devcontainer/python/devcontainer.json
  .devcontainer/rust/devcontainer.json
```

- Paths are relative to the workspace folder
- Sorted alphabetically by subdirectory name
- Each path on its own line, indented with two spaces
- Header line instructs user to use `--config`
