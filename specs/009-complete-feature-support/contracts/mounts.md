# Contract: Feature Mount Merging

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Purpose

Defines the contract for merging mount specifications from features and config.

---

## Input Contract

### Feature Mounts

```rust
pub struct FeatureMetadata {
    /// Mount specifications (string format)
    pub mounts: Vec<String>,
    // ... other fields
}
```

### Config Mounts

```rust
pub struct DevContainerConfig {
    /// Mount specifications (string or object format)
    pub mounts: Vec<serde_json::Value>,
    // ... other fields
}
```

### Mount Formats

**String Format**:
```
type=bind,source=/host/path,target=/container/path
type=volume,source=myvolume,target=/data
type=tmpfs,target=/tmp
```

**Object Format**:
```json
{
    "type": "bind",
    "source": "/host/path",
    "target": "/container/path"
}
```

---

## Output Contract

### MergedMounts

```rust
pub struct MergedMounts {
    /// Final mount strings in Docker CLI format
    pub mounts: Vec<String>,
}
```

---

## Merge Rules

### Rule 1: Config Takes Precedence

For mounts with the same target path, config mount overwrites feature mount:

```
mount_map = {}

FOR feature IN features (installation order):
    FOR mount IN feature.mounts:
        mount_map[mount.target] = mount

FOR mount IN config.mounts:
    mount_map[mount.target] = mount  // Overwrites features

result = mount_map.values()
```

### Rule 2: Preserve Declaration Order

Within each source (feature or config), mounts are processed in declaration order.
Final order is: features (in installation order) then config, with later overwrites.

### Rule 3: Normalize to String Format

All mounts (string or object) are normalized to Docker CLI string format:

```
type={type},source={source},target={target}[,readonly][,...]
```

### Rule 4: Validate During Merge

Invalid mount specifications produce errors with source attribution:

```
"Invalid mount in feature {feature_id}: {mount_string}: {error}"
"Invalid mount in config: {mount_string}: {error}"
```

---

## Function Signature

```rust
/// Merge mounts from features and config
///
/// # Arguments
/// * `config_mounts` - Mounts from devcontainer.json
/// * `features` - Resolved features in installation order
///
/// # Returns
/// * `Ok(MergedMounts)` - Deduplicated mounts (by target)
/// * `Err` - Invalid mount specification
///
/// # Precedence
/// Config mounts override feature mounts for same target path
pub fn merge_mounts(
    config_mounts: &[serde_json::Value],
    features: &[ResolvedFeature],
) -> Result<MergedMounts>;
```

---

## Parsing Contract

The existing `MountParser` handles mount parsing:

```rust
pub struct ParsedMount {
    pub mount_type: MountType,  // bind, volume, tmpfs
    pub source: Option<String>, // Not required for tmpfs
    pub target: String,         // Required for all
    pub readonly: bool,
    pub options: HashMap<String, String>,
}

pub enum MountType {
    Bind,
    Volume,
    Tmpfs,
}
```

---

## Examples

### Example 1: No Conflicts

**Feature "cache"**:
```json
"mounts": ["type=volume,source=cache,target=/cache"]
```

**Config**:
```json
"mounts": ["type=bind,source=${localWorkspaceFolder}/data,target=/data"]
```

**Merged Result**:
```
["type=volume,source=cache,target=/cache", "type=bind,source=/host/data,target=/data"]
```

### Example 2: Config Overrides Feature

**Feature "data"**:
```json
"mounts": ["type=volume,source=feature-data,target=/data"]
```

**Config**:
```json
"mounts": ["type=bind,source=${localWorkspaceFolder}/my-data,target=/data"]
```

**Merged Result**:
```
["type=bind,source=/host/my-data,target=/data"]
```
(Config mount to `/data` overwrites feature mount to `/data`)

### Example 3: Multiple Features

**Feature 1 (installed first)**:
```json
"mounts": ["type=volume,source=vol1,target=/vol1"]
```

**Feature 2 (installed second)**:
```json
"mounts": ["type=volume,source=vol2,target=/vol2", "type=volume,source=shared,target=/shared"]
```

**Feature 3 (installed third)**:
```json
"mounts": ["type=volume,source=override-shared,target=/shared"]
```

**Merged Result**:
```
[
    "type=volume,source=vol1,target=/vol1",
    "type=volume,source=vol2,target=/vol2",
    "type=volume,source=override-shared,target=/shared"
]
```
(Feature 3's mount to `/shared` overwrites Feature 2's)

### Example 4: Object Format Normalization

**Feature**:
```json
"mounts": [
    {"type": "bind", "source": "/host/path", "target": "/container"}
]
```

**Normalized Output**:
```
["type=bind,source=/host/path,target=/container"]
```

---

## Docker CLI Mapping

Merged mounts map to Docker `--mount` flags:

```bash
docker create \
    --mount type=bind,source=/host/path,target=/container \
    --mount type=volume,source=myvolume,target=/data \
    ...
```

---

## Error Scenarios

| Scenario | Error Message |
|----------|---------------|
| Missing target | "Invalid mount in feature {id}: {mount}: target is required" |
| Invalid mount type | "Invalid mount in feature {id}: {mount}: unknown type '{type}'" |
| Missing source for bind | "Invalid mount in feature {id}: {mount}: bind mount requires source" |
| Malformed string | "Invalid mount in feature {id}: {mount}: parse error: {details}" |

---

## Variable Substitution

Mount specifications may contain devcontainer variables:
- `${localWorkspaceFolder}` - Host workspace path
- `${containerWorkspaceFolder}` - Container workspace path
- Other devcontainer variables

Variable substitution is performed BEFORE merge, using existing variable expansion logic.

---

## Testing Requirements

1. **Unit Tests**: Merge logic, precedence, deduplication
2. **Parse Tests**: String format, object format, invalid formats
3. **Integration Tests**: Verify mounts appear in container
4. **Edge Cases**: Empty mounts, all same target, no features
