# Exec Subcommand Examples

This directory contains comprehensive examples demonstrating the `deacon exec` subcommand capabilities. Each example is self-contained with its own devcontainer configuration and test scripts.

## Overview

The `exec` subcommand executes commands inside an existing dev container, applying devcontainer.json semantics (remoteUser, remoteEnv, userEnvProbe, workspace mapping) so commands run as if inside the configured development environment.

## Examples Index

### Container Targeting

#### [container-id-targeting](./container-id-targeting/)
Direct container targeting using `--container-id` flag. Most explicit targeting method, useful when container ID is known from external sources or Docker listings.

**Key concepts**: Direct ID targeting, bypassing discovery

#### [id-label-targeting](./id-label-targeting/)
Container targeting using `--id-label` flag with container labels. Enables semantic targeting using `devcontainer.local_folder` or custom labels.

**Key concepts**: Label-based targeting, multiple label matching

#### [workspace-folder-discovery](./workspace-folder-discovery/)
Automatic container discovery using `--workspace-folder` flag. Most user-friendly method that finds containers by workspace path.

**Key concepts**: Automatic discovery, config resolution, workspace mapping

### Environment Configuration

#### [remote-env-variables](./remote-env-variables/)
Environment variable management using `--remote-env` flag and configuration. Demonstrates merge order and precedence.

**Key concepts**: Environment merge order (shell → config → CLI), variable overrides, empty values

#### [user-env-probe-modes](./user-env-probe-modes/)
Different `userEnvProbe` modes controlling shell initialization. Shows how environment variables are collected from shell startup files.

**Key concepts**: loginInteractiveShell, interactiveShell, loginShell, none modes, PATH initialization

### Execution Modes

#### [interactive-pty](./interactive-pty/)
Interactive command execution with PTY (pseudo-terminal) allocation. Required for shells, REPLs, and programs needing terminal capabilities.

**Key concepts**: PTY allocation, terminal dimensions, interactive input, ANSI control codes

#### [non-interactive-streaming](./non-interactive-streaming/)
Non-PTY mode with separate stdout/stderr streams. Essential for automation, CI/CD, and binary-safe I/O.

**Key concepts**: Stream separation, binary safety, output redirection, automation patterns

### User Context

#### [remote-user-execution](./remote-user-execution/)
Command execution as configured `remoteUser`. Demonstrates how user identity affects permissions and environment.

**Key concepts**: User identity, file ownership, permissions, home directory

#### [exit-code-handling](./exit-code-handling/)
Exit code propagation and signal mapping. Shows how success/failure and signal termination are reported.

**Key concepts**: Exit codes, POSIX signal mapping (128+N), error handling

### Configuration Semantics

#### [semantics](./semantics/)
Core devcontainer.json semantics applied during exec: remoteUser, remoteEnv, userEnvProbe, workspace folder mapping.

**Key concepts**: Configuration merging, image metadata, variable substitution

## Quick Start

Each example follows this pattern:

1. **Start the dev container**:
   ```bash
   cd <example-directory>
   deacon up --workspace-folder .
   ```

2. **Run the example commands** (see each example's README.md)

3. **Clean up** (optional):
   ```bash
   docker stop $(docker ps -q --filter "label=devcontainer.local_folder=$(pwd)")
   ```

## Common Usage Patterns

### Development Workflow
```bash
# Run tests in container
deacon exec --workspace-folder . npm test

# Open interactive shell
deacon exec --workspace-folder . bash

# Execute build script
deacon exec --workspace-folder . ./build.sh
```

### CI/CD Integration
```bash
# Run with exit code checking
deacon exec --workspace-folder . pytest tests/
if [ $? -ne 0 ]; then
  echo "Tests failed"
  exit 1
fi
```

### Debugging
```bash
# Inspect environment
deacon exec --workspace-folder . env

# Check user context
deacon exec --workspace-folder . id

# Verify PATH
deacon exec --workspace-folder . bash -c 'echo $PATH'
```

## Example Categories

| Category | Examples | Use Case |
|----------|----------|----------|
| **Targeting** | container-id, id-label, workspace-folder | How to select which container to execute in |
| **Environment** | remote-env, user-env-probe | Configuring execution environment variables |
| **I/O Modes** | interactive-pty, non-interactive | Terminal vs automation usage |
| **Context** | remote-user, exit-codes | User identity and error handling |
| **Semantics** | semantics | Core devcontainer configuration behavior |

## Prerequisites

All examples require:
- Docker or compatible container runtime
- `deacon` CLI installed
- Basic understanding of containers and dev containers

Some examples additionally require:
- Terminal with TTY support (interactive examples)
- Multiple terminal windows (signal testing)
- Understanding of Unix permissions (user examples)

## Learning Path

**Recommended order for new users**:

1. Start with **workspace-folder-discovery** - easiest targeting method
2. Try **remote-env-variables** - understand environment configuration
3. Explore **interactive-pty** and **non-interactive-streaming** - different I/O modes
4. Review **exit-code-handling** - essential for automation
5. Study **semantics** - comprehensive configuration behavior

**For CI/CD users**:

1. **workspace-folder-discovery** or **id-label-targeting** - reliable targeting
2. **non-interactive-streaming** - proper stream handling
3. **exit-code-handling** - error detection
4. **remote-env-variables** - environment customization

**For interactive development**:

1. **workspace-folder-discovery** - quick container access
2. **interactive-pty** - shell and REPL usage
3. **user-env-probe-modes** - optimizing environment initialization
4. **remote-user-execution** - understanding user context

## Additional Resources

- [Exec Subcommand Specification](../../docs/subcommand-specs/exec/SPEC.md)
- [Dev Container Specification](https://containers.dev/implementors/spec)
- Main README: [../../README.md](../../README.md)

## Contributing

When adding new exec examples:

1. Create a new subdirectory under `examples/exec/`
2. Include a comprehensive README.md explaining the concept
3. Provide a self-contained devcontainer.json
4. Add test scripts demonstrating the feature
5. Update this index with a brief description
6. Follow the existing example structure and style
