# Progress Events Example

## What This Demonstrates

This example shows how to track container lifecycle execution using progress events:

- **Per-command progress events** with stable IDs
- **Event ordering** and timing information
- **JSON structured logging** for automation
- **Event types** for each lifecycle phase and command

## Progress Event System

According to the [Up SPEC](../../../docs/subcommand-specs/up/SPEC.md#10-output-specifications) and implementation in `crates/core/src/progress.rs`, the deacon CLI emits structured progress events throughout the lifecycle execution.

### Event Types

Each lifecycle phase emits events:
- `lifecycle.phase.begin` - Phase starts (onCreate, postCreate, etc.)
- `lifecycle.command.begin` - Individual command starts
- `lifecycle.command.end` - Individual command completes
- `lifecycle.phase.end` - Phase completes

### Event Structure

Each event includes:
- **id**: Monotonically increasing event ID (stable, sequential)
- **type**: Event type (e.g., "lifecycle.command.begin")
- **timestamp**: Unix timestamp in milliseconds
- **phase**: Lifecycle phase name (onCreate, postCreate, postStart, postAttach)
- **command_id**: Stable command identifier (for per-command events)
- **command**: The actual command being executed (redacted if needed)
- **context**: Additional context information

## Testing Progress Events

### 1. Basic Configuration Validation

First, verify the configuration is valid:

```bash
deacon read-configuration --config devcontainer.json
```

### 2. JSON Progress Output to File

Enable JSON progress events and write to a file:

```bash
# In a real scenario (requires Docker):
# deacon up --progress json --progress-file progress.json --workspace-folder .
#
# This creates progress.json with one JSON event per line (JSONL format)
```

### 3. Analyze Progress Events with jq

Once you have a progress file, analyze the events:

#### Count events by type
```bash
cat progress.json | jq -r '.type' | sort | uniq -c
```

Expected output (counts may vary):
```
  4 lifecycle.command.begin
  4 lifecycle.command.end
  4 lifecycle.phase.begin
  4 lifecycle.phase.end
```

#### Show event ordering with IDs
```bash
cat progress.json | jq -r '[.id, .type, .phase // "N/A"] | @tsv' | sort -n
```

Expected output shows sequential IDs:
```
1       lifecycle.phase.begin   onCreate
2       lifecycle.command.begin onCreate
3       lifecycle.command.end   onCreate
...
```

#### Extract command execution details
```bash
cat progress.json | jq 'select(.type == "lifecycle.command.begin") | {
  id: .id,
  phase: .phase,
  command_id: .command_id,
  command: .command
}'
```

Example output:
```json
{
  "id": 2,
  "phase": "onCreate",
  "command_id": "onCreate.0",
  "command": "echo 'onCreate: Step 1 - Creating directories'"
}
{
  "id": 5,
  "phase": "postCreate",
  "command_id": "postCreate.install-tools",
  "command": "echo 'postCreate: Installing essential tools'"
}
```

#### Show phase execution timeline
```bash
cat progress.json | jq 'select(.type | endswith("phase.begin") or endswith("phase.end")) | {
  id: .id,
  type: .type,
  phase: .phase,
  timestamp: .timestamp
}'
```

#### Verify event ID uniqueness and ordering
```bash
cat progress.json | jq -s 'map(.id) | . as $ids | ($ids | sort) == $ids and ($ids | unique | length) == ($ids | length)'
```

Should return `true` if all IDs are unique and sequential.

### 4. JSON Logging with DEACON_LOG_FORMAT

Enable structured JSON logging for all output:

```bash
# In a real scenario (requires Docker):
# DEACON_LOG_FORMAT=json deacon up --progress json --progress-file progress.json --workspace-folder .
#
# All log messages will be in JSON format for parsing
```

### 5. Silent Mode with Progress File

Use silent mode for clean progress-only output:

```bash
# In a real scenario (requires Docker):
# deacon up --progress json --progress-file progress.json --workspace-folder . 2>/dev/null
#
# No terminal output, only progress events in file
```

## Command ID Format

Command IDs follow a stable pattern based on lifecycle phase and command index/name:

### Array Commands
For array-based commands (onCreate):
- `onCreate.0` - First command
- `onCreate.1` - Second command
- `onCreate.2` - Third command

### Object Commands
For object-based commands (postCreate with named commands):
- `postCreate.install-tools` - Named "install-tools"
- `postCreate.configure-env` - Named "configure-env"
- `postCreate.setup-deps` - Named "setup-deps"

### String Commands
For string-based commands (postAttach):
- `postAttach.0` - Single command treated as array element

## Event Ordering Guarantees

The progress event system guarantees:

1. **Sequential IDs**: Event IDs are monotonically increasing
2. **Phase ordering**: Phases emit events in lifecycle order (onCreate → postCreate → postStart → postAttach)
3. **Command ordering**: Within a phase, commands emit events in array/object order
4. **Begin-End pairing**: Each command.begin has a corresponding command.end
5. **Nested phases**: Phase events wrap all command events

## Practical Use Cases

### CI/CD Integration
```bash
# Parse progress events to track build progress
cat progress.json | jq -r 'select(.type == "lifecycle.phase.end") | "\(.phase) completed in \(.duration_ms)ms"'
```

### Error Detection
```bash
# Find failed commands
cat progress.json | jq 'select(.type == "lifecycle.command.end" and .exit_code != 0) | {
  command_id: .command_id,
  exit_code: .exit_code,
  command: .command
}'
```

### Performance Analysis
```bash
# Find slowest commands
cat progress.json | jq 'select(.type == "lifecycle.command.end") | {
  command_id: .command_id,
  duration: .duration_ms,
  command: .command
}' | jq -s 'sort_by(.duration) | reverse | .[0:5]'
```

### Timeline Visualization
```bash
# Create a simple timeline
cat progress.json | jq -r 'select(.type == "lifecycle.command.begin" or .type == "lifecycle.command.end") | 
  "\(.timestamp) \(.type | split(".") | .[-1]) \(.command_id)"'
```

## Example Progress Event

Here's what a typical lifecycle.command.begin event looks like:

```json
{
  "id": 5,
  "type": "lifecycle.command.begin",
  "timestamp": 1704067200000,
  "phase": "postCreate",
  "command_id": "postCreate.install-tools",
  "command": "echo 'postCreate: Installing essential tools'",
  "index": 0,
  "total_commands": 3,
  "context": {
    "container_id": "abc123...",
    "workspace_folder": "/workspace"
  }
}
```

## Key Takeaways

- **Progress events are machine-readable** - Perfect for automation and monitoring
- **Command IDs are stable** - Use them to track specific commands across runs
- **Event IDs are sequential** - Easy to verify ordering and completeness
- **JSON format enables powerful analysis** - Use jq for filtering, sorting, aggregation
- **Works offline** - All examples can be validated without Docker using `read-configuration`

## References

- [Up SPEC: Output and Progress](../../../docs/subcommand-specs/up/SPEC.md#10-output-specifications)
- Implementation: `crates/core/src/progress.rs`
- Tests: `crates/deacon/tests/integration_progress.rs`, `crates/core/tests/integration_per_command_events.rs`
- Related issues: #107, #124, #110, #115, #125
