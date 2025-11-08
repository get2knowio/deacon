# Verbose Mode - Text Output

**User Story**: US4 - Combined verbose view  
**Priority**: P2  
**Format**: Text (default)

## Description

Demonstrates verbose mode which combines manifest, canonical ID, published tags, and dependency graph in a single command. This provides a complete overview of a feature.

## Use Case

- Quick comprehensive feature inspection
- Feature discovery and evaluation
- Documentation generation
- Learning about feature structure

## Prerequisites

- Network access to `ghcr.io`
- Set `DEACON_NETWORK_TESTS=1` environment variable

## Running

```bash
# Verbose output for a public feature
deacon features info verbose ghcr.io/devcontainers/features/node:1

# With debug logging
deacon features info verbose ghcr.io/devcontainers/features/node:1 --log-level debug
```

## Expected Output

Three Unicode-boxed sections in order:

### 1. Manifest and Canonical Identifier

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Manifest                                                                  ║
╚═══════════════════════════════════════════════════════════════════════════╝
{
  "schemaVersion": 2,
  "config": {...},
  "layers": [...]
}

╔═══════════════════════════════════════════════════════════════════════════╗
║ Canonical Identifier                                                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
ghcr.io/devcontainers/features/node@sha256:abc123...
```

### 2. Published Tags

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Published Tags                                                            ║
╚═══════════════════════════════════════════════════════════════════════════╝
1
1.0.0
1.1.0
...
```

### 3. Dependency Tree

```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Dependency Tree (Render with https://mermaid.live/)                      ║
╚═══════════════════════════════════════════════════════════════════════════╝
graph TD
    node["node<br/>v1"]
    common-utils["ghcr.io/devcontainers/features/common-utils<br/>"]
    
    node -.->|installs after| common-utils
```

## Section Ordering

Per spec (US4 Acceptance Scenario 1):
1. Manifest + Canonical Identifier
2. Published Tags  
3. Dependency Tree

All sections use consistent Unicode box drawing.

## Partial Failures

In text mode, if any section fails:
- Successfully fetched sections are displayed
- Failed section shows error message
- Command exits with code 1

Example:
```
╔═══════════════════════════════════════════════════════════════════════════╗
║ Manifest                                                                  ║
╚═══════════════════════════════════════════════════════════════════════════╝
{...}

Error: Failed to fetch tags: timeout after 10s
```

## Success Criteria

- ✅ All three sections displayed
- ✅ Sections in correct order
- ✅ Consistent box drawing
- ✅ Exit code 0 on success
- ✅ Partial results shown on failure with exit code 1

## Related Examples

- [verbose-json-output](../verbose-json-output/) - JSON format (manifest + tags only)
- [manifest-public-registry](../manifest-public-registry/) - Manifest section only
- [tags-public-feature](../tags-public-feature/) - Tags section only
- [dependencies-simple](../dependencies-simple/) - Dependencies section only
