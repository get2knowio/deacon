# Basic Dockerfile Build Example

## What This Demonstrates

This example shows a basic Dockerfile-based DevContainer build with:

- **Build Arguments**: Passing `--build-arg` values to customize the build
- **Environment Variables**: Setting container environment from build args
- **Image Labels**: Adding metadata labels for image identification and validation
- **JSON Output**: Using `--output-format json` to get structured build results

## Why This Matters

Understanding Dockerfile builds is essential for:
- **Custom Base Images**: Building images with specific dependencies and tools
- **Build-Time Configuration**: Parameterizing builds with build arguments
- **Image Metadata**: Tagging images with labels for discovery and management
- **CI/CD Integration**: Getting structured output for automation workflows

## DevContainer Specification References

This example aligns with:
- **[Container Build](https://containers.dev/implementors/spec/#build)**: Building images from Dockerfiles
- **[Build Properties](https://containers.dev/implementors/spec/#build-properties)**: Configuring build context and arguments

## Files

- `Dockerfile`: Simple Alpine-based image with ARG, ENV, and LABEL directives
- `devcontainer.json`: DevContainer configuration with build context and arguments

## Run

### Basic Build
```sh
deacon build --workspace-folder .
```

### Build with Custom Build Args
```sh
deacon build --workspace-folder . --build-arg FOO=BAR
```

### Build with JSON Output
```sh
deacon build --workspace-folder . --build-arg FOO=BAR --output-format json
```

## Validation

After building, you can inspect the created image to verify the build arguments and labels:

### Check Image Labels
```sh
# Get the image ID from the build output
IMAGE_ID="<image-id-from-build-output>"

# Inspect image labels
docker image inspect "$IMAGE_ID" --format '{{json .Config.Labels}}' | jq '.'
```

You should see labels like:
```json
{
  "parity.token": "__UUID__",
  "example.type": "basic-dockerfile",
  "build.arg.foo": "BAR"
}
```

### Check Environment Variables
```sh
# Inspect environment variables
docker image inspect "$IMAGE_ID" --format '{{json .Config.Env}}' | jq '.'
```

You should see `FOO_ENV=BAR` in the environment variables list.

### Verify Build Args Were Applied
```sh
# Combine both checks
docker image inspect "$IMAGE_ID" | jq '{
  labels: .Config.Labels,
  env: .Config.Env | map(select(startswith("FOO_")))
}'
```

## Expected Output

When you run with JSON output format, you'll get structured information including:
- `image_id`: The Docker image ID that was built
- `config_hash`: Hash of the devcontainer configuration
- `build_duration`: How long the build took

Example JSON output:
```json
{
  "image_id": "sha256:abc123...",
  "config_hash": "def456...",
  "build_duration": "5.2s"
}
```

## Cleanup

To remove the built image:
```sh
docker rmi "$IMAGE_ID"
```

Or remove all images with the example label:
```sh
docker images --filter "label=example.type=basic-dockerfile" -q | xargs -r docker rmi
```

## See Also

- `../platform-and-cache/` - Platform targeting and cache control
- `../secrets-and-ssh/` - BuildKit secrets and SSH forwarding
- CLI Spec: Container Build section in `docs/subcommand-specs/*/SPEC.md`
