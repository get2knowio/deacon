# Quickstart — Features Info

This guide shows how to use the Features Info subcommand once implemented.

## Prerequisites
- Deacon CLI built from this repository
- Network access for registry queries

## Manifest and Canonical ID

Text mode:

```sh
# Prints boxed sections: Manifest and Canonical Identifier
deacon features info manifest ghcr.io/devcontainers/features/node:1
```

JSON mode:

```sh
# Prints a single JSON document with { manifest, canonicalId }
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json
```

Expected JSON output structure:
```json
{
  "manifest": {
    "schemaVersion": 2,
    "mediaType": "application/vnd.oci.image.manifest.v1+json",
    "config": { ... },
    "layers": [ ... ]
  },
  "canonicalId": "sha256:abc123..."
}
```

### Local Feature Example

```sh
# For local features, canonicalId is null
deacon features info manifest ./my-local-feature
deacon features info manifest ./my-local-feature --output-format json
```

Expected JSON output for local features:
```json
{
  "manifest": {
    "id": "my-feature",
    "version": "1.0.0",
    ...
  },
  "canonicalId": null
}
```

## Published Tags

Text mode:

```sh
# Text mode prints a boxed "Published Tags" list
deacon features info tags ghcr.io/devcontainers/features/node
```

JSON mode:

```sh
# JSON mode prints { publishedTags: [...] }
deacon features info tags ghcr.io/devcontainers/features/node --output-format json
```

Expected JSON output:
```json
{
  "publishedTags": [
    "1",
    "1.0",
    "1.0.0",
    "latest"
  ]
}
```

## Dependency Graph (text only)

```sh
# Emits Mermaid graph; copy into https://mermaid.live/ to render
deacon features info dependencies ghcr.io/devcontainers/features/node:1
```

Expected text output:
```
┌─ Dependency Tree (Render with https://mermaid.live/) ──────────┐
│ graph TD                                                        │
│     node --> common-utils                                       │
│     node --> python                                            │
└─────────────────────────────────────────────────────────────────┘
```

### Error Case: JSON Mode Not Supported

```sh
# JSON mode returns {} and exits with code 1
deacon features info dependencies ghcr.io/devcontainers/features/node:1 --output-format json
# Output: {}
# Exit code: 1
```

## Verbose (combined)

Text mode:

```sh
# Text mode: three boxed sections (manifest/canonicalId, tags, dependency graph)
deacon features info verbose ghcr.io/devcontainers/features/node:1
```

Expected text output shows three sections:
1. Manifest (JSON)
2. Canonical Identifier
3. Published Tags (list)
4. Dependency Tree (Mermaid)

JSON mode:

```sh
# JSON mode: union of manifest/canonicalId and publishedTags
deacon features info verbose ghcr.io/devcontainers/features/node:1 --output-format json
```

Expected JSON output:
```json
{
  "manifest": { ... },
  "canonicalId": "sha256:abc123...",
  "publishedTags": ["1", "1.0", "latest"]
}
```

### Error Case: Partial Failure

```sh
# If any sub-mode fails, include errors map and exit with code 1
deacon features info verbose ghcr.io/invalid/feature:tag --output-format json
```

Expected JSON output with errors:
```json
{
  "errors": {
    "manifest": "Failed to fetch manifest: ...",
    "tags": "Failed to list tags: ...",
    "dependencies": "Failed to fetch feature: ..."
  }
}
```

## Error Handling Examples

### Invalid Feature Reference

```sh
# Text mode: error message to stderr
deacon features info manifest invalid-ref
# Exit code: 1

# JSON mode: empty object and exit code 1
deacon features info manifest invalid-ref --output-format json
# Output: {}
# Exit code: 1
```

### Network Timeout

```sh
# 10-second timeout applies to all modes
deacon features info manifest ghcr.io/slow/feature:1
# Error after 10s timeout
```

## Exit Codes
- 0 on success
- 1 on any error

## Notes
- For local feature paths, `canonicalId` is `null` in JSON mode.
- Pagination limits: up to 10 pages or 1000 tags; per-request timeout 10s.
- Tags are always sorted lexicographically for deterministic output.
- Dependencies mode only supports text output (JSON returns `{}` + exit 1).
- Verbose mode in JSON omits the dependency graph.
