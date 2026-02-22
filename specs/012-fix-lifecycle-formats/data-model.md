# Data Model: Fix Lifecycle Command Format Support

**Feature Branch**: `012-fix-lifecycle-formats`
**Date**: 2026-02-21

## Entity: LifecycleCommandValue

Represents a parsed lifecycle command with preserved format semantics.

**Location**: `crates/core/src/container_lifecycle.rs` (or new `lifecycle_format.rs` if file grows)

```rust
/// A parsed lifecycle command value that preserves format semantics.
///
/// The DevContainer spec defines three formats for lifecycle commands:
/// - String: executed through a shell (`/bin/sh -c` in container, platform shell on host)
/// - Array: exec-style, passed directly to OS without shell interpretation
/// - Object: named parallel commands, each value is itself a Shell or Exec command
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleCommandValue {
    /// Shell-interpreted command string (e.g., "npm install && npm build")
    Shell(String),

    /// Exec-style command as program + arguments (e.g., ["npm", "install"])
    /// First element is the executable, remaining are arguments.
    Exec(Vec<String>),

    /// Named parallel commands (e.g., {"install": "npm install", "build": ["npm", "run", "build"]})
    /// Uses IndexMap to preserve declaration order from JSON.
    /// Values are Shell or Exec variants (not nested Parallel).
    Parallel(indexmap::IndexMap<String, LifecycleCommandValue>),
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `Shell(String)` | variant | Command string executed via shell. Supports `&&`, pipes, redirects. |
| `Exec(Vec<String>)` | variant | Program + args passed directly to OS. No shell interpretation. |
| `Parallel(IndexMap<String, LifecycleCommandValue>)` | variant | Named concurrent commands. Values are `Shell` or `Exec` only. |

### Validation Rules

- `Shell("")` → treated as no-op (skip)
- `Exec(vec![])` → treated as no-op (skip)
- `Parallel` with empty map → treated as no-op (skip)
- `Parallel` values must be `Shell` or `Exec` — never nested `Parallel`
- `Exec` elements must all be strings (validated during parsing)

### State Transitions

```
serde_json::Value  -->  LifecycleCommandValue  -->  Execution
     (raw JSON)         (parsed, validated)       (shell/exec/parallel)
```

Parsing: `LifecycleCommandValue::from_json_value(&serde_json::Value) -> Result<Self>`
- `Value::Null` → `None` (filtered at aggregation layer)
- `Value::String(s)` → `Shell(s)`
- `Value::Array(arr)` → `Exec(arr)` (validates all elements are strings)
- `Value::Object(map)` → `Parallel(map)` (recursively parses each value as Shell or Exec)
- Other types → `Err`

## Entity: AggregatedLifecycleCommand (Modified)

The existing `AggregatedLifecycleCommand` is modified to carry `LifecycleCommandValue` instead of `serde_json::Value`.

```rust
/// A lifecycle command with source attribution.
#[derive(Debug, Clone)]
pub struct AggregatedLifecycleCommand {
    /// The parsed command value with format semantics preserved
    pub command: LifecycleCommandValue,
    /// Where this command came from (feature or config)
    pub source: LifecycleCommandSource,
}
```

### Relationship

- Each `AggregatedLifecycleCommand` contains exactly one `LifecycleCommandValue`
- Multiple `AggregatedLifecycleCommand` are collected into `LifecycleCommandList`
- Source attribution is preserved for error reporting

## Entity: ParallelCommandResult (New)

Result from executing a single entry within a parallel (object-format) command set.

```rust
/// Result of executing one named command within a parallel set.
#[derive(Debug, Clone)]
pub struct ParallelCommandResult {
    /// The named key from the object (e.g., "install", "build")
    pub key: String,
    /// Exit code from the command
    pub exit_code: i32,
    /// Duration of execution
    pub duration: std::time::Duration,
    /// Whether the command succeeded
    pub success: bool,
}
```

## Entity Relationships

```
DevContainerConfig
  └── lifecycle fields: Option<serde_json::Value>  (6 fields, unchanged)

FeatureMetadata
  └── lifecycle fields: Option<serde_json::Value>  (6 fields, unchanged)

aggregate_lifecycle_commands()
  ├── input: features + config (serde_json::Value)
  ├── filters: is_empty_command()
  └── output: LifecycleCommandList
        └── Vec<AggregatedLifecycleCommand>
              ├── command: LifecycleCommandValue  ← NEW (was serde_json::Value)
              └── source: LifecycleCommandSource

execute_lifecycle_phase_impl()
  ├── input: Vec<AggregatedLifecycleCommand>
  ├── for Shell: wrap in sh -c, Docker exec
  ├── for Exec: pass args directly to Docker exec
  └── for Parallel: spawn concurrent tasks via JoinSet
        └── each entry → Shell or Exec execution
        └── collect ParallelCommandResult per entry

execute_host_lifecycle_phase()
  ├── input: Vec<AggregatedLifecycleCommand>  ← NEW (was Vec<String>)
  ├── for Shell: sh -c on host
  ├── for Exec: direct Command::new() on host
  └── for Parallel: thread::scope or spawn_blocking
```

## Impact on Existing Types

| Type | Change | Reason |
|------|--------|--------|
| `AggregatedLifecycleCommand.command` | `serde_json::Value` → `LifecycleCommandValue` | Preserve format semantics |
| `aggregate_lifecycle_commands()` | Parse `Value` → `LifecycleCommandValue` during aggregation | Single parsing point |
| `execute_lifecycle_phase_impl()` | Branch on `LifecycleCommandValue` variant | Format-aware execution |
| `execute_host_lifecycle_phase()` | Accept `AggregatedLifecycleCommand` instead of `Vec<String>` | Format-aware host execution |
| `commands_from_json_value()` | **Remove** (replaced by `LifecycleCommandValue::from_json_value`) | Centralize parsing in core |
| `flatten_aggregated_commands()` | **Remove** (no longer needed) | Flattening loses format info |
| `LifecycleCommands::from_json_value()` | Extend to support object format | Host-side format parity |
