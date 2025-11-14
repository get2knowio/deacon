# Quickstart — Features Publish

This guide shows how to publish a packaged feature to an OCI registry with semantic tags and collection metadata.

## Prerequisites
- Packaged feature artifact (or let the CLI package on the fly)
- Access to registry (default `ghcr.io`), with authentication configured (Docker config helpers or env vars)

## Command

Text mode:

```
deacon features publish ./path/to/feature --namespace owner/repo --registry ghcr.io --log-level info
```

JSON mode (machine consumption):

```
deacon features publish ./path/to/feature --namespace owner/repo --registry ghcr.io --output json
```

## Authentication

The publish command supports multiple authentication methods:

### Environment Variables
```bash
export DEACON_REGISTRY_TOKEN="your-bearer-token"
# or
export DEACON_REGISTRY_USER="username"
export DEACON_REGISTRY_PASS="password"
```

### Docker Config
The command automatically uses credentials from `~/.docker/config.json` if available.

### CLI Flags (for testing)
```bash
deacon features publish ./path/to/feature --namespace owner/repo --username myuser --password-stdin
```

## Behavior
- Computes desired tags: `X`, `X.Y`, `X.Y.Z`, `latest` from the feature version
- Lists existing tags, publishes only missing; logs skipped ones
- Publishes `devcontainer-collection.json` under `<registry>/<namespace>` (artifact tag `collection`)
- Outputs JSON with `featureId`, `version`, `digest`, `publishedTags`, `skippedTags`

## JSON Output Schema

When using `--output json`, the command produces a single JSON document matching this schema:

```json
{
  "features": [
    {
      "featureId": "owner/repo/my-feature",
      "version": "1.2.3",
      "digest": "sha256:abcdef123456...",
      "publishedTags": ["1", "1.2", "1.2.3", "latest"],
      "skippedTags": [],
      "movedLatest": true,
      "registry": "ghcr.io",
      "namespace": "owner/repo"
    }
  ],
  "collection": {
    "digest": "sha256:collection123..."
  },
  "summary": {
    "features": 1,
    "publishedTags": 4,
    "skippedTags": 0
  }
}
```

## Troubleshooting
- Invalid version → exits non‑zero with message
- Missing auth → clear error; ensure `docker login ghcr.io` or proper credentials in config
- Network errors → non‑zero exit with diagnostic; re-run when stable
