# Include Merged Configuration Example

This example demonstrates using `--include-merged-configuration` to output merged configuration.

## Usage

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json --include-merged-configuration
```

## Expected Output

The command will output a JSON object with:
- `configuration`: The base configuration
- `workspace`: Workspace path information
- `featuresConfiguration`: Auto-included when computing merged config without container
- `mergedConfiguration`: Configuration merged with features/container metadata

## What It Demonstrates

- Merged configuration computation
- Auto-inclusion of features configuration
- Placeholder behavior (until issues #288 and #289 are resolved)

## Implementation Notes

Per the specification:
- When `--include-merged-configuration` is used WITHOUT a container, features are automatically resolved to derive metadata
- The merged configuration merges the base config with image metadata from either:
  - Container inspection (when `--container-id` is provided) - blocked by #288
  - Features metadata derivation (when no container) - blocked by #289

Currently, this returns the base configuration as a placeholder until the dependencies are resolved.
