# Advanced Container Lifecycle Example

## What This Demonstrates

This example showcases advanced lifecycle command patterns and real-world development environment setup:

- **Named Command Objects**: Using objects instead of arrays for better organization
- **Environment Variable Usage**: Referencing container and remote environment variables
- **Volume Mounts**: Persistent storage for dependencies and caches
- **Error Handling**: Graceful fallbacks when services aren't ready
- **Development Workflow**: Complete Node.js development environment setup

## Advanced Lifecycle Features

### Named Command Objects

Instead of simple arrays, this example uses named objects for `onCreateCommand` and `postStartCommand`:

```json
{
  "onCreateCommand": {
    "setup-dirs": "mkdir -p /tmp/lifecycle-logs ...",
    "log-creation": "echo '[onCreate] Container created...'",
    "validate-env": "echo '[onCreate] Environment: ...'"
  }
}
```

Benefits:
- Better organization and readability
- Individual command identification
- Selective execution control
- Clearer debugging output

### Environment Variable Integration

Commands reference both `containerEnv` and `remoteEnv` variables:
- `${NODE_ENV}`, `${PROJECT_NAME}` from containerEnv
- `${containerEnv:PATH}` for PATH extension

### Robust Error Handling

Commands include fallbacks for common scenarios:
```bash
# Graceful service checking
docker --version >> /tmp/lifecycle-logs/postStart.log 2>&1 || echo 'Docker not ready yet' >> /tmp/lifecycle-logs/postStart.log

# Conditional file operations  
[ ! -f package.json ] && npm init -y || echo 'package.json exists'
```

## Real-World Development Setup

This configuration creates a complete Node.js development environment:

1. **Directory Structure**: Creates `src/`, `tests/`, `docs/` directories
2. **Package Management**: Initializes npm and installs common development dependencies
3. **Volume Optimization**: Caches npm packages and node_modules for faster rebuilds
4. **Development Tools**: Express for web development, Jest for testing, Nodemon for hot reload
5. **Port Forwarding**: Multiple ports for different services (app: 3000, alt: 8080, debug: 9229)

## DevContainer Specification References

This example demonstrates:
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Advanced command organization
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Container and remote environment setup
- **[Mounts](https://containers.dev/implementors/spec/#mounts)**: Volume and bind mounts for performance
- **[Port Attributes](https://containers.dev/implementors/spec/#port-attributes)**: Multiple port forwarding

## Run

Test the configuration:
```sh
deacon read-configuration --config devcontainer.json
```

Inspect lifecycle commands structure:
```sh
deacon read-configuration --config devcontainer.json | jq '{
  onCreate: .onCreateCommand,
  postCreate: .postCreateCommand, 
  postStart: .postStartCommand,
  postAttach: .postAttachCommand
}'
```

Check environment variable usage:
```sh
deacon read-configuration --config devcontainer.json | jq '.containerEnv, .remoteEnv'
```

## Expected Execution Flow

1. **Container Creation** (`onCreateCommand`):
   - Creates directory structure
   - Logs creation time and environment variables
   - Sets up workspace folders

2. **Feature Installation**: 
   - Git tools installed
   - Docker-in-Docker configured

3. **Post-Creation** (`postCreateCommand`):
   - Node.js project initialization
   - Development dependencies installation
   - Setup completion logging

4. **Container Start** (`postStartCommand`):
   - Start time logging
   - Service availability checking
   - Workspace validation

5. **Attachment** (`postAttachCommand`):
   - Environment summary display
   - Version information
   - Workspace status report

## CLI Flags for Testing

When using with DevContainer CLI, you can control lifecycle execution:

```bash
# Skip post-creation commands (useful for quick testing)
devcontainer up --skip-post-create

# Skip non-blocking commands (postStart, postAttach)
devcontainer up --skip-non-blocking-commands

# Remove existing container first
devcontainer up --remove-existing-container
```

## Troubleshooting

- **Lifecycle Logs**: Check `/tmp/lifecycle-logs/` for detailed execution logs
- **Command Failures**: Named commands help identify which specific step failed
- **Environment Issues**: Verify containerEnv variables are properly substituted
- **Mount Problems**: Check volume mount permissions and paths