# Custom Container Name Example

This example demonstrates the `--container-name` flag for the `deacon up` command.

## Overview

By default, `deacon` generates deterministic container names based on workspace and configuration hashes (e.g., `deacon-a1b2c3d4`). The `--container-name` flag allows you to specify a custom name for the container, which is useful for:

- **Scripting**: Easier to reference containers by predictable names
- **Multi-workspace setups**: Avoid name collisions when working with multiple workspaces
- **Debugging**: More meaningful container names in `docker ps` output

## Usage

### Default behavior (generated name)
```bash
deacon up
# Creates container with generated name like: deacon-a1b2c3d4
```

### With custom name
```bash
deacon up --container-name my-project-dev
# Creates container with name: my-project-dev
```

## Example Commands

```bash
# Start a development container with custom name
deacon up --container-name my-app-backend

# Start with custom name and other flags
deacon up --container-name my-service --skip-post-create

# Use custom name for specific workspace
deacon up --workspace-folder /path/to/project --container-name project-main
```

## Important Notes

- The custom name must be valid for your container runtime (Docker/Podman)
- If a container with the specified name already exists:
  - Without `--remove-existing-container`: The existing container will be reused
  - With `--remove-existing-container`: The existing container will be removed and recreated
- Custom names override the deterministic name generation

## Error Handling

If you specify an invalid container name (e.g., containing invalid characters), the container runtime will reject it with a validation error.

## See Also

- `deacon up --help` for all available flags
- Container lifecycle management documentation in `docs/subcommand-specs/*/SPEC.md`
