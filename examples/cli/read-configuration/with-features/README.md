# Include Features Configuration Example

This example demonstrates using `--include-features-configuration` to output feature information.

## Usage

```bash
deacon read-configuration --workspace-folder . --config devcontainer.json --include-features-configuration
```

## Expected Output

The command will output a JSON object with:
- `configuration`: The parsed DevContainer configuration
- `workspace`: Workspace path information
- `featuresConfiguration`: Feature resolution information

## What It Demonstrates

- Including features configuration in output
- Feature set structure
- Source information for features

## Note

This example uses empty features to avoid registry calls. In real usage, you would have actual feature references like:

```json
{
  "features": {
    "ghcr.io/devcontainers/features/node:1": {
      "version": "18"
    }
  }
}
```
