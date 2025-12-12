# Quickstart: Up Build Parity and Metadata

This document provides working command examples for the build parity features implemented in the `up` subcommand.

## Build Options

### Cache-from and Cache-to

Use external cache sources and destinations for faster builds:

```bash
# Single cache source
deacon up --workspace-folder . --cache-from type=registry,ref=myregistry/cache:latest

# Multiple cache sources (order preserved)
deacon up --workspace-folder . \
  --cache-from type=registry,ref=myregistry/cache:v1 \
  --cache-from type=registry,ref=myregistry/cache:latest

# Cache source and destination
deacon up --workspace-folder . \
  --cache-from type=registry,ref=myregistry/cache:latest \
  --cache-to type=registry,ref=myregistry/cache:latest,mode=max
```

### BuildKit Control

Control BuildKit usage explicitly:

```bash
# Auto-detect BuildKit (default, respects DOCKER_BUILDKIT env var)
deacon up --workspace-folder . --buildkit auto

# Force legacy docker build (no BuildKit)
deacon up --workspace-folder . --buildkit never

# Disable build cache entirely
deacon up --workspace-folder . --build-no-cache
```

## Feature Controls

### Skip Feature Auto-Mapping (hidden flag)

Prevent auto-added features; only explicitly declared features are used:

```bash
# Only use features declared in devcontainer.json
deacon up --workspace-folder . --skip-feature-auto-mapping
```

Note: When enabled, CLI-provided features via `--additional-features` are ignored with an info log.

### Lockfile Validation (experimental, hidden flags)

Validate features against a lockfile before build:

```bash
# Validate against explicit lockfile (warn on mismatch, continue)
deacon up --workspace-folder . --experimental-lockfile ./devcontainer-lock.json

# Frozen mode: fail if lockfile missing or features don't match exactly
deacon up --workspace-folder . --experimental-frozen-lockfile

# Frozen mode with explicit lockfile path
deacon up --workspace-folder . \
  --experimental-lockfile ./custom-lock.json \
  --experimental-frozen-lockfile
```

## Feature Metadata in Merged Configuration

Request merged configuration to see feature metadata:

```bash
# Include merged configuration in output
deacon up --workspace-folder . --include-merged-configuration

# JSON output shows featureMetadata for each feature
# Example output structure:
# {
#   "outcome": "success",
#   "containerId": "abc123...",
#   "mergedConfiguration": {
#     "features": {
#       "ghcr.io/devcontainers/features/node:1": {}
#     },
#     "featureMetadata": [
#       {
#         "id": "ghcr.io/devcontainers/features/node:1",
#         "metadata": {}
#       }
#     ]
#   }
# }
```

## Combined Examples

### CI Pipeline with Caching

```bash
deacon up --workspace-folder . \
  --cache-from type=registry,ref=ghcr.io/myorg/devcontainer-cache:latest \
  --cache-to type=registry,ref=ghcr.io/myorg/devcontainer-cache:latest,mode=max \
  --experimental-frozen-lockfile \
  --include-merged-configuration
```

### Local Development without Cache

```bash
deacon up --workspace-folder . --buildkit never --build-no-cache
```

## Development Commands

After making changes, run these commands to verify:

```bash
# Format and lint
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Unit tests (fast feedback)
make test-nextest-unit

# Docker integration tests (for build/runtime changes)
make test-nextest-docker

# Fast test suite (excludes slow tests)
make test-nextest-fast

# Full test suite (before PR)
make test-nextest
```

## Key Implementation Files

- `crates/deacon/src/commands/up/args.rs` - CLI argument parsing and BuildOptions construction
- `crates/deacon/src/commands/up/mod.rs` - Main up command with lockfile validation
- `crates/deacon/src/commands/up/image_build.rs` - Dockerfile build with BuildOptions
- `crates/deacon/src/commands/up/features_build.rs` - Feature build with BuildOptions
- `crates/core/src/build/mod.rs` - BuildOptions struct definition
- `crates/core/src/lockfile.rs` - Lockfile validation logic
- `crates/core/src/features.rs` - Feature merging with skip-auto-mapping
