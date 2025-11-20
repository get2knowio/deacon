# Prebuild Mode Example

## Overview

This example demonstrates the `--prebuild` flag, which is used to create pre-built development container images for faster startup times in CI/CD pipelines and team environments.

## What is Prebuild Mode?

Prebuild mode stops the lifecycle execution after `onCreate` and `updateContent` commands, creating a container image that can be reused. This is ideal for:
- CI/CD pipelines that build base images
- Team environments with shared base images
- Reducing startup time for developers

## Lifecycle Behavior with --prebuild

### First Run (Prebuild)
```
1. onCreateCommand    ✓ Runs
2. updateContentCommand ✓ Runs
3. postCreateCommand  ✗ Skipped
4. postStartCommand   ✗ Skipped
5. postAttachCommand  ✗ Skipped
```

### Subsequent Runs (Prebuild Again)
```
1. onCreateCommand    ✗ Skipped (already ran)
2. updateContentCommand ✓ Runs again
3. postCreateCommand  ✗ Skipped
4. postStartCommand   ✗ Skipped
5. postAttachCommand  ✗ Skipped
```

### Normal Run After Prebuild
```
1. onCreateCommand    ✗ Already completed
2. updateContentCommand ✓ Runs
3. postCreateCommand  ✓ Runs
4. postStartCommand   ✓ Runs
5. postAttachCommand  ✓ Runs
```

## Usage

### Step 1: Create Prebuild Image

```bash
deacon up --workspace-folder . --prebuild
```

This will:
1. Pull the base image and apply Features
2. Run `onCreateCommand` (install build-essential)
3. Run `updateContentCommand` (clean npm cache)
4. Stop (skip postCreate, postStart, postAttach)
5. Return container details

### Step 2: Commit Prebuild Image

```bash
# Get container ID from previous command
docker commit <container-id> myproject:prebuild

# Tag for registry
docker tag myproject:prebuild registry.example.com/myproject:prebuild

# Push to registry
docker push registry.example.com/myproject:prebuild
```

### Step 3: Use Prebuild Image

Update devcontainer.json to use the prebuild image:

```json
{
  "image": "registry.example.com/myproject:prebuild",
  "remoteUser": "vscode",
  "workspaceFolder": "/workspace",
  "postCreateCommand": "npm install",
  "postStartCommand": "echo 'Ready to develop!'"
}
```

Then run normally:

```bash
deacon up --workspace-folder .
```

This skips the time-consuming onCreate and Feature installation.

### Rerun Prebuild

Running prebuild again on the same container:

```bash
deacon up --workspace-folder . --prebuild
```

Will:
- Skip onCreate (already completed, marker present)
- Rerun updateContentCommand (to refresh content)
- Skip other lifecycle hooks

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "vscode",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Testing Prebuild

Verify prebuild execution:

```bash
# Check that onCreate ran (build-essential installed)
docker exec <container-id> gcc --version

# Check that updateContent ran
docker exec <container-id> cat /tmp/*.marker  # Look for update markers

# Verify postCreate did NOT run
docker exec <container-id> test -d /workspace/node_modules && echo "exists" || echo "skipped"
# Should output: "skipped" on first prebuild
```

## CI/CD Pipeline Example

```yaml
# .github/workflows/prebuild.yml
name: Prebuild Dev Container

on:
  push:
    branches: [main]
    paths:
      - '.devcontainer/**'

jobs:
  prebuild:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Build prebuild image
        run: |
          deacon up --workspace-folder . --prebuild
          CONTAINER_ID=$(docker ps -lq)
          docker commit $CONTAINER_ID myproject:prebuild
          
      - name: Push to registry
        run: |
          docker tag myproject:prebuild ghcr.io/${{ github.repository }}:prebuild
          docker push ghcr.io/${{ github.repository }}:prebuild
```

## Benefits

- **Faster Startup**: Skip time-consuming installations
- **Consistency**: All team members use the same base image
- **CI Optimization**: Build once, use many times
- **Bandwidth Savings**: Reduce repeated downloads of dependencies

## Cleanup

```bash
docker rm -f <container-id>
docker rmi myproject:prebuild
```

## Related Examples

- `lifecycle-hooks/` - Full lifecycle execution
- `with-features/` - Features without prebuild
- `basic-image/` - Simple image-based container
