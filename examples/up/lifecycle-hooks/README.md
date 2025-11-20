# Lifecycle Hooks Example

## Overview

This example demonstrates all available lifecycle hooks in the Dev Container specification and their execution order.

## Lifecycle Hook Types

### 1. onCreateCommand
**When**: Runs once when the container is first created
**Format**: Object with named commands (runs in parallel)
**Use Case**: Install system dependencies, set up base configuration

```json
"onCreateCommand": {
  "install-deps": "apt-get update && apt-get install -y curl git",
  "create-marker": "echo 'onCreate completed' > /tmp/oncreate.marker"
}
```

### 2. updateContentCommand
**When**: Runs after onCreate and on every subsequent container start
**Format**: Array of commands (runs sequentially)
**Use Case**: Update cached content, refresh dependencies

```json
"updateContentCommand": [
  "echo 'Updating content...'",
  "mkdir -p /tmp/updates",
  "date > /tmp/updates/last-update.txt"
]
```

### 3. postCreateCommand
**When**: Runs once after onCreate and updateContent
**Format**: String command
**Use Case**: Set up workspace structure, initialize projects

```json
"postCreateCommand": "mkdir -p /workspace/{src,tests,docs}"
```

### 4. postStartCommand
**When**: Runs every time the container starts
**Format**: Object with named commands (runs in parallel)
**Use Case**: Start background services, check environment

```json
"postStartCommand": {
  "log-start": "echo \"Started at $(date)\" >> /tmp/start.log",
  "check-git": "git --version"
}
```

### 5. postAttachCommand
**When**: Runs every time a tool attaches to the container
**Format**: Array of commands (runs sequentially)
**Use Case**: Display welcome messages, show environment info

```json
"postAttachCommand": [
  "echo 'Welcome!'",
  "echo 'User: $(whoami)'"
]
```

## Execution Order

```
Container Creation (first time):
  1. onCreateCommand    (parallel execution)
  2. updateContentCommand  (sequential execution)
  3. postCreateCommand
  4. postStartCommand   (parallel execution)
  5. postAttachCommand  (sequential execution)

Container Restart (subsequent starts):
  1. updateContentCommand
  2. postStartCommand
  3. postAttachCommand
```

## Usage

### Normal Up (All Hooks)

```bash
deacon up --workspace-folder .
```

All lifecycle hooks execute in order.

### Skip Post-Create Hooks

```bash
deacon up --workspace-folder . --skip-post-create
```

Skips: onCreate, updateContent, postCreate, postStart, postAttach

### Skip Post-Attach Only

```bash
deacon up --workspace-folder . --skip-post-attach
```

Skips only the postAttachCommand.

### Skip Non-Blocking Commands

```bash
deacon up --workspace-folder . --skip-non-blocking-commands
```

Skips commands marked as non-blocking (background tasks).

## Testing Lifecycle Execution

After running `deacon up --workspace-folder .`, verify hook execution:

```bash
# Check onCreate marker
docker exec <container-id> cat /tmp/oncreate.marker

# Check updateContent execution
docker exec <container-id> cat /tmp/updates/last-update.txt

# Verify directory structure from postCreateCommand
docker exec <container-id> ls -la /workspace/

# Check postStart log
docker exec <container-id> cat /tmp/start.log

# Verify installed packages from onCreateCommand
docker exec <container-id> git --version
docker exec <container-id> curl --version
```

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "devuser",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Variable Substitution

The example uses `${localWorkspaceFolderBasename}` in `remoteEnv`, which gets substituted with the workspace folder name.

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `prebuild-mode/` - Prebuild workflow with different lifecycle execution
- `skip-lifecycle/` - Skipping lifecycle hooks
- `basic-image/` - Simple setup without lifecycle hooks
