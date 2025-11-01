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

## Behavior
- Computes desired tags: `X`, `X.Y`, `X.Y.Z`, `latest` from the feature version
- Lists existing tags, publishes only missing; logs skipped ones
- Publishes `devcontainer-collection.json` under `<registry>/<namespace>` (artifact tag `collection`)
- Outputs JSON with `featureId`, `version`, `digest`, `publishedTags`, `skippedTags`

## Troubleshooting
- Invalid version → exits non‑zero with message
- Missing auth → clear error; ensure `docker login ghcr.io` or proper credentials in config
- Network errors → non‑zero exit with diagnostic; re-run when stable
