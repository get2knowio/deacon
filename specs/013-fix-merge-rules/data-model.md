# Data Model: Fix Config Merge Rules

**Branch**: `013-fix-merge-rules` | **Date**: 2026-02-22

## Entities

### DevContainerConfig (existing, unchanged)

The primary configuration structure. No fields are added or removed. The merge *behavior* changes for specific field categories.

**Relevant fields by merge category:**

| Field | Type | Current Merge | Target Merge |
|-------|------|---------------|--------------|
| `privileged` | `Option<bool>` | `Option::or()` (last-wins) | Boolean OR |
| `init` | `Option<bool>` | `Option::or()` (last-wins) | Boolean OR |
| `mounts` | `Vec<serde_json::Value>` | Replace if overlay non-empty | Union with dedup |
| `forward_ports` | `Vec<PortSpec>` | Replace if overlay non-empty | Union with dedup |

**Unchanged merge categories (FR-007):**

| Category | Fields | Merge Behavior |
|----------|--------|---------------|
| Scalar (last-wins) | `name`, `image`, `dockerfile`, `build`, `workspace_folder`, `workspace_mount`, `app_port`, `shutdown_action`, `override_command`, `container_user`, `remote_user`, `update_remote_user_uid`, `host_requirements`, `other_ports_attributes` | `overlay.or(base)` |
| Map (key merge) | `container_env`, `remote_env`, `ports_attributes` | Deep merge, overlay wins per key |
| Object (deep merge) | `features`, `customizations` | Recursive JSON object merge |
| Concat arrays | `run_args`, `cap_add`, `security_opt` | `concat_string_arrays()` |
| Lifecycle (last-wins) | `on_create_command`, `post_create_command`, `post_start_command`, `post_attach_command`, `initialize_command`, `update_content_command` | `overlay.or(base)` |

### PortSpec (existing, unchanged)

```rust
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortSpec {
    Number(u16),
    String(String),
}
```

Already derives `PartialEq`. `PortSpec::Number(3000)` and `PortSpec::String("3000:3000")` are distinct values per the spec.

## New Functions

### `merge_bool_or(base: Option<bool>, overlay: Option<bool>) -> Option<bool>`

Truth table:

| Base | Overlay | Result |
|------|---------|--------|
| `None` | `None` | `None` |
| `None` | `Some(true)` | `Some(true)` |
| `None` | `Some(false)` | `Some(false)` |
| `Some(true)` | `None` | `Some(true)` |
| `Some(true)` | `Some(true)` | `Some(true)` |
| `Some(true)` | `Some(false)` | `Some(true)` |
| `Some(false)` | `None` | `Some(false)` |
| `Some(false)` | `Some(true)` | `Some(true)` |
| `Some(false)` | `Some(false)` | `Some(false)` |

### `union_json_arrays(base: &[serde_json::Value], overlay: &[serde_json::Value]) -> Vec<serde_json::Value>`

Algorithm:
1. Start with `result = base.to_vec()`
2. For each entry in overlay: if `!result.contains(&entry)`, append it
3. Return result

### `union_port_arrays(base: &[PortSpec], overlay: &[PortSpec]) -> Vec<PortSpec>`

Algorithm:
1. Start with `result = base.to_vec()`
2. For each entry in overlay: if `!result.contains(&entry)`, append it
3. Return result

## State Transitions

N/A — this is a stateless merge operation with no lifecycle transitions.

## Validation Rules

- Boolean OR preserves `None` when both inputs are `None` (FR-006)
- Union preserves base ordering, appends new overlay entries (FR-002, FR-003)
- Deduplication is by value equality, not by semantic equivalence (FR-004, FR-005)
