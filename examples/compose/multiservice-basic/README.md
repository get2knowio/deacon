# Multi-Service Docker Compose Example

## What This Demonstrates

This example shows how to use Docker Compose with multiple services in a DevContainer configuration:

- **Multi-service orchestration**: Primary application service (`app`) with a Redis dependency
- **Service configuration**: Using `dockerComposeFile` and `service` properties to define the primary container
- **Run services**: Additional services started via `runServices` (Redis in this case)
- **Service communication**: Environment variables for service discovery (REDIS_HOST, REDIS_PORT)
- **Compose shutdown**: Using `shutdownAction: "stopCompose"` to stop all services on container shutdown
- **Volume management**: Persistent data storage for Redis via named volumes

## Why This Matters

This configuration pattern is ideal for:
- **Complex development environments**: Applications requiring databases, caches, message queues, or other services
- **Service dependencies**: Testing integrations with external services in isolation
- **Team consistency**: Ensuring all developers run the same service topology
- **Production parity**: Matching production multi-service architecture in development

## DevContainer Specification References

This example aligns with:
- **[Docker Compose](https://containers.dev/implementors/json_reference/#compose-specific)**: dockerComposeFile, service, runServices properties
- **[Shutdown Actions](https://containers.dev/implementors/json_reference/#shutdown-action)**: stopCompose for multi-service cleanup
- **[Lifecycle Scripts](https://containers.dev/implementors/json_reference/#lifecycle-scripts)**: postCreateCommand, postStartCommand

## Usage

### Start the multi-service environment

```sh
cd examples/compose/multiservice-basic
deacon up
```

This will:
1. Start both the `app` service (primary) and `redis` service
2. Execute the postCreateCommand
3. Execute the postStartCommand
4. Keep both services running

### Verify services are running

Check that both services are active:
```sh
docker compose ps
```

You should see both `app` and `redis` services in the "running" state.

### Execute commands in the primary service

Run a command in the primary `app` service:
```sh
deacon exec sh -lc 'echo ok'
```

This executes the command in the `app` service container (not redis).

### Test service connectivity

Verify the app can connect to Redis:
```sh
deacon exec sh -c 'ping -c 1 redis'
```

Check Redis is accessible via environment variables:
```sh
deacon exec sh -c 'echo "Redis available at $REDIS_HOST:$REDIS_PORT"'
```

### Read the configuration

View the resolved configuration:
```sh
deacon read-configuration --config devcontainer.json
```

### Stop all services

Stop and clean up the compose project:
```sh
deacon down
```

Because `shutdownAction` is set to `stopCompose`, this will stop both the app and redis services.

## Files

- **docker-compose.yml**: Defines two services (app + redis) with networking and volumes
- **devcontainer.json**: DevContainer configuration specifying compose file, primary service, and run services
- **README.md**: This file

## Notes

- The `app` service uses Alpine Linux for minimal size
- Redis uses the official `redis:7-alpine` image for persistence testing
- Both images should be available offline if pulled once, or will require network access on first use
- The workspace folder is mounted into the `app` service at `/workspace`
- Redis data persists in a named Docker volume (`redis-data`)
