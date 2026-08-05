# Dev Container with Features Example

## Overview

This example demonstrates using Dev Container Features to extend a base image with additional development tools and configurations.

## Configuration Features

- **Base Image**: Ubuntu 22.04
- **Features**:
  - `common-utils`: Installs Zsh, Oh My Zsh, and creates a non-root user
  - `node`: Installs Node.js version 20
- **Remote User**: Uses `vscode` user created by the `common-utils` feature
- **Workspace**: Mounted at `/workspace`

## Usage

### Basic Up with Features

```bash
deacon up --workspace-folder .
```

The `up` command will:
1. Pull the Ubuntu 22.04 base image
2. Apply the Features (extends the image with a new layer)
3. Create a container with the configured user and workspace
4. Run the `postCreateCommand` to verify installations (unless you pass `--skip-post-create` like `./exec.sh` does for speed)
5. Return container details

### With Additional Features

Add more features at runtime using the `--additional-features` flag:

```bash
deacon up --workspace-folder . \
  --additional-features '{
    "ghcr.io/devcontainers/features/docker-in-docker:2": {
      "version": "latest"
    }
  }'
```

### Ignore Command-Line Features

To resolve only the Features declared in `devcontainer.json`, dropping anything passed
via `--additional-features`:

```bash
deacon up --workspace-folder . --ignore-additional-features
```

> `--skip-feature-auto-mapping` is still accepted for command-line compatibility with the
> reference CLI, but it has no effect on either implementation — deprecated Feature id
> auto-mapping is always applied. It used to reach the behavior above; use
> `--ignore-additional-features` instead.

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "vscode",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Testing

Verify the features are installed:

```bash
# Check Node.js
docker exec <container-id> node --version

# Check Zsh
docker exec <container-id> zsh --version

# Verify user
docker exec <container-id> whoami  # Should output: vscode
```

## Feature Details

### common-utils Feature
- Creates a non-root user with specified UID/GID
- Installs Zsh and Oh My Zsh for an enhanced shell experience
- Sets up common development utilities

### node Feature
- Installs Node.js and npm
- Configures Node.js environment

## Cleanup

```bash
docker rm -f <container-id>
docker rmi <extended-image-id>
```

## Related Examples

- `basic-image/` - Simple image without features
- `lifecycle-hooks/` - Using lifecycle commands with features
- `prebuild-mode/` - Prebuild workflow with features
