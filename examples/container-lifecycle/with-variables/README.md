# Lifecycle Commands with Variable Substitution

## What This Demonstrates

This example shows how to use DevContainer variable substitution within lifecycle commands to create dynamic, reusable development environments that adapt to different projects and workspace configurations.

## Variable Substitution Types

### Built-in Variables

DevContainer provides several built-in variables that get automatically substituted:

| Variable | Description | Example Value |
|----------|-------------|---------------|
| `${localWorkspaceFolder}` | Host workspace path | `/home/user/my-project` |
| `${localWorkspaceFolderBasename}` | Workspace folder name | `my-project` |
| `${containerWorkspaceFolder}` | Container workspace path | `/workspaces/my-project` |

### Environment Variable References

You can reference environment variables defined in the same configuration:

```json
{
  "containerEnv": {
    "PROJECT_ROOT": "${containerWorkspaceFolder}",
    "PROJECT_NAME": "${localWorkspaceFolderBasename}"
  },
  "onCreateCommand": [
    "echo 'Setting up ${PROJECT_NAME} in ${PROJECT_ROOT}'"
  ]
}
```

### Cross-Reference Variables

Reference containerEnv variables in remoteEnv and commands:

```json
{
  "containerEnv": {
    "PROJECT_ROOT": "${containerWorkspaceFolder}"
  },
  "remoteEnv": {
    "PYTHONPATH": "${containerEnv:PROJECT_ROOT}/src"
  }
}
```

## Variable Usage in Lifecycle Commands

### onCreate - Dynamic Project Structure

```bash
mkdir -p ${PROJECT_ROOT}/{src,tests,docs,bin}
echo 'PROJECT=${PROJECT_NAME}' > ${PROJECT_ROOT}/.env
```

Creates project-specific directories and configuration files using the actual project name and paths.

### postCreate - Environment Setup

```bash
cd ${PROJECT_ROOT} && python -m venv .venv
echo 'export VIRTUAL_ENV=${PROJECT_ROOT}/.venv' >> ${USER_HOME}/env.sh
```

Sets up Python virtual environment in the correct project directory.

### postStart - Path Validation

```bash
echo 'PYTHONPATH=${PYTHONPATH}' && ls -la ${PROJECT_ROOT}/
```

Validates that substituted paths exist and are accessible.

### postAttach - Status Display

```bash
echo 'Workspace: ${PROJECT_ROOT}'
echo 'To activate virtual environment: source ${PROJECT_ROOT}/.venv/bin/activate'
```

Shows project-specific information using substituted variables.

## Advanced Variable Patterns

### Conditional Commands

```bash
cd ${PROJECT_ROOT} && [ -f .env ] && cat .env || echo 'No .env file found'
```

Use shell conditionals with substituted paths for robust error handling.

### Path Construction

```bash
export PYTHONPATH="${containerEnv:PROJECT_ROOT}/src:${containerEnv:PROJECT_ROOT}/lib"
```

Build complex paths by combining multiple variable references.

### Cross-Mount References

```json
{
  "mounts": [
    "source=${localWorkspaceFolder}/.vscode,target=${containerWorkspaceFolder}/.vscode,type=bind"
  ]
}
```

Mount host directories to container locations using variable substitution.

## DevContainer Specification References

This example demonstrates:
- **[Variable Substitution](https://containers.dev/implementors/spec/#variables-in-devcontainer-json)**: Built-in and environment variables
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Container and remote environment setup
- **[Lifecycle Scripts](https://containers.dev/implementors/spec/#lifecycle-scripts)**: Using variables in command execution

## Run

Test variable substitution:
```sh
deacon read-configuration --config devcontainer.json
```

View substituted variables:
```sh
deacon read-configuration --config devcontainer.json | jq '.containerEnv, .remoteEnv'
```

Check lifecycle commands with variables:
```sh
deacon read-configuration --config devcontainer.json | jq -r '
  .onCreateCommand[],
  .postCreateCommand[],
  .postStartCommand | to_entries[] | .key + ": " + .value,
  .postAttachCommand[]
'
```

## Variable Substitution Context

### At Configuration Parse Time
These variables are substituted when the configuration is loaded:
- `${localWorkspaceFolder}`
- `${localWorkspaceFolderBasename}` 
- `${containerWorkspaceFolder}`

### At Command Execution Time
These variables are available in the shell environment when commands run:
- `${PROJECT_ROOT}` (from containerEnv)
- `${PROJECT_NAME}` (from containerEnv)
- `${PYTHONPATH}` (from remoteEnv)

## Best Practices

### 1. Consistent Naming
Use descriptive variable names that make the configuration self-documenting:
```json
{
  "containerEnv": {
    "PROJECT_ROOT": "${containerWorkspaceFolder}",
    "PROJECT_NAME": "${localWorkspaceFolderBasename}",
    "USER_HOME": "${containerWorkspaceFolder}/.devcontainer"
  }
}
```

### 2. Path Safety
Always use absolute paths and validate they exist:
```bash
"mkdir -p ${PROJECT_ROOT}/{src,tests} && cd ${PROJECT_ROOT}"
```

### 3. Environment Layering
Build complex environments by layering variables:
```json
{
  "containerEnv": {
    "PROJECT_ROOT": "${containerWorkspaceFolder}"
  },
  "remoteEnv": {
    "PATH": "${containerEnv:PATH}:${containerEnv:PROJECT_ROOT}/bin"
  }
}
```

### 4. Error Handling
Include fallbacks for missing variables or paths:
```bash
"cd ${PROJECT_ROOT} && [ -f requirements.txt ] && pip install -r requirements.txt || echo 'No requirements.txt found'"
```

## Common Variable Substitution Patterns

- **Project Structure**: `${PROJECT_ROOT}/{src,tests,docs,bin}`
- **Virtual Environments**: `${PROJECT_ROOT}/.venv`
- **Configuration Files**: `${PROJECT_ROOT}/.env`
- **Path Extension**: `${PATH}:${PROJECT_ROOT}/bin`
- **Cross-references**: `${containerEnv:PROJECT_ROOT}`