# Remove Existing Container Example

## Overview

This example demonstrates the `--remove-existing-container` flag, which forcibly removes an existing container before creating a new one.

## Purpose

Use `--remove-existing-container` to:
- Force a clean container rebuild
- Reset container state
- Apply configuration changes that require recreation
- Clear accumulated data or state

## Usage

### Normal Reconnection (Default Behavior)

By default, `up` reconnects to existing containers:

```bash
# First run - creates container
deacon up --workspace-folder .

# Second run - reconnects to same container
deacon up --workspace-folder .
```

The `onCreateCommand` only runs once (on first creation).

### Force Container Replacement

Remove and recreate the container:

```bash
deacon up --workspace-folder . --remove-existing-container
```

This will:
1. Find the existing container (by workspace/ID labels)
2. Stop and remove it
3. Create a new container
4. Run all lifecycle commands (including onCreate)

### Verify Replacement

Check creation timestamp to confirm replacement:

```bash
# First run
deacon up --workspace-folder .
CONTAINER_ID_1=$(docker ps -lq)
docker exec $CONTAINER_ID_1 cat /tmp/created.txt
# Output: Container created at: <timestamp-1>

# Second run without flag (reconnect)
deacon up --workspace-folder .
CONTAINER_ID_2=$(docker ps -lq)
docker exec $CONTAINER_ID_2 cat /tmp/created.txt
# Output: Container created at: <timestamp-1> (same)

# Third run with --remove-existing-container
deacon up --workspace-folder . --remove-existing-container
CONTAINER_ID_3=$(docker ps -lq)
docker exec $CONTAINER_ID_3 cat /tmp/created.txt
# Output: Container created at: <timestamp-3> (new)
```

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<new-container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

The `containerId` will be different from the previous container.

## Use Cases

### 1. Configuration Changes

Some changes require container recreation:

```bash
# Update devcontainer.json with new base image
# Then force recreation:
deacon up --workspace-folder . --remove-existing-container
```

### 2. Clean Development Environment

Reset to pristine state:

```bash
deacon up --workspace-folder . --remove-existing-container
```

Clears:
- Installed packages
- Generated files
- Cached data
- Runtime state

### 3. Testing Container Setup

Test that setup scripts work correctly from scratch:

```bash
deacon up --workspace-folder . --remove-existing-container
```

### 4. Apply New Features

When adding features to devcontainer.json:

```bash
# Edit devcontainer.json to add features
deacon up --workspace-folder . --remove-existing-container
```

### 5. CI/CD Clean Builds

Ensure clean state in CI pipelines:

```bash
# .github/workflows/ci.yml
- name: Start Dev Container
  run: deacon up --workspace-folder . --remove-existing-container
```

## Comparison: Remove vs Expect Existing

### --remove-existing-container
- Removes existing container if found
- Creates new container
- Always succeeds (unless other errors)

### --expect-existing-container
- Expects container to exist
- Fails if not found
- Never creates new container
- Reconnects to existing

These flags are mutually exclusive.

## Data Preservation

### Lost on Removal
- Container state and filesystem
- Runtime data
- Installed packages not in Dockerfile

### Preserved
- Named volumes (unless explicitly removed)
- Bind-mounted directories
- Host filesystem

### Preserve Data with Volumes

```bash
# Use volumes for persistent data
deacon up --workspace-folder . \
  --mount "type=volume,source=project-data,target=/data" \
  --remove-existing-container

# Data in /data persists across removals
```

## Combining with Other Flags

### Remove and Rebuild with No Cache

```bash
deacon up --workspace-folder . \
  --remove-existing-container \
  --build-no-cache
```

Forces both container and image rebuild.

### Remove and Include Configuration

```bash
deacon up --workspace-folder . \
  --remove-existing-container \
  --include-merged-configuration
```

Returns configuration of the newly created container.

### Remove with Custom ID Labels

```bash
deacon up --workspace-folder . \
  --id-label "env=dev" \
  --remove-existing-container
```

Removes the specific labeled container and creates a new one with the same labels.

## Testing

Script to verify removal and recreation:

```bash
#!/bin/bash

# Create initial container
echo "Creating container..."
OUTPUT1=$(deacon up --workspace-folder .)
ID1=$(echo "$OUTPUT1" | jq -r '.containerId')
TIMESTAMP1=$(docker exec $ID1 cat /tmp/created.txt)

echo "Container 1: $ID1"
echo "$TIMESTAMP1"

# Wait a moment
sleep 2

# Remove and recreate
echo "Removing and recreating..."
OUTPUT2=$(deacon up --workspace-folder . --remove-existing-container)
ID2=$(echo "$OUTPUT2" | jq -r '.containerId')
TIMESTAMP2=$(docker exec $ID2 cat /tmp/created.txt)

echo "Container 2: $ID2"
echo "$TIMESTAMP2"

# Verify different
if [ "$ID1" != "$ID2" ]; then
  echo "✓ Container was replaced"
else
  echo "✗ Container was not replaced"
fi

if [ "$TIMESTAMP1" != "$TIMESTAMP2" ]; then
  echo "✓ Timestamps differ (onCreate ran again)"
else
  echo "✗ Timestamps match (onCreate did not run)"
fi
```

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `id-labels-reconnect/` - Reconnecting to existing containers
- `prebuild-mode/` - Prebuild workflow with container reuse
- `basic-image/` - Simple container creation
