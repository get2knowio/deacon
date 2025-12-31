# Feature With Options Example

## What This Demonstrates

This example showcases a comprehensive DevContainer feature that demonstrates the full power of the feature system. It includes:

- **Multiple option types**: Boolean, string, and enum options with defaults and validation
- **Environment configuration**: Setting container environment variables
- **Volume mounts**: Persistent storage through named volumes
- **Security settings**: Container privileges, capabilities, and security options
- **Dependency management**: Feature dependencies and installation order
- **Lifecycle hooks**: Commands that run during container creation and startup
- **Rich metadata**: Documentation URLs, descriptions, and version information

## Why This Matters

Advanced features like this enable:
- **Reusable development tools**: Packaging complex toolchains for team distribution
- **Configurable installations**: Users can customize behavior through well-defined options
- **Enterprise deployments**: Security controls and compliance requirements
- **Complex workflows**: Multi-step setup processes with proper dependency handling
- **Community sharing**: Publishing sophisticated tools to public registries

Real-world applications include:
- Database clients with connection configuration
- Language runtimes with version and extension options
- Security tools with customizable policies
- Development servers with port and path configuration

## DevContainer Specification References

This example demonstrates advanced patterns from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Feature Options](https://containers.dev/implementors/spec/#option-resolution)**: Comprehensive option types, validation, and resolution
- **[Container Environment](https://containers.dev/implementors/spec/#environment-variables)**: Setting environment variables through features
- **[Mounts](https://containers.dev/implementors/spec/#mounts)**: Volume and bind mounts for persistent data
- **[Security Options](https://containers.dev/implementors/spec/#security-options)**: Container privileges and security configurations
- **[Feature Dependencies](https://containers.dev/implementors/spec/#dependencies)**: Declaring and resolving feature dependencies
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Feature-level lifecycle hooks and commands
- **[Feature Metadata](https://containers.dev/implementors/spec/#devcontainer-feature-json-properties)**: Complete metadata specification

## Commands
```sh
# Test feature (requires install.sh script for complete functionality)
deacon features test . --progress json

# Package feature for distribution
OUT=$(mktemp -d)
deacon features package . --output "$OUT" --progress json

# Dry-run publish to see what would be published
deacon features publish . --registry ghcr.io/example/with-options-feature --dry-run --progress json
```
