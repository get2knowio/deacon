# Exec Semantics Example

## What This Demonstrates

This example demonstrates the core execution semantics of the `deacon exec` command, covering:

- **Working directory parity**: Verify that exec commands run in the correct container working directory
- **User mapping**: Execute commands as different users with the `--user` flag
- **TTY behavior**: Control terminal allocation with `--no-tty` flag
- **Environment propagation**: Pass environment variables to exec commands with `--env`

## Why This Matters

Understanding exec semantics is crucial for:
- **Interactive sessions**: Running commands that require TTY (like text editors or REPLs)
- **Automation**: Executing non-interactive commands in CI/CD pipelines
- **Security**: Running commands as specific users with appropriate permissions
- **Environment consistency**: Ensuring commands have access to required environment variables

## DevContainer Specification References

This example aligns with the following specification areas:
- **[Exec Semantics](https://containers.dev/implementors/spec/#exec)**: Command execution in running containers
- **[Working Directory](https://containers.dev/implementors/spec/#workspace-folder)**: Container workspace configuration
- **[User Mapping](https://containers.dev/implementors/spec/#remote-user)**: User context for command execution

## Prerequisites

Before running these examples, ensure you have:
1. Docker installed and running
2. `deacon` CLI installed and in your PATH
3. Permission to run Docker commands

## Examples

All commands should be run from this directory (`examples/exec/semantics/`).

### 1. Working Directory Parity

Verify that exec commands run in the configured `workspaceFolder`:

```sh
# Start the container
deacon up

# Execute pwd to verify working directory
deacon exec sh -lc 'pwd'
# Expected output: /wsp
```

The command should output `/wsp`, matching the `workspaceFolder` specified in `devcontainer.json`.

### 2. User Parity

Execute commands as different users:

```sh
# Execute as root user
deacon exec --user root sh -lc 'id -u'
# Expected output: 0

# Execute as default user (non-root if specified)
deacon exec sh -lc 'id -u'
# Expected output: depends on container configuration
```

The `--user` flag allows running commands with specific user permissions, useful for operations requiring elevated privileges or testing user-specific behaviors.

### 3. TTY Behavior

Control terminal allocation for interactive vs non-interactive commands:

```sh
# Default behavior (TTY auto-detection)
deacon exec test -t 1 && echo TTY || echo NOTTY
# Output depends on your terminal

# Explicitly disable TTY
deacon exec --no-tty sh -lc 'test -t 1 && echo TTY || echo NOTTY'
# Expected output: NOTTY
```

**Note**: In CI environments or when piping output, TTY is typically not allocated by default. The `--no-tty` flag explicitly disables TTY allocation, which is useful for:
- Automated scripts that don't need interactive terminal features
- Commands whose output will be captured or piped
- Consistent behavior across different execution contexts

### 4. Environment Variable Propagation

Pass environment variables to exec commands:

```sh
# Set an environment variable
deacon exec --env FOO=BAR sh -lc 'echo $FOO'
# Expected output: BAR

# Set multiple environment variables
deacon exec --env VAR1=value1 --env VAR2=value2 sh -lc 'echo $VAR1 $VAR2'
# Expected output: value1 value2
```

Environment variables passed via `--env` are available to the executed command, enabling configuration of tools and scripts at runtime.

## Cleanup

After exploring the examples, clean up the container:

```sh
deacon down
```

## Notes

- **TTY Detection**: The default TTY behavior is auto-detected based on whether stdin is a terminal. In CI environments, TTY is typically not available.
- **User Context**: Without `--user`, commands run as the user specified by `remoteUser` in the configuration, or the default container user.
- **Working Directory**: If `workspaceFolder` is not specified in `devcontainer.json`, the working directory defaults to `/workspaces/<workspace-name>`.
- **Environment Merging**: Variables set with `--env` are merged with `containerEnv` and `remoteEnv` from the configuration, with `--env` taking precedence.

## Related Examples

- **Container Lifecycle**: See `examples/container-lifecycle/` for lifecycle command execution
- **Configuration**: See `examples/configuration/` for devcontainer.json structure
- **Environment Variables**: See `examples/configuration/with-variables/` for variable substitution

## Troubleshooting

### Container Not Found

If you get a "container not found" error, ensure the container is running:

```sh
deacon up
```

### Permission Denied

If you encounter permission errors, try running the command as root:

```sh
deacon exec --user root <command>
```

### TTY Allocation Issues

If you experience issues with terminal allocation:
- Use `--no-tty` for non-interactive commands
- Ensure your terminal supports TTY (not applicable in pure CI environments)
