# Additional Mounts Example

## Overview

This example demonstrates using the `--mount` flag to add custom bind mounts and volumes beyond the default workspace mount.

## Configuration Features

- **Default Mounts**: Workspace folder (automatic)
- **Custom Bind Mount**: Local `config/` directory to `/etc/myapp`
- **Custom Volume**: Named volume `myapp-cache` to `/var/cache/myapp`

## Mount Types

### Bind Mounts
Bind mounts link a host directory to a container path:
- Changes are immediately visible in both directions
- Use for configuration files, source code, or shared data
- Syntax: `type=bind,source=<host-path>,target=<container-path>`

### Volume Mounts
Volumes are managed by Docker and persist independently:
- Survive container removal
- Better performance than bind mounts
- Use for caches, databases, or generated data
- Syntax: `type=volume,source=<volume-name>,target=<container-path>`

## Usage

### Basic (Config File Mounts)

```bash
deacon up --workspace-folder .
```

Uses mounts defined in `devcontainer.json`.

### Add Runtime Mounts

Add additional mounts at runtime:

```bash
deacon up --workspace-folder . \
  --mount "type=bind,source=$HOME/.ssh,target=/home/vscode/.ssh,readonly" \
  --mount "type=volume,source=npm-cache,target=/home/vscode/.npm"
```

### Multiple Mount Flags

Each `--mount` flag adds one mount:

```bash
deacon up --workspace-folder . \
  --mount "type=bind,source=/tmp/logs,target=/var/log/myapp" \
  --mount "type=volume,source=postgres-data,target=/var/lib/postgresql/data" \
  --mount "type=bind,source=$HOME/.gitconfig,target=/etc/gitconfig,readonly"
```

### External Volumes

Mark volumes as external (pre-existing):

```bash
# Create volume first
docker volume create shared-data

# Use with external flag
deacon up --workspace-folder . \
  --mount "type=volume,source=shared-data,target=/data,external=true"
```

## Mount Format

The `--mount` flag requires this format:

```
type=<bind|volume>,source=<source>,target=<target>[,external=<true|false>][,readonly]
```

Required fields:
- `type`: Either `bind` or `volume`
- `source`: Host path (bind) or volume name (volume)
- `target`: Container path

Optional fields:
- `external`: For volumes, indicates pre-existing volume (default: false)
- `readonly`: Mount as read-only

## Workspace Mount Consistency

Control how workspace mounts are synchronized:

```bash
# Cached (default) - host authoritative, better performance
deacon up --workspace-folder . --workspace-mount-consistency cached

# Consistent - fully synchronized
deacon up --workspace-folder . --workspace-mount-consistency consistent

# Delegated - container authoritative
deacon up --workspace-folder . --workspace-mount-consistency delegated
```

## Compose Integration

With Compose, additional mounts are converted to compose volumes:

```bash
deacon up --workspace-folder . \
  --mount "type=volume,source=node-modules,target=/workspace/node_modules"
```

## Testing Mounts

Verify mounts are correctly applied:

```bash
# Check bind mount
docker exec <container-id> cat /etc/myapp/app.conf

# Check volume mount
docker exec <container-id> df -h /var/cache/myapp

# List all mounts
docker inspect <container-id> --format '{{json .Mounts}}' | jq
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

## Common Use Cases

### SSH Keys
```bash
--mount "type=bind,source=$HOME/.ssh,target=/root/.ssh,readonly"
```

### Git Configuration
```bash
--mount "type=bind,source=$HOME/.gitconfig,target=/etc/gitconfig,readonly"
```

### Package Manager Cache
```bash
--mount "type=volume,source=npm-cache,target=/root/.npm"
--mount "type=volume,source=pip-cache,target=/root/.cache/pip"
```

### Shared Data Between Containers
```bash
--mount "type=volume,source=shared-data,target=/data,external=true"
```

## Cleanup

```bash
# Remove container
docker rm -f <container-id>

# Remove named volumes
docker volume rm myapp-cache npm-cache pip-cache shared-data
```

## Related Examples

- `basic-image/` - Simple setup without custom mounts
- `compose-basic/` - Mounts in Compose configuration
- `remote-env-secrets/` - Environment and secrets management
