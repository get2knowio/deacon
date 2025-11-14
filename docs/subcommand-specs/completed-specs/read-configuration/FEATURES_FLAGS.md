# Read-Configuration: Features Resolution Flags

This document describes the features resolution flags and their behavior in the `read-configuration` subcommand.

## Overview

The `read-configuration` subcommand supports three flags for controlling feature resolution:

1. `--include-features-configuration`: Forces feature resolution and includes it in output
2. `--additional-features <JSON>`: Merges additional features with config features
3. `--skip-feature-auto-mapping`: Hidden testing flag to disable auto-mapping

## Flags

### `--include-features-configuration`

**Type**: Boolean flag (optional)
**Default**: `false`

Forces computation and output of feature resolution details even when `--include-merged-configuration` is not specified.

**Behavior**:
- Resolves all features from the devcontainer configuration
- Fetches feature metadata from OCI registries
- Resolves feature dependencies
- Groups features by registry in the output
- Adds `featuresConfiguration` field to JSON output

**Example**:
```bash
deacon read-configuration --workspace-folder . --include-features-configuration
```

**Output Structure**:
```json
{
  "configuration": { ... },
  "featuresConfiguration": {
    "featureSets": [
      {
        "features": [
          {
            "id": "node",
            "options": {
              "version": "lts"
            }
          }
        ],
        "sourceInformation": {
          "type": "oci",
          "registry": "ghcr.io"
        }
      }
    ]
  }
}
```

### `--additional-features <JSON>`

**Type**: JSON string (optional)
**Format**: Must be a valid JSON object mapping feature IDs to options

Accepts additional features to merge with features from the devcontainer.json. The features are merged using the same logic as the `features plan` command.

**Value Formats**:
- **Boolean**: `true` to enable feature with default options, `false` to skip
- **String**: Auto-mapped to `{"version": "<value>"}` (e.g., `"lts"` becomes `{"version": "lts"}`)
- **Object**: Full options object with key-value pairs

**Examples**:

1. Simple string value (auto-mapped to version):
```bash
deacon read-configuration \
  --workspace-folder . \
  --additional-features '{"ghcr.io/devcontainers/features/node:1": "lts"}'
```

2. Boolean value:
```bash
deacon read-configuration \
  --workspace-folder . \
  --additional-features '{"ghcr.io/devcontainers/features/docker-in-docker:2": true}'
```

3. Full options object:
```bash
deacon read-configuration \
  --workspace-folder . \
  --additional-features '{
    "ghcr.io/devcontainers/features/node:1": {
      "version": "18",
      "nvmVersion": "latest"
    }
  }'
```

4. Empty object (no additional features):
```bash
deacon read-configuration \
  --workspace-folder . \
  --additional-features '{}'
```

**Validation**:
- Input must be valid JSON
- JSON must be an object (not array or primitive)
- Arrays like `["feature1", "feature2"]` are rejected
- Primitives like `"string"` or `123` are rejected

**Error Messages**:
- Invalid JSON: `Failed to parse --additional-features JSON: <input>`
- Non-object JSON: `--additional-features must be a JSON object.`

### `--skip-feature-auto-mapping`

**Type**: Boolean flag (optional, hidden)
**Default**: `false`

Hidden testing flag that disables auto-mapping of string values to version options. When enabled, string feature values are treated as empty options instead of being mapped to `{"version": "<value>"}`.

**Usage** (for testing only):
```bash
deacon read-configuration \
  --workspace-folder . \
  --skip-feature-auto-mapping
```

## Feature Resolution Behavior

### When Features Are Resolved

Features are automatically resolved and included in output when:
1. `--include-features-configuration` flag is set, OR
2. `--include-merged-configuration` is set AND no container is found

This aligns with the spec requirement that merged configuration without a container requires features for metadata derivation.

### Registry Grouping

Features are grouped by their source registry in the output:
- Each `FeatureSet` contains features from a single registry
- Registry is extracted from the feature's OCI reference
- Default registry is `ghcr.io` if not specified

**Example** with multiple registries:
```json
{
  "featuresConfiguration": {
    "featureSets": [
      {
        "features": [
          {
            "id": "node",
            "options": {"version": "lts"}
          }
        ],
        "sourceInformation": {
          "type": "oci",
          "registry": "ghcr.io"
        }
      },
      {
        "features": [
          {
            "id": "custom-feature",
            "options": {}
          }
        ],
        "sourceInformation": {
          "type": "oci",
          "registry": "myregistry.io"
        }
      }
    ]
  }
}
```

### Options Parsing

Feature options are parsed from the configuration value:

1. **Object values**: Keys and values extracted directly (bool/string only)
   ```json
   "ghcr.io/devcontainers/features/node:1": {
     "version": "18",
     "nvmVersion": "latest"
   }
   ```

2. **String values**: Auto-mapped to version option (unless `--skip-feature-auto-mapping`)
   ```json
   "ghcr.io/devcontainers/features/node:1": "lts"
   ```
   Becomes:
   ```json
   {
     "id": "node",
     "options": {
       "version": "lts"
     }
   }
   ```

