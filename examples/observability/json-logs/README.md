# Observability: JSON Logs Example

## What This Demonstrates

This example shows how to use structured JSON logging for machine-readable observability. It demonstrates:

- **JSON log format**: Structured log output suitable for log aggregation tools
- **Standardized spans**: Canonical span names like `config.resolve` for consistent tracing
- **Structured fields**: Key fields such as `workspace_id`, `duration_ms` for filtering and analysis
- **Runtime format selection**: Using `DEACON_LOG_FORMAT` environment variable to switch formats

## Why This Matters

JSON logging is essential for:
- **CI/CD integration**: Machine-parseable logs for automated analysis and monitoring
- **Log aggregation**: Integration with tools like Elasticsearch, Splunk, or CloudWatch
- **Debugging**: Structured fields enable precise filtering and querying
- **Observability**: Standardized spans provide consistent tracing across workflows

## DevContainer Specification References

This example aligns with the logging and output behavior in the Up and Read-Configuration SPECs: see ../../docs/subcommand-specs/up/SPEC.md#10-output-specifications and ../../docs/subcommand-specs/read-configuration/SPEC.md#10-output-specifications:

- **Standardized Tracing Spans**: Core workflow spans (`config.resolve`, `feature.plan`, etc.)
- **Common Fields**: Standard fields (`workspace_id`, `duration_ms`, `feature_id`, etc.)
- **Log Format Selection**: Runtime format via `DEACON_LOG_FORMAT` environment variable

## Standardized Span Names

The following canonical span names are emitted during core workflows:

- `config.resolve` - Configuration parsing and resolution
- `feature.plan` - Feature dependency planning
- `feature.install` - Feature installation execution
- `template.apply` - Template application workflow
- `container.build` - Container image building
- `container.create` - Container creation
- `lifecycle.run` - Lifecycle command execution
- `registry.pull` - OCI registry pull operations
- `registry.publish` - OCI registry publish operations

## Common Structured Fields

Spans include these standardized fields for filtering and correlation:

- `workspace_id` - 8-character hash of workspace path (for correlation)
- `duration_ms` - Operation duration in milliseconds (recorded on completion)
- `feature_id` - Feature identifier (in feature operations)
- `template_id` - Template identifier (in template operations)
- `container_id` - Container identifier (in container operations)
- `image_id` - Container image identifier (in build operations)
- `ref` - Registry reference (in registry operations)

## JSON Log Schema

Each JSON log entry follows this structure:

```json
{
  "timestamp": "2025-01-09T23:32:24.004390Z",
  "level": "INFO",
  "target": "deacon_core::observability",
  "span": {
    "name": "config.resolve",
    "workspace_id": "a1b2c3d4"
  },
  "message": "Configuration resolved successfully",
  "fields": {
    "duration_ms": 142
  }
}
```

## Run

### Basic JSON Logging

```sh
# Enable JSON format via environment variable
export DEACON_LOG_FORMAT=json
deacon read-configuration --config devcontainer.json
```

### Parse JSON Logs with jq

```sh
# Extract all span names
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq -r 'select(.span != null) | .span.name' \
  | sort -u

# Filter for config.resolve spans
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.name == "config.resolve")'

# Extract duration metrics
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.fields.duration_ms != null) | {span: .span.name, duration_ms: .fields.duration_ms}'

# Verify workspace_id field presence
export DEACON_LOG_FORMAT=json
deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.workspace_id != null) | .span.workspace_id' \
  | head -1
```

### Compare Text vs JSON Format

```sh
# Default text format (human-readable)
deacon read-configuration --config devcontainer.json

# JSON format (machine-readable)
DEACON_LOG_FORMAT=json deacon read-configuration --config devcontainer.json
```

### Integration with Log Analysis Tools

```sh
# Send to file for analysis
DEACON_LOG_FORMAT=json deacon config substitute --workspace-folder . 2> logs.jsonl

# Count log entries by level
jq -r '.level' < logs.jsonl | sort | uniq -c

# Find slowest operations
jq 'select(.fields.duration_ms != null) | {span: .span.name, duration: .fields.duration_ms}' < logs.jsonl \
  | jq -s 'sort_by(.duration) | reverse | .[0:5]'

# Extract all unique span names
jq -r 'select(.span != null) | .span.name' < logs.jsonl | sort -u
```

## Expected Output

When running with JSON logging enabled, you should see structured log entries like:

```json
{"timestamp":"2025-01-09T23:32:24.004390Z","level":"INFO","target":"deacon_core::observability","span":{"name":"config.resolve","workspace_id":"a1b2c3d4"},"message":"Starting configuration resolution"}
{"timestamp":"2025-01-09T23:32:24.146780Z","level":"INFO","target":"deacon_core::observability","span":{"name":"config.resolve","workspace_id":"a1b2c3d4"},"fields":{"duration_ms":142},"message":"Configuration resolved successfully"}
```

## Verification

To verify that JSON logs contain expected span names and fields:

```sh
# Check for config.resolve span
DEACON_LOG_FORMAT=json deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.name == "config.resolve")' \
  | grep -q "config.resolve" && echo "✓ config.resolve span found"

# Check for workspace_id field
DEACON_LOG_FORMAT=json deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.span.workspace_id != null)' \
  | grep -q "workspace_id" && echo "✓ workspace_id field found"

# Check for duration_ms field
DEACON_LOG_FORMAT=json deacon config substitute --workspace-folder . --output-format json 2>&1 \
  | jq 'select(.fields.duration_ms != null)' \
  | grep -q "duration_ms" && echo "✓ duration_ms field found"
```

## Notes

- JSON logs are written to **stderr**, while command output goes to **stdout**
- Use `2>&1` to capture both streams or `2>` to redirect only logs
- The `--output-format json` flag controls command output format, not logging format
- `DEACON_LOG_FORMAT=json` controls the logging format for observability
- All standardized spans follow the pattern `<domain>.<action>` (e.g., `config.resolve`, `feature.install`)
- The `workspace_id` is a deterministic 8-character hash for workspace correlation across operations
