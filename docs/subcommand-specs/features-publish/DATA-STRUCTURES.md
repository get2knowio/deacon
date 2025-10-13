# Features Publish Data Structures

## Publish Result (conceptual)
```json
{
  "featureId": "<id>",
  "digest": "sha256:...",
  "publishedTags": ["1", "1.2", "1.2.3", "latest"]
}
```

## Collection Reference
```json
{
  "registry": "ghcr.io",
  "namespace": "owner/repo"
}
```

## Authentication (env conventions)
- `DOCKER_CONFIG` pointing to auth.json with registry credentials.
- `DEVCONTAINERS_OCI_AUTH`: `"host|username|password"` used in tests.

