# Minimal Feature Example

## What This Demonstrates

This example shows the simplest possible DevContainer feature - containing only the essential `id` field required for feature identification. It represents the minimal viable feature that can be packaged and distributed through OCI registries.

Despite its simplicity, this minimal feature demonstrates:
- **Feature packaging workflow**: How to create, test, and package features for distribution
- **Feature identification**: The fundamental requirement of unique feature IDs
- **OCI compatibility**: Features as distributable, versioned packages
- **Development workflow**: Local testing and validation before publishing

## Why This Matters

Starting with minimal features is valuable for:
- **Learning the ecosystem**: Understanding feature development without complexity
- **Rapid prototyping**: Quick iteration on feature concepts before adding functionality
- **Template creation**: Base structure for more complex features
- **Testing workflows**: Validating packaging and publishing pipelines
- **Educational purposes**: Teaching DevContainer feature concepts step by step

## DevContainer Specification References

This example implements core concepts from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Features](https://containers.dev/implementors/spec/#features)**: Overview of the feature system and architecture
- **[Feature Definition](https://containers.dev/implementors/spec/#devcontainer-feature-json-properties)**: Required and optional properties for feature manifests
- **[Feature Distribution](https://containers.dev/implementors/spec/#distributing-features)**: How features are packaged and distributed via OCI registries
- **[Feature Resolution](https://containers.dev/implementors/spec/#feature-resolution)**: How DevContainer tools discover and install features

## Commands
```sh
# Test feature (requires install.sh script - see note below)
deacon features test . --progress json

# Package feature for distribution  
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --progress json
```

**Note**: The test command requires an `install.sh` script for complete feature testing. This minimal example focuses on the manifest structure.
