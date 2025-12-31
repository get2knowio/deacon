# Contract: Lifecycle Command Aggregation

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Purpose

Defines the contract for aggregating and executing lifecycle commands from features and config during `deacon up`.

---

## Lifecycle Phases

| Phase | Blocking | Execution Context |
|-------|----------|-------------------|
| `onCreateCommand` | Yes | In container, after creation |
| `updateContentCommand` | Yes | In container, after onCreate |
| `postCreateCommand` | Yes | In container, after updateContent |
| `postStartCommand` | No | In container, background |
| `postAttachCommand` | No | In container, background |

---

## Input Contract

### Feature Lifecycle Commands

```rust
pub struct FeatureMetadata {
    pub on_create_command: Option<serde_json::Value>,
    pub update_content_command: Option<serde_json::Value>,
    pub post_create_command: Option<serde_json::Value>,
    pub post_start_command: Option<serde_json::Value>,
    pub post_attach_command: Option<serde_json::Value>,
    // ... other fields
}
```

### Config Lifecycle Commands

```rust
pub struct DevContainerConfig {
    pub on_create_command: Option<serde_json::Value>,
    pub update_content_command: Option<serde_json::Value>,
    pub post_create_command: Option<serde_json::Value>,
    pub post_start_command: Option<serde_json::Value>,
    pub post_attach_command: Option<serde_json::Value>,
    // ... other fields
}
```

### Command Value Format

Commands can be:
- `null` - No command (skip)
- `""` - Empty string (skip)
- `"command"` - Single command string
- `["cmd1", "arg1", "arg2"]` - Command with arguments
- `{"cmd1": "...", "cmd2": "..."}` - Parallel commands (object)

---

## Output Contract

### LifecycleCommandList

```rust
pub struct AggregatedLifecycleCommand {
    pub command: serde_json::Value,
    pub source: LifecycleCommandSource,
}

pub enum LifecycleCommandSource {
    Feature { id: String },
    Config,
}

pub struct LifecycleCommandList {
    pub commands: Vec<AggregatedLifecycleCommand>,
}
```

---

## Aggregation Rules

### Rule 1: Feature Commands Before Config

```
ORDER = [
    feature1.command,  // First installed feature
    feature2.command,  // Second installed feature
    ...,
    config.command     // Config command last
]
```

**Rationale**: Features set up prerequisites that config commands may depend on.

### Rule 2: Feature Order = Installation Order

Features are processed in the order determined by `FeatureDependencyResolver`:
1. Features with no dependencies first
2. Features with satisfied dependencies next
3. Respects `installsAfter` and `dependsOn` constraints

### Rule 3: Filter Empty Commands

Commands are filtered BEFORE aggregation if they are:
- `null`
- Empty string `""`
- Empty array `[]`
- Empty object `{}`

### Rule 4: Preserve All Non-Empty Commands

Unlike security options (which merge), lifecycle commands are NOT merged:
- Each feature's command runs independently
- Config command runs after all feature commands
- No deduplication

---

## Function Signatures

```rust
/// Aggregate lifecycle commands for a specific phase
///
/// # Arguments
/// * `phase` - Which lifecycle phase to aggregate
/// * `features` - Resolved features in installation order
/// * `config` - DevContainerConfig with user commands
///
/// # Returns
/// LifecycleCommandList with feature commands first, then config
pub fn aggregate_lifecycle_commands(
    phase: LifecyclePhase,
    features: &[ResolvedFeature],
    config: &DevContainerConfig,
) -> LifecycleCommandList;

/// Check if a command value is empty/null
pub fn is_empty_command(cmd: &serde_json::Value) -> bool;
```

---

## Execution Contract

### Rule 5: Fail-Fast on Error

```
FOR EACH command IN command_list:
    result = execute(command)
    IF result.exit_code != 0:
        RETURN Error("Lifecycle command failed: {source}")
        // ALL remaining commands are SKIPPED
```

### Rule 6: Exit Code Propagation

| Command Exit | Result |
|--------------|--------|
| 0 | Continue to next command |
| Non-zero | Stop immediately, exit up with code 1 |

### Rule 7: Error Attribution

Error messages MUST include the command source:
- Feature: `"Lifecycle command failed (feature:node): npm install"`
- Config: `"Lifecycle command failed (config): ./setup.sh"`

---

## Examples

### Example 1: Basic Ordering

**Input**:
- Feature "node" (installed first): `onCreateCommand: "npm install"`
- Feature "python" (installed second): `onCreateCommand: "pip install -r requirements.txt"`
- Config: `onCreateCommand: "echo ready"`

**Output** (LifecycleCommandList):
```json
[
    {"command": "npm install", "source": "feature:node"},
    {"command": "pip install -r requirements.txt", "source": "feature:python"},
    {"command": "echo ready", "source": "config"}
]
```

### Example 2: Empty Command Filtering

**Input**:
- Feature "node": `onCreateCommand: null`
- Feature "python": `onCreateCommand: "pip install"`
- Config: `onCreateCommand: ""`

**Output** (LifecycleCommandList):
```json
[
    {"command": "pip install", "source": "feature:python"}
]
```

### Example 3: Complex Command Format

**Input**:
- Feature "node": `onCreateCommand: {"npm": "npm install", "build": "npm run build"}`
- Config: `onCreateCommand: ["./setup.sh", "--verbose"]`

**Output** (LifecycleCommandList):
```json
[
    {"command": {"npm": "npm install", "build": "npm run build"}, "source": "feature:node"},
    {"command": ["./setup.sh", "--verbose"], "source": "config"}
]
```

---

## Error Scenarios

| Scenario | Behavior |
|----------|----------|
| Feature command fails (exit 1) | Stop immediately, skip remaining commands, exit up with code 1 |
| Config command fails | Stop immediately, exit up with code 1 |
| Command execution timeout | Existing timeout behavior applies, propagate error |
| Invalid command format | Error at execution time with source attribution |

---

## Testing Requirements

1. **Unit Tests**: Test aggregation logic, ordering, filtering
2. **Integration Tests**: Verify execution order with real container
3. **Error Tests**: Verify fail-fast behavior and error messages
4. **Edge Cases**: All empty commands, single feature, no features
