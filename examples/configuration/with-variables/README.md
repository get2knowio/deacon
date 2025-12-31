# Variable Substitution Configuration Example

## What This Demonstrates

This example showcases the powerful variable substitution capabilities of DevContainers, demonstrating how to create dynamic, context-aware configurations that adapt to different environments and users.

Key variable substitution patterns shown:
- **Workspace variables**: `${localWorkspaceFolder}` for dynamic path resolution
- **Container variables**: `${devcontainerId}` for unique container identification  
- **Environment variables**: `${localEnv:VAR}` for accessing host environment
- **Variable composition**: Combining variables like `project-${devcontainerId}`
- **Missing variable handling**: Graceful handling of undefined environment variables

## Why This Matters

Variable substitution is essential for:
- **Multi-user environments**: Configurations that work across different user accounts and machines
- **Dynamic paths**: Adapting to different workspace locations and folder structures
- **Environment-specific configs**: Different behavior for development, staging, and production
- **Reusable configurations**: Templates that can be shared across projects and teams
- **Security**: Injecting secrets and credentials without hardcoding them

## DevContainer Specification References

This example implements key aspects from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Variable Substitution](https://containers.dev/implementors/spec/#variables-in-devcontainer-json)**: Complete variable substitution syntax and behavior
- **[Built-in Variables](https://containers.dev/implementors/spec/#variables)**: Standard variables like `localWorkspaceFolder`, `devcontainerId`
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Accessing and using host environment variables
- **[Configuration](https://containers.dev/implementors/spec/#devcontainer-json)**: How variables integrate with all configuration properties
- **[Workspace folder](https://containers.dev/implementors/spec/#workspace-folder)**: Dynamic workspace folder configuration

## Try
```sh
deacon read-configuration --config devcontainer.jsonc --progress json
```
