# read-configuration Command Examples

This directory contains examples demonstrating the `deacon read-configuration` command and its various flags and output modes.

## Overview

The `read-configuration` command reads and displays DevContainer configuration with variable substitution, extends resolution, and optional features/merged configuration output.

## Examples

### 1. Basic Configuration Reading (`basic/`)

Demonstrates basic usage of `read-configuration` with a simple configuration.

**Commands:**
```bash
cd basic
deacon read-configuration --workspace-folder . --config devcontainer.json
```

### 2. Include Features Configuration (`with-features/`)

Shows how to include features configuration in the output using `--include-features-configuration`.

**Commands:**
```bash
cd with-features
deacon read-configuration --workspace-folder . --config devcontainer.json --include-features-configuration
```

### 3. Include Merged Configuration (`with-merged/`)

Demonstrates the `--include-merged-configuration` flag which outputs the merged configuration.

**Commands:**
```bash
cd with-merged
deacon read-configuration --workspace-folder . --config devcontainer.json --include-merged-configuration
```

### 4. Additional Features (`with-additional-features/`)

Shows how to use `--additional-features` to add features at runtime.

**Commands:**
```bash
cd with-additional-features
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --include-features-configuration \
  --additional-features '{"ghcr.io/devcontainers/features/node:1": "lts"}'
```

### 5. Override Configuration (`with-override/`)

Demonstrates using `--override-config` for configuration overrides.

**Commands:**
```bash
cd with-override
deacon read-configuration --workspace-folder . \
  --config devcontainer.json \
  --override-config override.json
```

### 6. Secrets Management (`with-secrets/`)

Shows how to use `--secrets-file` for secure variable substitution.

**Commands:**
```bash
cd with-secrets
deacon read-configuration --workspace-folder . \
  --config devcontainer.json \
  --secrets-file secrets.env
```

### 7. Mount Workspace Git Root (`mount-git-root/`)

Demonstrates the `--mount-workspace-git-root` flag behavior.

**Commands:**
```bash
cd mount-git-root

# Mount git root (default behavior)
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --mount-workspace-git-root true

# Mount workspace folder as-is
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --mount-workspace-git-root false
```

### 8. Error Scenarios (`errors/`)

Examples of common error scenarios and their error messages.

**Examples:**
- Missing configuration file
- Invalid JSON syntax
- Circular extends chain
- Invalid feature references
- Invalid flag combinations

## Command Line Flags Reference

### Configuration Selection
- `--workspace-folder <PATH>` - Workspace folder path
- `--config <PATH>` - Configuration file path
- `--override-config <PATH>` - Override configuration file

### Output Control
- `--include-features-configuration` - Include features in output
- `--include-merged-configuration` - Include merged configuration
- `--mount-workspace-git-root <true|false>` - Control workspace root detection

### Features
- `--additional-features <JSON>` - Additional features (JSON object)
- `--skip-feature-auto-mapping` - Skip auto-mapping of feature string values

### Secrets
- `--secrets-file <PATH>` - Load secrets from file
- `--no-redact` - Disable secret redaction (debug only)

### Container Discovery
- `--container-id <ID>` - Target specific container
- `--id-label <KEY=VALUE>` - Find container by labels

### Runtime
- `--docker-path <PATH>` - Docker executable path
- `--docker-compose-path <PATH>` - Docker Compose executable path
- `--runtime <docker|podman>` - Container runtime

## Output Structure

### Basic Output
```json
{
  "configuration": { /* DevContainer config */ },
  "workspace": {
    "workspaceFolder": "/workspaces/project",
    "workspaceMount": "type=bind,source=/path,target=/workspaces/project",
    "configFolderPath": "/path/.devcontainer",
    "rootFolderPath": "/path"
  }
}
```

### With Features Configuration
```json
{
  "configuration": { /* ... */ },
  "workspace": { /* ... */ },
  "featuresConfiguration": {
    "featureSets": [
      {
        "features": [
          {
            "id": "node",
            "options": { "version": "18" }
          }
        ],
        "sourceInformation": { "type": "oci", "registry": "ghcr.io" }
      }
    ]
  }
}
```

### With Merged Configuration
```json
{
  "configuration": { /* base config */ },
  "workspace": { /* ... */ },
  "featuresConfiguration": { /* auto-included */ },
  "mergedConfiguration": { /* merged with features metadata */ }
}
```

## Related Documentation

- [CLI Specification](../../../docs/subcommand-specs/read-configuration/SPEC.md)
- [Configuration Examples](../../configuration/)
- [Feature Examples](../../features/)
- [Variable Substitution](../../configuration/with-variables/)
