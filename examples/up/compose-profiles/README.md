# Compose Profiles Example

## Overview

This example demonstrates using Docker Compose profiles with the `up` subcommand to conditionally start services based on the development context.

## Configuration Features

- **Base Services**: Always started (app, cache)
- **Profiles**:
  - `dev`: Development tools (mailcatcher for email testing)
  - `test`: Testing infrastructure (ephemeral test database)
  - `prod`: Production-like services (nginx reverse proxy)
- **Project Name**: Defined in `.env` file as `myapp`
- **Dependencies**: App depends on cache service

## Usage

### Default (No Profiles)

Start only base services:

```bash
deacon up --workspace-folder .
```

Services started: `app`, `cache`

### Development Profile

Start with development tools:

```bash
COMPOSE_PROFILES=dev deacon up --workspace-folder .
```

Services started: `app`, `cache`, `mailcatcher`

### Multiple Profiles

Combine profiles for comprehensive environment:

```bash
COMPOSE_PROFILES=dev,test deacon up --workspace-folder .
```

Services started: `app`, `cache`, `mailcatcher`, `test-db`

### Testing Profile Only

```bash
COMPOSE_PROFILES=test deacon up --workspace-folder .
```

Services started: `app`, `cache`, `test-db`

## Expected Output

```json
{
  "outcome": "success",
  "containerId": "<app-container-id>",
  "composeProjectName": "myapp",
  "remoteUser": "vscode",
  "remoteWorkspaceFolder": "/workspace"
}
```

## Project Name from .env

The `.env` file defines `COMPOSE_PROJECT_NAME=myapp`, ensuring consistent project naming across invocations.

## Testing Profile Activation

Check which services are running:

```bash
docker compose ps
```

Expected output shows only services for active profiles.

## Use Cases

### Development with Email Testing
```bash
COMPOSE_PROFILES=dev deacon up --workspace-folder .
# Access mailcatcher at http://localhost:1080
```

### Running Integration Tests
```bash
COMPOSE_PROFILES=test deacon up --workspace-folder .
# Test database available at postgresql://testuser:testpass@test-db:5432/testdb
```

### Production Simulation
```bash
COMPOSE_PROFILES=prod deacon up --workspace-folder .
# Nginx reverse proxy available at http://localhost:8080
```

## Cleanup

```bash
# Stop all services (respects active profiles)
docker compose down

# Stop all services including profiled ones
docker compose --profile dev --profile test --profile prod down

# Remove volumes
docker compose down -v
```

## Related Examples

- `compose-basic/` - Basic compose without profiles
- `basic-image/` - Single container setup
- `lifecycle-hooks/` - Lifecycle commands with compose
