# Container Lifecycle Examples

This directory contains comprehensive examples demonstrating DevContainer lifecycle command execution, timing, and best practices.

## Overview

Container lifecycle commands run at specific points during container creation and startup, allowing you to automate development environment setup. These examples show the proper usage, execution order, and advanced patterns for lifecycle commands.

## Examples

### [Basic Lifecycle](./basic/)
**What it demonstrates:** Fundamental lifecycle command execution in the correct order (onCreate → postCreate → postStart → postAttach).
- Simple array-based commands
- Logging and output capture
- Basic error handling
- Demonstrates the four main lifecycle phases

### [Advanced Lifecycle](./advanced/) 
**What it demonstrates:** Production-ready lifecycle patterns with robust error handling and environment setup.
- Named command objects for better organization
- Volume mounts for performance
- Environment variable integration
- Real-world Node.js development setup
- Graceful fallbacks and error handling

### [Execution Order](./execution-order/)
**What it demonstrates:** Clear visualization of lifecycle command execution sequence and timing.
- Timestamps showing exact execution order
- Demonstrates when each command runs
- Frequency of execution (once vs. every time)
- Minimal example focused purely on sequence

### [Variable Substitution](./with-variables/)
**What it demonstrates:** Dynamic configuration using DevContainer variable substitution in lifecycle commands.
- Built-in variables (`${localWorkspaceFolder}`, `${containerWorkspaceFolder}`)
- Environment variable references (`${containerEnv:VARIABLE}`)
- Cross-references between containerEnv and remoteEnv
- Path construction and validation

### [Non-Blocking and Skip Flags](./non-blocking-and-skip/)
**What it demonstrates:** Skip flags to control lifecycle command execution during development iteration.
- `--skip-post-create` flag behavior
- `--skip-non-blocking-commands` flag behavior
- Combining skip flags for faster iteration
- Marker files to verify which phases executed

### [Progress Events](./progress-events/)
**What it demonstrates:** Machine-readable progress tracking for automation and monitoring.
- Per-command progress events with stable IDs
- Event ordering and timing information
- JSON structured logging with `--progress-file`
- Using `jq` to analyze events and verify behavior

### [Redaction](./redaction/)
**What it demonstrates:** Automatic secret redaction in command output and progress files.
- Default redaction of sensitive values (API_KEY, PASSWORD, SECRET, TOKEN)
- `--no-redact` flag for debugging (use with caution)
- Redaction in terminal output, progress files, and logs
- Verification strategies with `jq`

## Lifecycle Command Reference

Based on the [DevContainer specification](https://containers.dev/implementors/spec/#lifecycle-scripts) and [Up SPEC](../../docs/subcommand-specs/up/SPEC.md):

| Command | When It Runs | Frequency | Purpose |
|---------|-------------|-----------|---------|
| `initializeCommand` | Host-side, before container creation | Once | Host preparation |
| `onCreateCommand` | During initial container creation | Once | Container setup |
| `updateContentCommand` | During content synchronization | As needed | Content updates |
| `postCreateCommand` | After creation and features | Once | Project setup |
| `postStartCommand` | When container starts | Every start | Service startup |
| `postAttachCommand` | When attaching to container | Every attach | User notifications |

## Command Formats

### Array Format (Sequential)
```json
{
  "postCreateCommand": [
    "echo 'First command'",
    "echo 'Second command'", 
    "echo 'Third command'"
  ]
}
```

### Object Format (Named)
```json
{
  "postCreateCommand": {
    "install-deps": "npm install",
    "build-project": "npm run build",
    "setup-db": "npm run db:setup"
  }
}
```

### String Format (Single Command)
```json
{
  "postCreateCommand": "npm install && npm run build"
}
```

## Best Practices

### 1. Use Appropriate Lifecycle Phases
- **onCreate**: System-level setup, directory creation, tool installation
- **postCreate**: Project-specific setup, dependency installation
- **postStart**: Service startup, background processes
- **postAttach**: User notifications, status displays

### 2. Handle Errors Gracefully
```bash
"npm install || echo 'Failed to install dependencies'"
"docker --version 2>/dev/null || echo 'Docker not available'"
```

### 3. Use Named Objects for Complex Workflows
Named commands provide better organization and debugging:
```json
{
  "postCreateCommand": {
    "setup-env": "cp .env.example .env",
    "install-deps": "npm install", 
    "build": "npm run build",
    "migrate": "npm run db:migrate"
  }
}
```

### 4. Log for Debugging
Include logging to help troubleshoot issues:
```bash
"echo 'Starting setup...' | tee /tmp/setup.log"
"npm install 2>&1 | tee -a /tmp/setup.log"
```

### 5. Use Variable Substitution
Make configurations reusable across projects:
```json
{
  "containerEnv": {
    "PROJECT_ROOT": "${containerWorkspaceFolder}"
  },
  "postCreateCommand": "cd ${PROJECT_ROOT} && npm install"
}
```

## Testing Your Lifecycle Commands

### Configuration Validation
```bash
# Test configuration parsing
deacon read-configuration --config devcontainer.json

# Check lifecycle commands structure
deacon read-configuration --config devcontainer.json | jq '{
  onCreate: .onCreateCommand,
  postCreate: .postCreateCommand,
  postStart: .postStartCommand, 
  postAttach: .postAttachCommand
}'
```

### CLI Options for Development
When using DevContainer CLI, control lifecycle execution:
```bash
# Skip post-creation commands (faster iteration)
devcontainer up --skip-post-create

# Skip non-blocking commands
devcontainer up --skip-non-blocking-commands  

# Force container recreation
devcontainer up --remove-existing-container
```

## Common Patterns

### Development Environment Setup
```json
{
  "onCreateCommand": "mkdir -p {src,tests,docs}",
  "postCreateCommand": ["npm install", "npm run build"],
  "postStartCommand": "npm run dev &",
  "postAttachCommand": "echo 'Development server running on port 3000'"
}
```

### Database Setup
```json
{
  "onCreateCommand": "mkdir -p /tmp/db-data",
  "postCreateCommand": "npm run db:setup",
  "postStartCommand": "npm run db:migrate",
  "postAttachCommand": "npm run db:status"
}
```

### Multi-Language Projects  
```json
{
  "postCreateCommand": {
    "python-deps": "pip install -r requirements.txt",
    "node-deps": "npm install",
    "ruby-deps": "bundle install"
  }
}
```

## Troubleshooting

### Common Issues
1. **Commands not running**: Check JSON syntax and command format
2. **Path errors**: Verify variable substitution and absolute paths
3. **Permission issues**: Ensure proper user context in commands
4. **Service startup**: Use postStartCommand for services, not postCreateCommand
5. **Execution order**: Remember onCreate runs before features are installed

### Debugging Tips
- Add logging to commands: `echo 'Debug info' | tee /tmp/debug.log`
- Check command exit codes: `command && echo 'Success' || echo 'Failed'`
- Use named objects to identify which commands fail
- Test configurations with `deacon read-configuration` before building

## References

- [DevContainer Specification - Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)
- [Up SPEC: Core Execution Logic](../../docs/subcommand-specs/up/SPEC.md#5-core-execution-logic)
- [Variable Substitution Documentation](https://containers.dev/implementors/spec/#variables-in-devcontainer-json)