# Basic Configuration Example

## What This Demonstrates

This example shows a complete, production-ready DevContainer configuration that combines essential elements of containerized development environments. It demonstrates:

- **Multiple features**: Integration of common development tools (git, common-utils) from the DevContainer features registry
- **Lifecycle commands**: Automated setup through onCreate/postCreate/postStart/postAttach hooks
- **Environment configuration**: Container and remote environment variables for development workflows
- **Port forwarding**: Automatic exposure of development server ports
- **Volume mounts**: Persistent caching of build artifacts and dependencies
- **Security settings**: Proper container privileges and initialization

## Why This Matters

This configuration pattern is ideal for:
- **Team standardization**: Ensuring all developers have identical development environments
- **Onboarding**: New team members get a fully configured environment in minutes
- **CI/CD consistency**: Same environment for development, testing, and deployment
- **Dependency isolation**: Avoiding "works on my machine" issues by containerizing all dependencies

## DevContainer Specification References

This example aligns with several key sections of the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Configuration](https://containers.dev/implementors/spec/#devcontainer-json)**: Core devcontainer.json structure and properties
- **[Features](https://containers.dev/implementors/spec/#features)**: Installing and configuring development tools via features
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Setting container and remote environment variables  
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Automated setup through lifecycle hooks
- **[Port Attributes](https://containers.dev/implementors/spec/#port-attributes)**: Configuring port forwarding for development servers

## Run
```sh
deacon read-configuration --config devcontainer.jsonc
```
(From inside this directory.)
