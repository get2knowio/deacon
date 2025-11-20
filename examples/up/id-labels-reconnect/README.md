# ID Labels Reconnect Example

## Overview

This example demonstrates using `--id-label` flags to tag containers with custom identifiers, enabling reconnection to existing containers across multiple `up` invocations.

## What are ID Labels?

ID labels are Docker labels attached to containers that uniquely identify them. They allow the `up` command to:
- Find and reconnect to existing containers
- Avoid creating duplicate containers
- Support workflows where containers persist across sessions

## Usage

### Create Container with ID Labels

```bash
deacon up --workspace-folder . \
  --id-label "project=myapp" \
  --id-label "environment=dev" \
  --id-label "team=backend"
```

This creates a container with three custom labels.

### Reconnect to Existing Container

Run the same command again:

```bash
deacon up --workspace-folder . \
  --id-label "project=myapp" \
  --id-label "environment=dev" \
  --id-label "team=backend"
```

Instead of creating a new container, it will:
1. Find the existing container with matching labels
2. Start it if stopped
3. Run lifecycle commands as appropriate
4. Return the existing container details

### Expect Existing Container

Fail if no matching container exists:

```bash
deacon up --workspace-folder . \
  --id-label "project=myapp" \
  --id-label "environment=dev" \
  --expect-existing-container
```

This will:
- Look for a container with the specified labels
- Return success if found
- Exit with error if not found (without creating a new container)

### Reconnect by ID Labels Only (No Workspace)

You can reconnect without specifying workspace folder:

```bash
deacon up \
  --id-label "project=myapp" \
  --id-label "environment=dev"
```

Note: At least one of `--workspace-folder` or `--id-label` is required.

## ID Label Format

Each `--id-label` flag must follow `name=value` format:

```bash
--id-label "key=value"
--id-label "namespace.key=value"
--id-label "org.example.project=myapp"
```

Multiple labels create a unique identifier set - all labels must match for reconnection.

## Automatic ID Labels

When not provided, `up` automatically generates ID labels based on:
- Workspace folder path
- Configuration file location
- Other identifiable properties

## Use Cases

### 1. Multi-Environment Development

Different labels for different environments:

```bash
# Development container
deacon up --workspace-folder . \
  --id-label "env=development" \
  --id-label "user=$USER"

# Testing container
deacon up --workspace-folder . \
  --id-label "env=testing" \
  --id-label "user=$USER"

# Staging container
deacon up --workspace-folder . \
  --id-label "env=staging" \
  --id-label "user=$USER"
```

### 2. Team Collaboration

Identify containers by team or project:

```bash
deacon up --workspace-folder . \
  --id-label "team=frontend" \
  --id-label "project=dashboard" \
  --id-label "developer=$USER"
```

### 3. CI/CD Pipelines

Use build identifiers:

```bash
deacon up --workspace-folder . \
  --id-label "ci=true" \
  --id-label "pipeline.id=$CI_PIPELINE_ID" \
  --id-label "job.id=$CI_JOB_ID"
```

### 4. Container Lifecycle Management

Track container purpose:

```bash
deacon up --workspace-folder . \
  --id-label "purpose=testing" \
  --id-label "temporary=true" \
  --id-label "expires=$(date -d '+1 day' -Iseconds)"
```

## Inspecting ID Labels

Check labels on a container:

```bash
# List all labels
docker inspect <container-id> --format '{{json .Config.Labels}}' | jq

# Check specific label
docker inspect <container-id> --format '{{index .Config.Labels "project"}}'

# Find containers by label
docker ps --filter "label=project=myapp"
docker ps --filter "label=environment=dev" --filter "label=project=myapp"
```

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

On reconnection, the `containerId` will be the same as the previous invocation.

## Container Discovery

The `up` command finds containers by matching ALL provided ID labels:

```bash
# These must ALL match
--id-label "a=1" --id-label "b=2" --id-label "c=3"

# A container with labels a=1, b=2, c=3, d=4 WILL match
# A container with labels a=1, b=2 only will NOT match
```

## Combining with Other Flags

### Remove and Recreate with Same Labels

```bash
deacon up --workspace-folder . \
  --id-label "project=myapp" \
  --remove-existing-container
```

Finds the labeled container, removes it, creates a new one with the same labels.

### Expect Existing with Include Configuration

```bash
deacon up \
  --id-label "project=myapp" \
  --expect-existing-container \
  --include-merged-configuration
```

Returns the existing container's configuration without modifications.

## Testing

Create and reconnect workflow:

```bash
# Step 1: Create
OUTPUT1=$(deacon up --workspace-folder . --id-label "test=reconnect")
CONTAINER_ID_1=$(echo "$OUTPUT1" | jq -r '.containerId')

# Step 2: Reconnect
OUTPUT2=$(deacon up --workspace-folder . --id-label "test=reconnect")
CONTAINER_ID_2=$(echo "$OUTPUT2" | jq -r '.containerId')

# Verify same container
if [ "$CONTAINER_ID_1" == "$CONTAINER_ID_2" ]; then
  echo "✓ Successfully reconnected to same container"
else
  echo "✗ Created new container instead of reconnecting"
fi
```

## Cleanup

```bash
# Remove by ID
docker rm -f <container-id>

# Remove by labels
docker rm -f $(docker ps -aq --filter "label=project=myapp")
```

## Related Examples

- `basic-image/` - Simple container without custom labels
- `remove-existing/` - Removing and recreating containers
- `configuration-output/` - Inspecting container configuration