3. **Boolean values**:
   - `true`: Feature enabled with no options
   - `false`: Feature skipped (not included in resolution)

4. **Other types** (numbers, arrays, nested objects): Dropped from options

## Output Schema

### Top-Level Structure

```typescript
{
  configuration: DevContainerConfig,      // Always present
  featuresConfiguration?: FeaturesConfig, // Present when requested
  mergedConfiguration?: MergedConfig      // Present when requested
}
```

### FeaturesConfiguration

```typescript
{
  featureSets: FeatureSet[],
  dstFolder?: string  // Optional destination folder (computed)
}
```

### FeatureSet

```typescript
{
  features: Feature[],
  sourceInformation: SourceInformation,
  internalVersion?: string,
  computedDigest?: string
}
```

### Feature

```typescript
{
  id: string,
  options?: Record<string, string | boolean>
}
```

### SourceInformation

```typescript
{
  type: "oci",
  registry: string  // e.g., "ghcr.io"
}
```

## Examples

### Example 1: Basic feature resolution

**Command**:
```bash
deacon read-configuration \
  --workspace-folder /path/to/project \
  --include-features-configuration
```

**Config**:
```json
{
  "name": "my-project",
  "image": "ubuntu:latest",
  "features": {
    "ghcr.io/devcontainers/features/node:1": "lts",
    "ghcr.io/devcontainers/features/docker-in-docker:2": true
  }
}
```

**Output**:
```json
{
  "configuration": {
    "name": "my-project",
    "image": "ubuntu:latest",
    "features": {
      "ghcr.io/devcontainers/features/node:1": "lts",
      "ghcr.io/devcontainers/features/docker-in-docker:2": true
    }
  },
  "featuresConfiguration": {
    "featureSets": [
      {
        "features": [
          {
            "id": "node",
            "options": {
              "version": "lts"
            }
          },
          {
            "id": "docker-in-docker"
          }
        ],
        "sourceInformation": {
          "type": "oci",
          "registry": "ghcr.io"
        }
      }
    ]
  }
}
```

### Example 2: Adding features via CLI

**Command**:
```bash
deacon read-configuration \
  --workspace-folder /path/to/project \
  --include-features-configuration \
  --additional-features '{
    "ghcr.io/devcontainers/features/python:1": "3.11"
  }'
```

This merges the CLI features with config features and resolves all of them.

### Example 3: Empty config with CLI features

**Command**:
```bash
deacon read-configuration \
  --workspace-folder /path/to/project \
  --include-features-configuration \
  --additional-features '{
    "ghcr.io/devcontainers/features/node:1": {"version": "18"},
    "ghcr.io/devcontainers/features/git:1": true
  }'
```

**Config**:
```json
{
  "name": "my-project",
  "image": "ubuntu:latest"
}
```

This adds features entirely through CLI without any in the config.

## Backward Compatibility

### Breaking Change in Output Format

The output format changed from a flat configuration object to a structured payload:

**Before**:
```json
{
  "name": "my-container",
  "image": "ubuntu:latest",
  ...
}
```

**After**:
```json
{
  "configuration": {
    "name": "my-container",
    "image": "ubuntu:latest",
    ...
  }
}
```

**Migration**: Consumers should access the configuration via the `configuration` key instead of the root level.

### Handling Missing featuresConfiguration

The `featuresConfiguration` field is optional and only present when:
- `--include-features-configuration` is set, OR
- `--include-merged-configuration` is set without a container

Consumers should check for the field's presence before accessing it:

```javascript
const output = JSON.parse(stdout);
if (output.featuresConfiguration) {
  // Process features
}
```

## Error Handling

### Common Errors

1. **Invalid JSON in --additional-features**
   ```
   Error: Failed to parse --additional-features JSON: invalid JSON
   ```

2. **Non-object JSON in --additional-features**
   ```
   Error: --additional-features must be a JSON object.
   ```

3. **Invalid feature reference**
   ```
   Error: Failed to fetch feature 'invalid-ref': ...
   ```

4. **Registry not accessible**
   ```
   Error: Failed to fetch feature 'ghcr.io/...': connection error
   ```

### Graceful Degradation

- If no features are configured and `--include-features-configuration` is set, returns empty `featureSets` array
- If `--additional-features` is an empty object `{}`, it's treated as no additional features
- Feature resolution errors fail the entire command (no partial results)

## Testing

See test cases in `crates/deacon/src/commands/read_configuration.rs`:
- `test_read_configuration_include_features_flag`
- `test_read_configuration_additional_features_flag`
- `test_read_configuration_skip_feature_auto_mapping_flag`
- `test_read_configuration_string_value_auto_mapping`
- `test_read_configuration_empty_additional_features`
- `test_read_configuration_invalid_additional_features_json`
- `test_read_configuration_additional_features_not_object`
