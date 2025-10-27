# Additional Features Example

This example demonstrates using `--additional-features` to add features at runtime.

## Usage

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --additional-features '{}'
```

Or with actual features (requires registry access):

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json \
  --include-features-configuration \
  --additional-features '{"ghcr.io/devcontainers/features/node:1": "lts"}'
```

## Expected Output

The features from both the config file and `--additional-features` are merged and included in the output.

## What It Demonstrates

- Runtime feature addition
- Feature merging behavior
- JSON object format for additional features

## Format

The `--additional-features` value must be a JSON object where:
- Keys are feature IDs (e.g., `ghcr.io/devcontainers/features/node:1`)
- Values can be:
  - String: version or simple option value (e.g., `"lts"` or `"18"`)
  - Boolean: `true` to install with defaults, `false` to skip
  - Object: full options object (e.g., `{"version": "18", "installTools": true}`)

## Common Errors

**Invalid JSON:**
```bash
Error: Failed to parse --additional-features JSON
```

**Non-object JSON:**
```bash
Error: --additional-features must be a JSON object.
```
