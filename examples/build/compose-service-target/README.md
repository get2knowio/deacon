# Compose Service Target Build Example

This example demonstrates building a Docker Compose service using `deacon build`.

## Structure

- `docker-compose.yml` - Defines multiple services (app and db)
- `Dockerfile` - Build definition for the app service
- `.devcontainer.json` - Targets the app service for build

## Usage

Build the targeted service:

```bash
deacon build --workspace-folder .
```

Build with custom tags:

```bash
deacon build --workspace-folder . --image-name myapp:latest
```

## Behavior

- Only the service specified in `.devcontainer.json` (app) is built
- The db service is not built since it uses a pre-existing image
- BuildKit-only flags (--push, --output, --platform, --cache-to) are not supported in compose mode
