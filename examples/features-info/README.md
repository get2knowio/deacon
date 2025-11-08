# Features Info Examples

This directory contains examples demonstrating the `deacon features info` command functionality, which provides information about features from both local paths and OCI registries.

## Overview

The `features info` command supports four modes:
- **manifest**: Display OCI manifest and canonical identifier
- **tags**: List published tags from the registry
- **dependencies**: Visualize dependency graph (text-only, as Mermaid)
- **verbose**: Combined view of manifest, tags, and dependencies

All modes support two output formats:
- **text** (default): Human-readable with Unicode-boxed sections
- **json**: Machine-readable structured output

## Examples by User Story

### User Story 1: Inspect Manifest and Canonical ID (Priority: P1)

- **[manifest-public-registry/](manifest-public-registry/)** - Fetch manifest from a public OCI registry
- **[manifest-local-feature/](manifest-local-feature/)** - Read manifest from a local feature directory
- **[manifest-json-output/](manifest-json-output/)** - Get manifest in JSON format for automation

### User Story 2: Discover Published Tags (Priority: P1)

- **[tags-public-feature/](tags-public-feature/)** - List all published tags for a feature
- **[tags-json-output/](tags-json-output/)** - Get tags list in JSON format

### User Story 3: Visualize Dependency Graph (Priority: P2)

- **[dependencies-simple/](dependencies-simple/)** - View dependency graph for a feature with basic dependencies
- **[dependencies-complex/](dependencies-complex/)** - View complex dependency relationships

### User Story 4: Combined Verbose View (Priority: P2)

- **[verbose-text-output/](verbose-text-output/)** - See all information at once in text format
- **[verbose-json-output/](verbose-json-output/)** - Get combined manifest and tags in JSON

## Edge Cases

- **[error-handling-invalid-ref/](error-handling-invalid-ref/)** - Demonstrates error handling for invalid references
- **[error-handling-network-failure/](error-handling-network-failure/)** - Shows timeout and network error behavior
- **[local-feature-only-manifest/](local-feature-only-manifest/)** - Local features only support manifest mode

## Quick Start

### Basic Usage

```bash
# Text output (default)
deacon features info manifest ghcr.io/devcontainers/features/node:1

# JSON output
deacon features info manifest ghcr.io/devcontainers/features/node:1 --output-format json

# List tags
deacon features info tags ghcr.io/devcontainers/features/node

# View dependencies
deacon features info dependencies ghcr.io/devcontainers/features/node:1

# Verbose mode
deacon features info verbose ghcr.io/devcontainers/features/node:1
```

### Local Feature

```bash
cd examples/features-info/manifest-local-feature
deacon features info manifest ./my-feature
```

## Testing Examples

Each example includes a README.md with:
- Description and use case
- Prerequisites (if any)
- Commands to run
- Expected output format
- Success criteria

Run all examples:
```bash
cd examples/features-info
for dir in */; do
  echo "Testing: $dir"
  (cd "$dir" && bash -c "$(grep -A10 '## Running' README.md | grep '^deacon' | head -1)")
done
```

## Notes

- Network tests require `DEACON_NETWORK_TESTS=1` environment variable
- JSON mode always prints only JSON to stdout (logs go to stderr)
- Error cases produce `{}` in JSON mode with exit code 1
- Local features set `canonicalId` to `null` in JSON output
- Dependencies mode is text-only (JSON mode returns error)
