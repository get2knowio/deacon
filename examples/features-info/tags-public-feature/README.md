# Tags from Public Feature

**User Story**: US2 - Discover published tags  
**Priority**: P1  
**Format**: Text (default)

## Description

Demonstrates listing all published tags for a feature from a public registry. This helps users discover available versions.

## Use Case

- Developers selecting a feature version for their devcontainer
- Browsing available updates
- Understanding feature versioning scheme
- CI/CD pipelines discovering latest versions

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable

## Running

```bash
# List all tags (note: no tag specified in reference)
deacon features info tags ghcr.io/devcontainers/features/node

# With debug logging to see pagination
deacon features info tags ghcr.io/devcontainers/features/node --log-level debug
```

## Expected Output

A Unicode-boxed section listing all published tags:

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Published Tags                                                            ║
╚═══════════════════════════════════════════════════════════════════════════╝
1
1.0.0
1.1.0
1.2.0
1.2.1
2
2.0.0
latest
```

## Tag Ordering

Tags are displayed in a deterministic order:
- Registry order if provided and consistent
- Otherwise, ascending lexicographic sort

Per spec (FR-010): Ensures stable, repeatable output across runs.

## Pagination Behavior

Per spec (FR-004):
- Automatically handles pagination via `Link` headers
- Maximum 10 pages fetched
- Maximum 1000 tags total
- 10-second timeout per request

Large repositories will show first 1000 tags.

## Success Criteria

- ✅ Command completes with exit code 0
- ✅ All available tags are listed
- ✅ Tags are sorted consistently
- ✅ Output uses Unicode box drawing
- ✅ Handles pagination transparently

## Related Examples

- [tags-json-output](../tags-json-output/) - Same command with JSON output
- [manifest-public-registry](../manifest-public-registry/) - Manifest for specific tag
