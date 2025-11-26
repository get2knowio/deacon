# Up Subcommand Examples

This directory contains comprehensive examples demonstrating all features of the `deacon up` subcommand, which provisions development containers from devcontainer configurations.

## Quick Start

The simplest up command:

```bash
cd basic-image/
deacon up --workspace-folder .
```

## Example Categories

### Basic Container Creation

| Example | Description | Key Features |
|---------|-------------|--------------|
| [basic-image/](basic-image/) | Simple image-based container | Base image, workspace mount, basic lifecycle |
| [dockerfile-build/](dockerfile-build/) | Build from Dockerfile | Custom Dockerfile, build options, non-root user |
| [with-features/](with-features/) | Dev Container Features | Feature installation, additional features flag |

### Docker Compose

| Example | Description | Key Features |
|---------|-------------|--------------|
| [compose-basic/](compose-basic/) | Multi-service setup | Compose file, service selection, volume management |
| [compose-profiles/](compose-profiles/) | Conditional services | Profiles, project name, environment-specific services |

### Lifecycle & Build

| Example | Description | Key Features |
|---------|-------------|--------------|
| [lifecycle-hooks/](lifecycle-hooks/) | All lifecycle commands | onCreate, updateContent, postCreate, postStart, postAttach |
| [prebuild-mode/](prebuild-mode/) | Prebuild workflow | `--prebuild` flag, CI/CD optimization, image commits |
| [skip-lifecycle/](skip-lifecycle/) | Skip lifecycle commands | `--skip-post-create`, `--skip-post-attach` flags |

### Configuration & Customization

| Example | Description | Key Features |
|---------|-------------|--------------|
| [dotfiles-integration/](dotfiles-integration/) | Personal dotfiles | Dotfiles repository, install command, target path |
| [additional-mounts/](additional-mounts/) | Custom mounts | Bind mounts, volumes, external volumes, mount consistency |
| [remote-env-secrets/](remote-env-secrets/) | Environment & secrets | Remote env, secrets files, variable substitution |
| [configuration-output/](configuration-output/) | Config inspection | `--include-configuration`, `--include-merged-configuration` |

### Container Management

| Example | Description | Key Features |
|---------|-------------|--------------|
| [id-labels-reconnect/](id-labels-reconnect/) | Container reconnection | ID labels, container discovery, `--expect-existing-container` |
| [remove-existing/](remove-existing/) | Container replacement | `--remove-existing-container`, force recreation |

### GPU & Hardware

| Example | Description | Key Features |
|---------|-------------|--------------|
| [gpu-modes/](gpu-modes/) | GPU mode handling | `--gpu-mode` flag, auto-detection, GPU resource requests |

## Common Usage Patterns

### 1. First-Time Setup

```bash
# Navigate to project
cd your-project/

# Start dev container
deacon up --workspace-folder .
```

### 2. Development Workflow

```bash
# Start with custom mounts and environment
deacon up --workspace-folder . \
  --mount "type=volume,source=node-modules,target=/workspace/node_modules" \
  --remote-env "NODE_ENV=development"
```

### 3. CI/CD Pipeline

```bash
# Prebuild image
deacon up --workspace-folder . --prebuild
CONTAINER_ID=$(docker ps -lq)
docker commit $CONTAINER_ID myapp:latest

# Use prebuilt image in tests
docker run myapp:latest npm test
```

### 4. Team Collaboration

```bash
# Use shared dotfiles and secrets
deacon up --workspace-folder . \
  --dotfiles-repository https://github.com/your-team/dotfiles \
  --secrets-file team-secrets.env
```

### 5. Multi-Environment Setup

```bash
# Development
deacon up --workspace-folder . --id-label "env=dev"

# Staging
deacon up --workspace-folder . --id-label "env=staging"

# Testing
deacon up --workspace-folder . --id-label "env=test"
```

### 6. GPU-Accelerated Workloads

```bash
# Guarantee GPU access (requires GPU host)
deacon up --workspace-folder . --gpu-mode all

# Auto-detect with fallback
deacon up --workspace-folder . --gpu-mode detect

# Explicit CPU-only (default)
deacon up --workspace-folder . --gpu-mode none
```

## Flag Reference

### Essential Flags

- `--workspace-folder <path>`: Project workspace directory
- `--config <path>`: Path to devcontainer.json
- `--id-label <name=value>`: Custom container labels (repeatable)

### Build Flags

- `--build-no-cache`: Force clean build
- `--buildkit <auto|never>`: BuildKit usage
- `--cache-from <ref>`: Build cache source (repeatable)
- `--cache-to <ref>`: Build cache destination

