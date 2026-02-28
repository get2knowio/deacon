# Quickstart: Named Config Folder Search

**Feature**: 014-named-config-search
**Date**: 2026-02-22

## Overview

This feature extends `ConfigLoader::discover_config()` to search for devcontainer.json files inside named subdirectories of `.devcontainer/`. The change is localized to 4 files and follows the existing shared discovery pattern.

## Implementation Order

### Step 1: Add `ConfigError::MultipleConfigs` variant

**File**: `crates/core/src/errors.rs`

Add to `ConfigError` enum:
```rust
/// Multiple configuration files found — user must select one
#[error("Multiple devcontainer configurations found. Use --config to specify one:\n{}", paths.join("\n"))]
MultipleConfigs { paths: Vec<String> },
```

Update existing error display test to cover the new variant.

### Step 2: Add `DiscoveryResult` enum and update `discover_config()`

**File**: `crates/core/src/config.rs`

1. Add `DiscoveryResult` enum near `ConfigLocation` (around line 204)
2. Add helper `fn check_config_file(dir: &Path) -> Option<PathBuf>` that checks for `devcontainer.json` then `devcontainer.jsonc` in a directory
3. Add helper `fn enumerate_named_configs(devcontainer_dir: &Path) -> Result<Vec<PathBuf>>` that enumerates and sorts subdirectories
4. Update `discover_config()` to return `DiscoveryResult` instead of `ConfigLocation`
5. Update existing unit tests, add new tests

**Key logic in `discover_config()`**:
```rust
// Priority 1: .devcontainer/devcontainer.json(c)
if let Some(path) = check_config_file(&workspace.join(".devcontainer")) {
    return Ok(DiscoveryResult::Single(path));
}

// Priority 2: root .devcontainer.json(c)
for ext in &["json", "jsonc"] {
    let path = workspace.join(format!(".devcontainer.{ext}"));
    if path.exists() {
        return Ok(DiscoveryResult::Single(path));
    }
}

// Priority 3: named config folders
let devcontainer_dir = workspace.join(".devcontainer");
if devcontainer_dir.is_dir() {
    let named = enumerate_named_configs(&devcontainer_dir)?;
    match named.len() {
        0 => {},
        1 => return Ok(DiscoveryResult::Single(named.into_iter().next().unwrap())),
        _ => return Ok(DiscoveryResult::Multiple(named)),
    }
}

Ok(DiscoveryResult::None(
    workspace.join(".devcontainer").join("devcontainer.json")
))
```

### Step 3: Update shared `load_config()` helper

**File**: `crates/deacon/src/commands/shared/config_loader.rs`

Update the `else` branch (when `args.config_path` is `None`) to match on `DiscoveryResult`:
```rust
} else {
    match ConfigLoader::discover_config(&workspace_folder)? {
        DiscoveryResult::Single(path) => path,
        DiscoveryResult::Multiple(paths) => {
            let display_paths: Vec<String> = paths.iter()
                .map(|p| p.strip_prefix(&workspace_folder)
                    .unwrap_or(p)
                    .display()
                    .to_string())
                .map(|p| format!("  {p}"))
                .collect();
            return Err(DeaconError::Config(ConfigError::MultipleConfigs {
                paths: display_paths,
            }));
        }
        DiscoveryResult::None(default) => default,
    }
};
```

Update existing tests, add test for multiple configs error.

### Step 4: Update `down` command

**File**: `crates/deacon/src/commands/down.rs`

Update the `discover_config()` call to match on `DiscoveryResult`:
```rust
let config_result = if let Some(config_path) = args.config_path.as_ref() {
    ConfigLoader::load_from_path(config_path)
} else {
    match ConfigLoader::discover_config(workspace_folder)? {
        DiscoveryResult::Single(path) => ConfigLoader::load_from_path(&path),
        DiscoveryResult::Multiple(paths) => {
            let display_paths = paths.iter()
                .map(|p| format!("  {}", p.strip_prefix(workspace_folder).unwrap_or(p).display()))
                .collect();
            return Err(anyhow::anyhow!(DeaconError::Config(
                ConfigError::MultipleConfigs { paths: display_paths }
            )));
        }
        DiscoveryResult::None(_) => {
            debug!("No configuration found, attempting auto-discovery from state");
            return execute_down_with_auto_discovery(workspace_folder, &args).await;
        }
    }
};
```

### Step 5: Write comprehensive tests

Add unit tests in `crates/core/src/config.rs`:
- Single named config auto-discovery
- Multiple named configs returns `Multiple`
- Priority 1 short-circuits over named configs
- Priority 2 short-circuits over named configs
- Subdirectories without config files are skipped
- Non-directory entries are ignored
- `.jsonc` files are discovered
- `.json` preferred over `.jsonc` in same directory
- Empty `.devcontainer/` directory
- Alphabetical ordering of results
- Deeply nested subdirectories are NOT found

Add integration tests in shared `config_loader.rs`:
- Multiple configs error via `load_config()`
- Single named config works end-to-end

## Build Verification

After each step:
```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings
make test-nextest-fast
```

Before PR:
```bash
make test-nextest
```

## Key Files Reference

| File | Change Type | Scope |
|------|-------------|-------|
| `crates/core/src/errors.rs` | Add variant | `MultipleConfigs` to `ConfigError` |
| `crates/core/src/config.rs` | Major change | `DiscoveryResult`, `discover_config()`, helpers, tests |
| `crates/deacon/src/commands/shared/config_loader.rs` | Moderate change | Handle `DiscoveryResult` in `load_config()` |
| `crates/deacon/src/commands/down.rs` | Moderate change | Handle `DiscoveryResult` in custom discovery |
