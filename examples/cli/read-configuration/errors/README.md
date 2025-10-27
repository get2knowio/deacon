# Error Scenarios

This directory contains examples of common error scenarios and their error messages.

## 1. Missing Configuration File

```bash
deacon read-configuration --workspace-folder /tmp --config /tmp/nonexistent.json
```

**Error:**
```
Error: Dev container config (/tmp/nonexistent.json) not found.
```

## 2. Invalid JSON Syntax

Create a file with invalid JSON:
```bash
echo "{ invalid json" > /tmp/invalid.json
deacon read-configuration --workspace-folder /tmp --config /tmp/invalid.json
```

**Error:**
```
Error: expected `,` or `}` at line 1 column 11
```

## 3. Non-Object Root

Create a file with array root:
```bash
echo "[]" > /tmp/array.json
deacon read-configuration --workspace-folder /tmp --config /tmp/array.json
```

**Error:**
```
Error: Invalid devcontainer.json: must contain a JSON object literal.
```

## 4. Missing Required Selector

```bash
deacon read-configuration --config devcontainer.json
```

**Error:**
```
Error: Missing required argument: One of --container-id, --id-label or --workspace-folder is required.
```

## 5. Invalid id-label Format

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json --id-label invalid
```

**Error:**
```
Error: Unmatched argument format: id-label must match <name>=<value>.
```

## 6. Terminal Dimensions Mismatch

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json --terminal-columns 80
```

**Error:**
```
Error: --terminal-columns and --terminal-rows must both be provided or both be omitted.
```

## 7. Invalid Additional Features JSON

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --additional-features "not json"
```

**Error:**
```
Error: Failed to parse --additional-features JSON
```

## 8. Additional Features Not an Object

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --additional-features '["array", "not", "object"]'
```

**Error:**
```
Error: --additional-features must be a JSON object.
```

## 9. Container Not Found

```bash
deacon read-configuration --container-id nonexistent-container-id
```

**Error:**
```
Error: Dev container not found. Container ID or labels did not match any running containers.
```

## 10. Docker Not Available

When Docker is not running or not accessible:

```bash
deacon read-configuration --container-id abc123 --workspace-folder .
```

**Error:**
```
Error: Failed to execute docker command: ... (Docker daemon not running or not accessible)
```

## Validation Rules Summary

### Required Arguments
- At least one of: `--container-id`, `--id-label`, or `--workspace-folder`

### Format Validation
- `--id-label`: Must match `<name>=<value>` with non-empty name and value
- `--additional-features`: Must be valid JSON object
- Config files: Must be valid JSON with object root

### Paired Arguments
- `--terminal-columns` and `--terminal-rows`: Both or neither
- `--include-merged-configuration` without container: Auto-includes features

### File Validation
- Config file must exist and be readable
- Override config must exist if specified
- Secrets file must exist if specified
