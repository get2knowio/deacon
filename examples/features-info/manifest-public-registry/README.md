# Manifest from Public Registry

**User Story**: US1 - Inspect manifest and canonical ID  
**Priority**: P1  
**Format**: Text (default)

## Description

Demonstrates fetching the OCI manifest and canonical identifier from a public registry. This is the most common use case for verifying feature integrity and provenance.

## Use Case

- CI systems verifying feature digests before installation
- Security audits requiring canonical identification
- Understanding the structure of a published feature

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable

## Running

```bash
# Fetch manifest for a specific tag
deacon features info manifest ghcr.io/devcontainers/features/node:1

# With debug logging
deacon features info manifest ghcr.io/devcontainers/features/node:1 --log-level debug
```

## Expected Output

The command produces two Unicode-boxed sections:

1. **Manifest** - The complete OCI manifest JSON
2. **Canonical Identifier** - The registry path with SHA256 digest

Example:
```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Manifest                                                                  ║
╚═══════════════════════════════════════════════════════════════════════════╝
{
  "schemaVersion": 2,
  "config": {
    "mediaType": "application/vnd.devcontainers.config.v0+json",
    "digest": "sha256:...",
    "size": 1234
  },
  "layers": [...]
}

╔═══════════════════════════════════════════════════════════════════════════╗
║ Canonical Identifier                                                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
ghcr.io/devcontainers/features/node@sha256:abc123...
```

## Success Criteria

- ✅ Command completes with exit code 0
- ✅ Manifest section displays valid JSON
- ✅ Canonical Identifier includes `@sha256:` digest
- ✅ Both sections use Unicode box drawing
- ✅ Output fits within standard 80-column terminal

## Related Examples

- [manifest-json-output](../manifest-json-output/) - Same command with JSON output
- [manifest-local-feature](../manifest-local-feature/) - Manifest from local feature
