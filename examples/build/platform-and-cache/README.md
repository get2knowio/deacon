# Platform and Cache Build Example

## What This Demonstrates

This example shows how to control Docker build behavior with:

- **Platform Targeting**: Building images for specific CPU architectures using `--platform`
- **Cache Control**: Disabling build cache with `--no-cache` for fresh builds
- **Build Performance**: Understanding cache behavior and when to bypass it

## Why This Matters

Platform and cache controls are critical for:
- **Multi-Architecture Support**: Building images for ARM, AMD64, etc.
- **CI/CD Pipelines**: Ensuring reproducible builds without stale cache
- **Development Iteration**: Forcing rebuilds when dependencies change
- **Cross-Platform Development**: Testing behavior across different architectures

## DevContainer Specification References

This example aligns with:
- **[Build Options](https://containers.dev/implementors/spec/#build-properties)**: Platform targeting and cache control
- CLI Spec: Container Build section in `docs/subcommand-specs/*/SPEC.md`

## Files

- `Dockerfile`: Simple Alpine image that shows platform detection
- `devcontainer.json`: Basic DevContainer configuration

## Run

### Default Build (Uses Cache, Host Platform)
```sh
deacon build --workspace-folder .
```

### Build Without Cache
Forces a complete rebuild, ignoring any cached layers:
```sh
deacon build --workspace-folder . --no-cache
```

The `--no-cache` flag is useful when:
- Dependencies have changed (e.g., apt packages, npm modules)
- You want to ensure a clean build
- Debugging build issues that might be masked by cache

### Build for Specific Platform
Target a specific CPU architecture:
```sh
# Build for AMD64 (x86_64)
deacon build --workspace-folder . --platform linux/amd64

# Build for ARM64 (Apple Silicon, ARM servers)
deacon build --workspace-folder . --platform linux/arm64

# Build for ARM v7 (Raspberry Pi, older ARM devices)
deacon build --workspace-folder . --platform linux/arm/v7
```

### Combine Flags
```sh
deacon build --workspace-folder . --platform linux/amd64 --no-cache
```

## Validation

### Check Platform Architecture
After building, verify the image was built for the correct platform:

```sh
IMAGE_ID="<image-id-from-build-output>"

# Check the architecture
docker image inspect "$IMAGE_ID" --format '{{.Architecture}}'

# Full platform information
docker image inspect "$IMAGE_ID" --format '{{.Os}}/{{.Architecture}}'
```

### Verify Cache Behavior

Build twice and observe the difference:

```sh
# First build (no cache available)
time deacon build --workspace-folder . --output-format json > build1.json

# Second build (uses cache)
time deacon build --workspace-folder . --output-format json > build2.json

# Compare build times
echo "First build: $(jq -r .build_duration build1.json)"
echo "Second build: $(jq -r .build_duration build2.json)"
```

The second build should be significantly faster because Docker reuses cached layers.

Now rebuild without cache:
```sh
# Third build (forced fresh build)
time deacon build --workspace-folder . --no-cache --output-format json > build3.json

echo "No-cache build: $(jq -r .build_duration build3.json)"
```

The no-cache build time should be similar to the first build.

### Check Build Timestamp
The Dockerfile creates a timestamp file showing when the build ran:

```sh
IMAGE_ID="<image-id-from-build-output>"

# Run the image to see the build timestamp
docker run --rm "$IMAGE_ID" cat /build-timestamp.txt
```

When using cache, the timestamp will be from the cached layer. With `--no-cache`, you'll get a fresh timestamp.

## Platform Support Notes

### Supported Platforms

Docker/BuildKit commonly supports:
- `linux/amd64` - Standard x86_64 (Intel/AMD)
- `linux/arm64` - ARM 64-bit (Apple M1/M2, ARM servers)
- `linux/arm/v7` - ARM 32-bit (Raspberry Pi)
- `linux/arm/v6` - Older ARM devices
- `linux/386` - 32-bit Intel/AMD

### Cross-Platform Building

Building for a different architecture than your host requires:
1. **QEMU** emulation (for non-native builds)
2. **BuildKit** with multi-platform support

If the platform is not supported, you'll see an error like:
```
Error: Image platform (linux/amd64) does not match the specified platform
```

### Performance Considerations

- **Native builds**: Fast, no emulation overhead
- **Cross-platform builds**: Slower due to QEMU emulation
- **Multi-platform builds**: Can build multiple platforms simultaneously with BuildKit

## Expected Output

### Standard Build
```
Building container image...
Successfully built image: sha256:abc123...
```

### With JSON Output
```json
{
  "image_id": "sha256:abc123...",
  "config_hash": "def456...",
  "build_duration": "3.5s"
}
```

### Platform-Specific Build
```
Building container image for platform: linux/amd64
Successfully built image: sha256:abc123...
```

## Cleanup

Remove built images:
```sh
docker images --filter "label=example.type=platform-and-cache" -q | xargs -r docker rmi
```

## See Also

- `../basic-dockerfile/` - Basic Dockerfile builds with build args
- `../secrets-and-ssh/` - BuildKit secrets and SSH forwarding
- Docker docs on [multi-platform builds](https://docs.docker.com/build/building/multi-platform/)
- CLI Spec: Container Build section in `docs/subcommand-specs/*/SPEC.md`
