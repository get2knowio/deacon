# Image Reference Build Example

This example demonstrates building from an image reference using `deacon build`.

## Structure

- `.devcontainer.json` - References a base image (alpine:3.19) instead of a Dockerfile

## Usage

Build from image reference:

```bash
deacon build --workspace-folder .
```

Build with custom tags and labels:

```bash
deacon build --workspace-folder . --image-name myimage:latest --label "version=1.0"
```

## Behavior

- Creates a temporary Dockerfile that extends the base image
- Applies user-specified labels via `--label` flags
- Applies devcontainer metadata labels
- Supports all standard build flags (--push, --output, --platform, --cache-to)
- Future enhancement: Apply features specified in configuration
