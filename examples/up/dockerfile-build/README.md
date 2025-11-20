# Dockerfile Build Example

## Overview

This example demonstrates building a custom container image from a Dockerfile during the `up` command execution.

## Configuration Features

- **Build**: Builds from a custom Dockerfile
- **Base Image**: Node.js 20 on Alpine Linux
- **Non-root User**: Creates and uses `devuser` (UID 1000)
- **Tools**: Includes git, SSH client, and bash
- **Lifecycle Verification**: Runs version checks after creation

## Usage

### Basic Build and Up

```bash
deacon up --workspace-folder .
```

This will:
1. Build the Docker image from the Dockerfile
2. Create a container from the built image
3. Run the `postCreateCommand` to verify Node.js and npm
4. Return container details in JSON format

### Build with No Cache

To force a clean build without using Docker's build cache:

```bash
deacon up --workspace-folder . --build-no-cache
```

### With BuildKit

BuildKit provides advanced build features and caching:

```bash
deacon up --workspace-folder . --buildkit auto
```

### With Build Cache Options

Use external cache sources for faster builds:

```bash
deacon up --workspace-folder . \
  --cache-from type=registry,ref=myregistry/myapp:cache \
  --cache-to type=registry,ref=myregistry/myapp:cache
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

## Testing

Verify the build and setup:
- Check Node.js version: `docker exec <container-id> node --version`
- Verify user: `docker exec <container-id> whoami` (should output "devuser")
- Check git availability: `docker exec <container-id> git --version`

## Cleanup

```bash
docker rm -f <container-id>
docker rmi <image-id>
```

## Related Examples

- `basic-image/` - Use a pre-built image instead of building
- `with-features/` - Add Features on top of a Dockerfile build
- `lifecycle-hooks/` - More complex build and setup workflows
