# Basic Image Up Example

## Overview

This example demonstrates the simplest `up` subcommand usage with a basic image-based devcontainer configuration.

## Configuration Features

- **Image**: Uses a pinned Alpine Linux base image (`alpine:3.18`)
- **Remote User**: Configured to run as root
- **Workspace**: Mounts workspace at `/workspace`
- **Lifecycle Hook**: Includes a simple `postCreateCommand` to verify setup

## Usage

### Basic Up Command

```bash
deacon up --workspace-folder .
```

This command will:
1. Pull the Alpine 3.18 image if not already available
2. Create a new container with the configured settings
3. Run the `postCreateCommand` lifecycle hook
4. Return JSON output with container details

### Expected Output Structure

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Testing

Run the up command and verify:
- Exit code is 0
- JSON output contains `outcome: "success"`
- Container ID is present in output
- Container is running: `docker ps --filter label=devcontainer.local_folder=...`

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `dockerfile-build/` - Build from a Dockerfile instead of using a pre-built image
- `with-features/` - Add Dev Container Features to extend the base image
- `lifecycle-hooks/` - More complex lifecycle command examples
