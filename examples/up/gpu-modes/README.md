# GPU Modes Up Example

## Overview

This example demonstrates the GPU mode handling capabilities of the `up` subcommand. GPU modes control how GPU resources are requested or skipped for containers, builds, and compose operations.

## GPU Mode Options

The `--gpu-mode` flag supports three values:

- **all**: Request GPU resources for all containers and builds (requires GPU-capable host)
- **detect**: Auto-detect host GPU availability; request GPUs if present, warn and continue without GPUs if not
- **none**: (default) Explicitly skip all GPU requests and GPU-related warnings

## Configuration Features

- **Image**: Uses a pinned Alpine Linux base image (`alpine:3.18`)
- **Remote User**: Configured to run as root
- **Workspace**: Mounts workspace at `/workspace`
- **Lifecycle Hook**: Simple `postCreateCommand` for verification

## Usage Scenarios

### 1. Guarantee GPU Access (Mode: all)

Use when your development workflow requires GPU resources and should fail if GPUs are unavailable.

```bash
deacon up --workspace-folder . --gpu-mode all
```

This command will:
1. Request GPU resources for the container
2. Pass `--gpus all` to the underlying Docker runtime
3. Fail if the host cannot provide GPU access
4. Apply GPU requests consistently to all build steps and services

**When to use**: GPU-accelerated ML/AI development, CUDA workloads, GPU-based rendering

### 2. Auto-Detect with Safe Fallback (Mode: detect)

Use when you want automatic GPU detection with graceful fallback on non-GPU hosts.

```bash
deacon up --workspace-folder . --gpu-mode detect
```

This command will:
1. Check host GPU availability before starting containers
2. If GPUs are present: Request GPU resources (behavior matches "all")
3. If GPUs are absent: Display a warning on stderr and continue without GPU requests
4. Complete successfully in both cases

**Expected warning on non-GPU hosts**:
```
Warning: GPU mode 'detect' was selected but no GPUs were found on the host.
Execution will proceed without GPU acceleration.
```

**When to use**: Cross-platform development, team environments with mixed hardware

### 3. Explicit CPU-Only Runs (Mode: none)

Use when you explicitly want to avoid GPU interactions.

```bash
deacon up --workspace-folder . --gpu-mode none
```

Or omit the flag entirely (none is the default):

```bash
deacon up --workspace-folder .
```

This command will:
1. Skip all GPU requests
2. Produce no GPU-related warnings or notices
3. Run purely on CPU resources

**When to use**: CPU-only workloads, testing without GPU dependencies, resource isolation

## Expected Output Structure

All GPU modes return the same JSON structure on success:

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

GPU mode selection affects container runtime configuration but does not change the JSON output format.

## Testing

### Test GPU Mode: all

```bash
# On GPU-capable host: succeeds with GPU access
deacon up --workspace-folder . --gpu-mode all

# On non-GPU host: runtime error from Docker
# Expected: Container creation fails with clear GPU error
```

Verify GPU access in container:
```bash
docker exec <container-id> nvidia-smi  # Should show GPU info if successful
```

### Test GPU Mode: detect

```bash
# On any host: succeeds with or without GPU
deacon up --workspace-folder . --gpu-mode detect

# On non-GPU host: check for warning on stderr
deacon up --workspace-folder . --gpu-mode detect 2>&1 | grep -i "gpu"
```

### Test GPU Mode: none

```bash
# On any host: succeeds, no GPU requests
deacon up --workspace-folder . --gpu-mode none

# Verify no GPU-related output
deacon up --workspace-folder . --gpu-mode none 2>&1 | grep -i "gpu" || echo "No GPU mentions (expected)"
```

## Multi-Service Compose Consistency

When using Docker Compose configurations, the selected GPU mode applies uniformly to all services:

```bash
# All services receive GPU requests
deacon up --workspace-folder <compose-project> --gpu-mode all

# All services use auto-detection
deacon up --workspace-folder <compose-project> --gpu-mode detect

# All services skip GPU requests
deacon up --workspace-folder <compose-project> --gpu-mode none
```

This ensures predictable, consistent GPU behavior across complex multi-container setups.

## Build-Time GPU Access

GPU modes also apply to build steps (when using Dockerfile-based configs):

```bash
# Build with GPU access (for GPU-accelerated build tools)
deacon up --workspace-folder <dockerfile-project> --gpu-mode all

# Build with auto-detection
deacon up --workspace-folder <dockerfile-project> --gpu-mode detect
```

## Output Streams

Per Deacon's output contract:

- **stdout**: JSON result only (parseable)
- **stderr**: Logs, warnings, diagnostics (including GPU detection warnings)

Example safe parsing:
```bash
RESULT=$(deacon up --workspace-folder . --gpu-mode detect 2>/dev/null)
echo "$RESULT" | jq '.containerId'
```

## Cleanup

```bash
# Get container ID from output
CONTAINER_ID=$(deacon up --workspace-folder . --gpu-mode none | jq -r '.containerId')

# Remove container
docker rm -f "$CONTAINER_ID"
```

## Edge Cases

- **GPU mode "all" on non-GPU host**: Runtime fails with clear Docker error (GPU resources unavailable)
- **GPU mode "detect" without drivers**: Warning displayed, execution continues without GPUs
- **Mixed GPU/non-GPU services**: GPU mode applies consistently to all services (no partial application)
- **Repeated invocations**: Each `up` invocation respects the current `--gpu-mode` flag (no caching of previous settings)

## Related Examples

- `basic-image/` - Simple image-based container (similar base configuration)
- `compose-basic/` - Multi-service setup (GPU modes apply to all services)
- `dockerfile-build/` - Build from Dockerfile (GPU modes apply to build steps)

## Specification Reference

- Feature Spec: `specs/001-gpu-modes/spec.md`
- Up Subcommand Spec: `docs/subcommand-specs/up/SPEC.md`
