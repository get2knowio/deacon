# Skip Lifecycle Example

## Overview

This example demonstrates the `--skip-post-create` and `--skip-post-attach` flags that control which lifecycle commands are executed during `up`.

## Lifecycle Skipping Options

### 1. --skip-post-create
Skips: onCreate, updateContent, postCreate, postStart, postAttach, and dotfiles

### 2. --skip-post-attach
Skips: Only postAttachCommand

### 3. --skip-non-blocking-commands
Skips: Background/non-blocking lifecycle commands

## Usage

### Normal Up (All Lifecycle Commands)

```bash
deacon up --workspace-folder .
```

Executes all lifecycle commands:
1. onCreateCommand
2. updateContentCommand
3. postCreateCommand
4. postStartCommand
5. postAttachCommand

### Skip Post-Create Lifecycle

```bash
deacon up --workspace-folder . --skip-post-create
```

Skips ALL lifecycle commands and dotfiles installation.
- No onCreate
- No updateContent
- No postCreate
- No postStart
- No postAttach
- No dotfiles

Use when:
- You want a minimal container without setup
- Testing container creation only
- Debugging configuration without lifecycle overhead

### Skip Only Post-Attach

```bash
deacon up --workspace-folder . --skip-post-attach
```

Executes onCreate, updateContent, postCreate, postStart
Skips: postAttachCommand only

Use when:
- You don't need attach-specific welcome messages
- Automating workflows where attach doesn't occur

### Skip Non-Blocking Commands

```bash
deacon up --workspace-folder . --skip-non-blocking-commands
```

Skips background tasks that don't block completion.

## Testing Lifecycle Execution

### Test 1: Normal Execution

```bash
deacon up --workspace-folder .
CONTAINER_ID=$(docker ps -lq)

# Verify onCreate ran
docker exec $CONTAINER_ID apt list --installed 2>/dev/null | grep -q apt && echo "✓ onCreate ran"

# Verify updateContent ran
docker exec $CONTAINER_ID test -f /tmp/update.txt && echo "✓ updateContent ran"

# Verify postCreate ran
docker exec $CONTAINER_ID test -d /workspace/setup && echo "✓ postCreate ran"

# Verify postStart ran
docker exec $CONTAINER_ID test -f /tmp/start.log && echo "✓ postStart ran"
```

### Test 2: Skip Post-Create

```bash
deacon up --workspace-folder . --skip-post-create
CONTAINER_ID=$(docker ps -lq)

# Verify onCreate did NOT run
docker exec $CONTAINER_ID apt list --installed 2>/dev/null | grep -q apt || echo "✓ onCreate skipped"

# Verify updateContent did NOT run
docker exec $CONTAINER_ID test -f /tmp/update.txt || echo "✓ updateContent skipped"

# Verify postCreate did NOT run
docker exec $CONTAINER_ID test -d /workspace/setup || echo "✓ postCreate skipped"

# Verify postStart did NOT run
docker exec $CONTAINER_ID test -f /tmp/start.log || echo "✓ postStart skipped"
```

## Expected Output

Both commands produce the same JSON structure:

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

The difference is in the container's internal state.

## Use Cases

### 1. Fast Container Startup

Skip setup for quick container access:

```bash
deacon up --workspace-folder . --skip-post-create
```

### 2. Debugging Configuration

Test configuration without lifecycle interference:

```bash
deacon up --workspace-folder . --skip-post-create --include-merged-configuration
```

### 3. Manual Setup

Start container and run setup manually:

```bash
deacon up --workspace-folder . --skip-post-create
docker exec -it <container-id> /bin/bash
# Manually run setup commands
```

### 4. CI/CD Optimization

Skip unnecessary commands in CI:

```bash
# Build pipeline - skip attach
deacon up --workspace-folder . --skip-post-attach
```

### 5. Prebuild Images

Use with prebuild for different execution strategies:

```bash
# Prebuild stops after updateContent
deacon up --workspace-folder . --prebuild

# Later: skip all lifecycle (already done in prebuild)
deacon up --workspace-folder . --skip-post-create
```

## Lifecycle Hook Timing

### Without Flags (Normal)
```
Container Start
  ├─ onCreateCommand        ← First creation only
  ├─ updateContentCommand   ← Every start
  ├─ Dotfiles              ← First creation only
  ├─ postCreateCommand     ← First creation only
  ├─ postStartCommand      ← Every start
  └─ postAttachCommand     ← Every attach
```

### With --skip-post-create
```
Container Start
  (All lifecycle commands skipped)
```

### With --skip-post-attach
```
Container Start
  ├─ onCreateCommand
  ├─ updateContentCommand
  ├─ Dotfiles
  ├─ postCreateCommand
  ├─ postStartCommand
  └─ postAttachCommand     ← Skipped
```

## Combining Flags

### Skip Lifecycle with Include Configuration

```bash
deacon up --workspace-folder . \
  --skip-post-create \
  --include-configuration
```

Fast container creation with configuration output.

### Skip Lifecycle with Remove Existing

```bash
deacon up --workspace-folder . \
  --skip-post-create \
  --remove-existing-container
```

Force clean container without setup.

### Skip with Custom Mounts

```bash
deacon up --workspace-folder . \
  --skip-post-create \
  --mount "type=bind,source=$HOME/.config,target=/root/.config"
```

Minimal container with custom configuration mounted.

## Performance Impact

| Command | Time (Example) | Lifecycle Executed |
|---------|---------------|-------------------|
| Normal up | ~60s | All |
| --skip-post-attach | ~55s | onCreate through postStart |
| --skip-post-create | ~10s | None |

Actual times depend on lifecycle command complexity.

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `lifecycle-hooks/` - Full lifecycle execution
- `prebuild-mode/` - Prebuild lifecycle workflow
- `basic-image/` - Simple container with minimal lifecycle
