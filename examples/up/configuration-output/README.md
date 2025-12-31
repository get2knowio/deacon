# Configuration Output Example

## Overview

This example demonstrates using `--include-configuration` and `--include-merged-configuration` flags to include full configuration details in the JSON output.

## Configuration Output Types

### 1. Base Configuration
The original `devcontainer.json` as parsed (before merging)

### 2. Merged Configuration
The final configuration after:
- Applying Features
- Merging image metadata
- Applying overrides
- Variable substitution

## Usage

### Default Output (No Configuration)

```bash
deacon up --workspace-folder .
```

Output:
```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace"
}
```

### Include Base Configuration

```bash
deacon up --workspace-folder . --include-configuration
```

Output:
```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace",
  "configuration": {
    "name": "Configuration Output Example",
    "image": "alpine:3.18",
    "remoteUser": "root",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
      "ENV_FROM_CONFIG": "true",
      "CONFIG_VERSION": "1.0"
    },
    "features": {
      "ghcr.io/devcontainers/features/git:1": {
        "version": "latest"
      }
    },
    "postCreateCommand": "git --version"
  }
}
```

### Include Merged Configuration

```bash
deacon up --workspace-folder . --include-merged-configuration
```

Output:
```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "root",
  "remoteWorkspaceFolder": "/workspace",
  "mergedConfiguration": {
    "name": "Configuration Output Example",
    "image": "<extended-image-with-features>",
    "remoteUser": "root",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
      "ENV_FROM_CONFIG": "true",
      "CONFIG_VERSION": "1.0",
      "...": "additional vars from Features/metadata"
    },
    "...": "additional fields from image metadata"
  }
}
```

### Include Both Configurations

```bash
deacon up --workspace-folder . \
  --include-configuration \
  --include-merged-configuration
```

Output includes both `configuration` and `mergedConfiguration` fields.

## Use Cases

### 1. Configuration Debugging

Verify what configuration is being used:

```bash
deacon up --workspace-folder . --include-configuration | jq '.configuration'
```

### 2. Feature Impact Analysis

Compare base vs merged configuration to see Feature contributions:

```bash
deacon up --workspace-folder . \
  --include-configuration \
  --include-merged-configuration \
  | jq '{base: .configuration, merged: .mergedConfiguration}'
```

### 3. CI/CD Configuration Validation

Validate configuration in pipelines:

```bash
#!/bin/bash
OUTPUT=$(deacon up --workspace-folder . --include-merged-configuration)
REMOTE_USER=$(echo "$OUTPUT" | jq -r '.mergedConfiguration.remoteUser')

if [ "$REMOTE_USER" != "root" ]; then
  echo "Error: Expected non-root user"
  exit 1
fi
```

### 4. Documentation Generation

Extract configuration for documentation:

```bash
deacon up --workspace-folder . --include-configuration \
  | jq '.configuration' > docs/devcontainer-config.json
```

### 5. Configuration Diffing

Compare configurations across branches:

```bash
# Main branch
git checkout main
deacon up --workspace-folder . --include-merged-configuration > /tmp/main-config.json

# Feature branch
git checkout feature-branch
deacon up --workspace-folder . --include-merged-configuration > /tmp/feature-config.json

# Diff
diff <(jq -S '.mergedConfiguration' /tmp/main-config.json) \
     <(jq -S '.mergedConfiguration' /tmp/feature-config.json)
```

## JSON Processing Examples

### Extract Specific Fields

```bash
# Get remote user
deacon up --workspace-folder . --include-configuration \
  | jq -r '.configuration.remoteUser'

# Get all environment variables
deacon up --workspace-folder . --include-merged-configuration \
  | jq '.mergedConfiguration.remoteEnv'

# List all features
deacon up --workspace-folder . --include-configuration \
  | jq -r '.configuration.features | keys[]'
```

### Validate Configuration Schema

```bash
deacon up --workspace-folder . --include-configuration \
  | jq -e '.configuration.image != null' \
  && echo "✓ Image specified" \
  || echo "✗ No image specified"
```

### Export for External Tools

```bash
# Export as YAML
deacon up --workspace-folder . --include-merged-configuration \
  | yq -P '.mergedConfiguration' > config.yaml

# Export as environment variables
deacon up --workspace-folder . --include-merged-configuration \
  | jq -r '.mergedConfiguration.remoteEnv | to_entries[] | "\(.key)=\(.value)"' \
  > .env
```

## Expected Output Structure

With both flags enabled:

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "<user>",
  "remoteWorkspaceFolder": "<path>",
  "configuration": {
    "...": "original devcontainer.json content"
  },
  "mergedConfiguration": {
    "...": "final configuration after all processing"
  }
}
```

## Performance Note

Including configuration in output adds minimal overhead:
- Configurations are already loaded internally
- Only JSON serialization is added
- No additional container operations required

## Cleanup

```bash
docker rm -f <container-id>
```

## Related Examples

- `basic-image/` - Simple output without configuration
- `with-features/` - See Feature impact on merged configuration
- `lifecycle-hooks/` - Configuration with lifecycle commands
