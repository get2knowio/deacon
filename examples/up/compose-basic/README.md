# Compose Basic Example

## Overview

This example demonstrates using Docker Compose with the `up` subcommand to create a multi-service development environment.

## Configuration Features

- **Compose File**: Uses `docker-compose.yml` with two services
- **Services**:
  - `app`: Node.js 20 development container (main dev container)
  - `db`: PostgreSQL 15 database (supporting service)
- **Service Selection**: Targets the `app` service for development
- **Volumes**: Shared workspace and persistent database storage
- **Environment**: Development-specific environment variables

## Usage

### Basic Compose Up

```bash
deacon up --workspace-folder .
```

This will:
1. Read the compose configuration
2. Start both `app` and `db` services
3. Connect to the `app` service as the dev container
4. Run the `postCreateCommand`
5. Return container details

### Expected Output

```json
{
  "outcome": "success",
  "containerId": "<app-container-id>",
  "composeProjectName": "<project-name>",
  "remoteUser": "node",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Testing

Verify the multi-service setup:

```bash
# Check Node.js in app container
docker exec <app-container-id> node --version

# Verify database is running
docker ps --filter name=db

# Test database connectivity from app container
docker exec <app-container-id> sh -c \
  "apk add --no-cache postgresql-client && \
   psql -h db -U devuser -d devdb -c 'SELECT version();'"
```

## Project Name

The compose project name is derived from:
1. `COMPOSE_PROJECT_NAME` environment variable
2. Directory name (default)

You can override it:

```bash
export COMPOSE_PROJECT_NAME=myproject
deacon up --workspace-folder .
```

## Additional Mounts

Add extra mounts to the app service:

```bash
deacon up --workspace-folder . \
  --mount "type=volume,source=node-modules,target=/workspace/node_modules"
```

## Cleanup

```bash
# Remove containers
docker compose down

# Remove containers and volumes
docker compose down -v
```

## Related Examples

- `compose-profiles/` - Using compose profiles
- `basic-image/` - Single container without compose
- `additional-mounts/` - Advanced mount configurations