### Lifecycle Control

- `--skip-post-create`: Skip all post-create lifecycle commands
- `--skip-post-attach`: Skip post-attach command only
- `--skip-non-blocking-commands`: Skip background tasks
- `--prebuild`: Stop after onCreate and updateContent

### Container Management

- `--remove-existing-container`: Remove and recreate container
- `--expect-existing-container`: Fail if container doesn't exist
- `--workspace-mount-consistency <consistent|cached|delegated>`: Mount sync behavior

### GPU & Hardware

- `--gpu-mode <all|detect|none>`: GPU resource handling (default: none)

### Customization

- `--mount "type=...,source=...,target=..."`: Additional mounts (repeatable)
- `--remote-env "NAME=value"`: Environment variables (repeatable)
- `--additional-features <json>`: Add features at runtime
- `--dotfiles-repository <url>`: Dotfiles repository URL
- `--dotfiles-install-command <cmd>`: Custom install script
- `--dotfiles-target-path <path>`: Dotfiles install location

### Output Control

- `--include-configuration`: Include base configuration in output
- `--include-merged-configuration`: Include merged configuration in output
- `--log-level <info|debug|trace>`: Logging verbosity
- `--log-format <text|json>`: Log output format

## Output Format

All `up` commands return JSON on stdout:

### Success

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "<username>",
  "remoteWorkspaceFolder": "<path>",
  "composeProjectName": "<project>" // Only for Compose
}
```

### With Configuration

```json
{
  "outcome": "success",
  "containerId": "<container-id>",
  "remoteUser": "<username>",
  "remoteWorkspaceFolder": "<path>",
  "configuration": { ... },           // With --include-configuration
  "mergedConfiguration": { ... }      // With --include-merged-configuration
}
```

### Error

```json
{
  "outcome": "error",
  "message": "<error-message>",
  "description": "<detailed-description>"
}
```

Exit code: 0 for success, 1 for error.

## Feature Matrix

| Feature | Examples Demonstrating |
|---------|----------------------|
| Image-based containers | basic-image, with-features |
| Dockerfile builds | dockerfile-build |
| Dev Container Features | with-features, prebuild-mode |
| Docker Compose | compose-basic, compose-profiles |
| Lifecycle hooks | lifecycle-hooks, prebuild-mode, skip-lifecycle |
| Dotfiles | dotfiles-integration |
| Custom mounts | additional-mounts, compose-basic |
| Environment variables | remote-env-secrets, lifecycle-hooks |
| Secrets | remote-env-secrets |
| Container reconnection | id-labels-reconnect, remove-existing |
| Configuration output | configuration-output |
| Prebuild workflow | prebuild-mode |
| BuildKit | dockerfile-build |
| Compose profiles | compose-profiles |
| GPU modes | gpu-modes |

## Testing Examples

Each example includes a README with:
- Overview and purpose
- Configuration details
- Usage commands
- Expected output
- Testing/verification steps
- Cleanup instructions
- Related examples

To test an example:

```bash
cd <example-directory>/
deacon up --workspace-folder .
# Follow testing steps in README.md
docker rm -f $(docker ps -lq)  # Cleanup
```

## Troubleshooting

### Container Already Exists

```bash
# Reconnect to existing
deacon up --workspace-folder .

# Or force recreate
deacon up --workspace-folder . --remove-existing-container
```

### Build Failures

```bash
# Clean build
deacon up --workspace-folder . --build-no-cache --remove-existing-container
```

### Missing Configuration

```bash
# Explicit config path
deacon up --workspace-folder . --config .devcontainer/devcontainer.json
```

### Debug Output

```bash
# Verbose logging
deacon up --workspace-folder . --log-level debug

# JSON logging
deacon up --workspace-folder . --log-format json
```

## Specification Reference

For complete details on the `up` subcommand:
- Specification: `docs/subcommand-specs/up/SPEC.md`
- Gap Analysis: `specs/001-up-gap-spec/spec.md`

## Contributing

When adding new examples:
1. Create self-contained directory with `.devcontainer/devcontainer.json`
2. Include comprehensive README.md
3. Add example files needed to demonstrate the feature
4. Update this main README.md
5. Test the example end-to-end

## Additional Resources

- [Dev Containers Specification](https://containers.dev/)
- [Features Reference](https://containers.dev/features)
- [Lifecycle Scripts](https://containers.dev/implementors/json_reference/#lifecycle-scripts)
- [Docker Compose Integration](https://containers.dev/guide/compose)
