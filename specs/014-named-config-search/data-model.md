# Data Model: Named Config Folder Search

**Feature**: 014-named-config-search
**Date**: 2026-02-22

## Entities

### DiscoveryResult (NEW)

Replaces the current `ConfigLocation` return type from `ConfigLoader::discover_config()`.

```rust
/// Result of devcontainer configuration discovery.
///
/// Represents the three possible outcomes when searching for
/// devcontainer.json across the three priority locations.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryResult {
    /// Exactly one configuration file was found.
    /// Contains the path to the discovered config file.
    Single(PathBuf),

    /// Multiple configuration files were found at the named config level.
    /// Contains all discovered paths, sorted alphabetically by subdirectory name.
    /// The caller should present these to the user and require explicit selection.
    Multiple(Vec<PathBuf>),

    /// No configuration file was found at any search location.
    /// Contains the default/preferred path for error messaging.
    None(PathBuf),
}
```

**Fields**:
| Variant | Payload | Description |
|---------|---------|-------------|
| `Single` | `PathBuf` | Path to the one discovered config file |
| `Multiple` | `Vec<PathBuf>` | All discovered config paths, sorted alphabetically |
| `None` | `PathBuf` | Default path (`.devcontainer/devcontainer.json`) for error context |

**Invariants**:
- `Multiple` always contains 2+ paths
- `Multiple` paths are sorted alphabetically by parent directory name
- `Single` path always exists on disk at time of construction
- `None` default path does NOT exist on disk

### ConfigError::MultipleConfigs (NEW)

New error variant added to the existing `ConfigError` enum.

```rust
/// Multiple configuration files found — user must select one
#[error("Multiple devcontainer configurations found. Use --config to specify one:\n{}",
    paths.iter().map(|p| format!("  {}", p)).collect::<Vec<_>>().join("\n"))]
MultipleConfigs { paths: Vec<String> },
```

**Fields**:
| Field | Type | Description |
|-------|------|-------------|
| `paths` | `Vec<String>` | Display-formatted paths to all discovered configs |

### ConfigLocation (EXISTING — unchanged)

The existing `ConfigLocation` struct is kept for backward compatibility in any internal usage but is no longer the return type of `discover_config()`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigLocation {
    pub path: PathBuf,
    pub exists: bool,
}
```

## State Transitions

Config discovery is stateless — it reads the filesystem and produces a `DiscoveryResult`. No state transitions apply.

## Validation Rules

| Rule | Source | Enforcement Point |
|------|--------|-------------------|
| Priority order: dir > root > named | FR-001 | `discover_config()` |
| Short-circuit on priority 1 or 2 match | FR-009 | `discover_config()` |
| One level deep only | FR-005 | `enumerate_named_configs()` |
| Alphabetical sort by subdirectory name | FR-005 | `enumerate_named_configs()` |
| Skip subdirs without config file | FR-006 | `enumerate_named_configs()` |
| Check both `.json` and `.jsonc` | FR-006, D2 | all search levels |
| Prefer `.json` over `.jsonc` in same dir | D3 | `check_config_in_dir()` |
| `--config` bypasses all discovery | FR-004 | `load_config()` (CLI layer) |
| Multiple configs → error with listing | FR-003, FR-008 | `load_config()` + `down` command |

## Relationships

```
ConfigLoadArgs --[config_path: Some]-->  bypass discovery
ConfigLoadArgs --[config_path: None]-->  ConfigLoader::discover_config()
                                              |
                                         DiscoveryResult
                                        /      |       \
                                   Single   Multiple   None
                                      |        |         |
                                  load_with_  error    fallback/
                                  extends()  listing   not_found
```
