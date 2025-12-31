# Contract: Feature Entrypoint Chaining

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Purpose

Defines the contract for chaining entrypoints from multiple features when creating containers.

---

## Background

Docker containers support only a single entrypoint. When multiple features define entrypoints, they must be chained via a wrapper script to ensure all initialization occurs in the correct order.

---

## Input Contract

### Feature Entrypoint

```rust
pub struct FeatureMetadata {
    /// Optional entrypoint script/command
    pub entrypoint: Option<String>,
    // ... other fields
}
```

### Config Entrypoint

```rust
pub struct DevContainerConfig {
    /// Override the container's default entrypoint
    pub override_command: Option<bool>,
    /// Custom entrypoint (when overriding)
    pub entrypoint: Option<String>,
    // ... other fields
}
```

---

## Output Contract

### EntrypointChain

```rust
#[derive(Debug, Clone)]
pub enum EntrypointChain {
    /// No entrypoint specified by any source
    None,
    /// Single entrypoint (no wrapper needed)
    Single(String),
    /// Multiple entrypoints requiring wrapper
    Chained {
        /// Path to wrapper script in container
        wrapper_path: String,
        /// Original entrypoints in execution order
        entrypoints: Vec<String>,
    },
}
```

---

## Chaining Rules

### Rule 1: Feature Order

Entrypoints execute in feature installation order:

```
1. Feature 1 entrypoint (first installed)
2. Feature 2 entrypoint (second installed)
3. ...
4. Config entrypoint (if specified, last)
```

### Rule 2: Skip Null Entrypoints

Features without entrypoints are skipped:

```
IF feature.entrypoint IS None:
    SKIP
```

### Rule 3: Single Entrypoint Optimization

If only one entrypoint total, use it directly (no wrapper):

```
IF count(entrypoints) == 1:
    RETURN EntrypointChain::Single(entrypoint)
```

### Rule 4: Wrapper Script Generation

Multiple entrypoints require a wrapper script:

```bash
#!/bin/sh
/path/to/feature1/entrypoint.sh || exit $?
/path/to/feature2/entrypoint.sh || exit $?
exec "$@"
```

---

## Function Signatures

```rust
/// Build entrypoint chain from features and config
///
/// # Arguments
/// * `features` - Resolved features in installation order
/// * `config_entrypoint` - Optional entrypoint from config
///
/// # Returns
/// EntrypointChain describing how to set container entrypoint
pub fn build_entrypoint_chain(
    features: &[ResolvedFeature],
    config_entrypoint: Option<&str>,
) -> EntrypointChain;

/// Generate wrapper script content for chained entrypoints
///
/// # Arguments
/// * `entrypoints` - List of entrypoint paths in execution order
///
/// # Returns
/// Shell script content as string
pub fn generate_wrapper_script(entrypoints: &[String]) -> String;
```

---

## Wrapper Script Contract

### Structure

```bash
#!/bin/sh
# Feature entrypoints
{entrypoint_1} || exit $?
{entrypoint_2} || exit $?
...
# Pass control to command
exec "$@"
```

### Properties

1. **Fail-fast**: Each entrypoint failure stops execution
2. **Exit code preservation**: Failed entrypoint's exit code is returned
3. **Command passthrough**: `exec "$@"` passes user command to final process
4. **Shebang**: Uses `/bin/sh` for maximum compatibility

### Location

Wrapper script is written to container data folder:
```
{container_data_folder}/entrypoint-wrapper.sh
```

---

## Docker CLI Mapping

| EntrypointChain | Docker Flag |
|-----------------|-------------|
| `None` | No `--entrypoint` flag (use image default) |
| `Single(path)` | `--entrypoint {path}` |
| `Chained { wrapper_path, .. }` | `--entrypoint {wrapper_path}` |

---

## Examples

### Example 1: No Entrypoints

**Features**: No entrypoints defined
**Config**: No entrypoint

**Result**: `EntrypointChain::None`

### Example 2: Single Feature Entrypoint

**Feature "docker-in-docker"**:
```json
"entrypoint": "/usr/local/share/docker-init.sh"
```

**Result**: `EntrypointChain::Single("/usr/local/share/docker-init.sh")`

### Example 3: Multiple Feature Entrypoints

**Feature 1**:
```json
"entrypoint": "/feature1/init.sh"
```

**Feature 2**:
```json
"entrypoint": "/feature2/init.sh"
```

**Generated Wrapper**:
```bash
#!/bin/sh
/feature1/init.sh || exit $?
/feature2/init.sh || exit $?
exec "$@"
```

**Result**:
```rust
EntrypointChain::Chained {
    wrapper_path: "/devcontainer/entrypoint-wrapper.sh",
    entrypoints: vec!["/feature1/init.sh", "/feature2/init.sh"],
}
```

### Example 4: Feature and Config Entrypoints

**Feature**:
```json
"entrypoint": "/feature/init.sh"
```

**Config**:
```json
"entrypoint": "/custom/init.sh"
```

**Generated Wrapper**:
```bash
#!/bin/sh
/feature/init.sh || exit $?
/custom/init.sh || exit $?
exec "$@"
```

### Example 5: Features with Gaps

**Feature 1**: `entrypoint: "/f1/init.sh"`
**Feature 2**: `entrypoint: null` (no entrypoint)
**Feature 3**: `entrypoint: "/f3/init.sh"`

**Generated Wrapper**:
```bash
#!/bin/sh
/f1/init.sh || exit $?
/f3/init.sh || exit $?
exec "$@"
```

(Feature 2 is skipped because it has no entrypoint)

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Entrypoint script not found | Runtime error from Docker (not caught during setup) |
| Entrypoint exits non-zero | Container creation fails, exit code preserved |
| Wrapper script write fails | Error: "Failed to create entrypoint wrapper: {error}" |

---

## Container Data Folder

The wrapper script location depends on `--container-data-folder`:

```
--container-data-folder=/devcontainer
  → /devcontainer/entrypoint-wrapper.sh

--container-data-folder=/tmp/deacon
  → /tmp/deacon/entrypoint-wrapper.sh
```

If no container data folder specified, create in temp location.

---

## User Command Interaction

The entrypoint chain is designed to work with user commands:

```bash
# Docker run equivalent
docker run --entrypoint /wrapper.sh image user-command args

# Wrapper executes:
# 1. /feature1/init.sh
# 2. /feature2/init.sh
# 3. exec user-command args
```

The `exec "$@"` ensures:
- User command becomes the main process (PID 1 behavior)
- Signals are properly forwarded
- Exit code comes from user command

---

## Testing Requirements

1. **Unit Tests**: Chain building logic, wrapper generation
2. **Integration Tests**: Container starts with chained entrypoints
3. **Edge Cases**: No entrypoints, single entrypoint, many entrypoints
4. **Error Tests**: Entrypoint failure propagation
