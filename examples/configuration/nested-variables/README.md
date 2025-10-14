# Nested Variable Substitution Example

## What This Demonstrates

This example showcases advanced variable substitution patterns in DevContainers, including:

- **Nested substitution**: Variables that reference other variables (`${containerEnv:WORKSPACE_ROOT}/project`)
- **Chained evaluation**: Variables that build on previous variables (VAR_1 → VAR_2 → VAR_3)
- **Cross-context references**: Variables that span different contexts (localWorkspaceFolder, containerEnv, remoteEnv)
- **Phased evaluation**: Understanding when different variable types are resolved
- **Unresolved placeholders**: Handling missing environment variables gracefully
- **Strict vs non-strict modes**: Different error handling behaviors

## Why This Matters

Advanced variable substitution is crucial for:
- **Dynamic path construction**: Building complex paths from simpler components
- **Configuration reusability**: Define base paths once, derive others from them
- **Environment adaptation**: Configurations that adjust to different host environments
- **Debugging and validation**: Understanding substitution order and detecting issues early
- **Security**: Proper handling of sensitive variables from the environment

## DevContainer Specification References

This example implements aspects from the [DevContainer Specification](https://containers.dev/implementors/spec/):

- **[Variable Substitution](https://containers.dev/implementors/spec/#variables-in-devcontainer-json)**: Complete variable substitution syntax
- **[Built-in Variables](https://containers.dev/implementors/spec/#variables)**: `localWorkspaceFolder`, `containerEnv`, `remoteEnv`, `localEnv`
- **[Substitution Phases](https://containers.dev/implementors/spec/#variable-substitution)**: When different variable types are resolved
- **[Environment Variables](https://containers.dev/implementors/spec/#environment-variables)**: Accessing host and container environment

## Variable Patterns Demonstrated

### 1. Simple Built-in Variables
```json
"WORKSPACE_ROOT": "${localWorkspaceFolder}"
```
Direct reference to a built-in variable.

### 2. Nested Variable References
```json
"PROJECT_DIR": "${containerEnv:WORKSPACE_ROOT}/project"
```
References another variable (`WORKSPACE_ROOT`) and appends a path.

### 3. Deep Nesting
```json
"LOG_PATH": "${containerEnv:PROJECT_DIR}/logs"
```
References `PROJECT_DIR` which itself references `WORKSPACE_ROOT` - requires multi-pass evaluation.

### 4. Chained Variables
```json
"CHAIN_VAR_1": "${localWorkspaceFolder}",
"CHAIN_VAR_2": "${containerEnv:CHAIN_VAR_1}/nested",
"CHAIN_VAR_3": "${containerEnv:CHAIN_VAR_2}/deep"
```
Demonstrates dependency chains where VAR_3 depends on VAR_2 which depends on VAR_1.

### 5. Composite Variables
```json
"NESTED_PATH": "${containerEnv:WORKSPACE_ROOT}-${localEnv:USER}/data"
```
Combines multiple variable references in a single value.

### 6. Host Environment Variables
```json
"USER_HOME": "${localEnv:HOME}"
```
Accesses host environment variables (like `$HOME`).

### 7. Missing Variables
```json
"MISSING_ENV": "${localEnv:NONEXISTENT_VARIABLE}"
```
References a variable that doesn't exist - behavior differs between strict and non-strict modes.

### 8. Remote Environment References
```json
"REMOTE_VAR": "${remoteEnv:HOME}/workspace"
```
References container environment variables (evaluated after container creation).

## Substitution Phases

Variables are evaluated in phases per the DevContainer spec:

### Phase 1: Pre-Container Creation
Resolved before the container is created:
- `${localWorkspaceFolder}` - Absolute path to workspace on host
- `${localEnv:VARIABLE}` - Host environment variables

### Phase 2: Post-Container Creation
Resolved after the container is created:
- `${containerEnv:VARIABLE}` - Container environment variables (can be nested)
- `${remoteEnv:VARIABLE}` - Remote/container environment variables
- `${containerWorkspaceFolder}` - Workspace path inside container

### Phase 3: Multi-Pass Resolution
The substitution engine performs multiple passes to resolve nested references:
1. First pass: Resolve all simple (non-nested) variables
2. Second pass: Resolve variables that reference previously resolved variables
3. Continue until no more substitutions are possible or max depth reached

## Try It

### Default (Non-Strict) Mode
By default, unresolved variables are left as-is with a warning:

```sh
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json
```

Check the substituted values:
```sh
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json | jq '.containerEnv'
```

### Strict Substitution Mode
In strict mode, unresolved variables cause an error:

```sh
cd examples/configuration/nested-variables
deacon config substitute --config devcontainer.json --strict-substitution
```

This will fail because `NONEXISTENT_VARIABLE` is not defined in the environment.

### With Dry Run
See what would be substituted without applying changes:

```sh
cd examples/configuration/nested-variables
deacon config substitute --config devcontainer.json --dry-run
```

### Inspect Specific Variables
Use `jq` to examine specific substituted values:

**Workspace-related variables**:
```sh
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json | jq '.containerEnv | {WORKSPACE_ROOT, PROJECT_DIR, LOG_PATH}'
```

**Chained variables**:
```sh
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json | jq '.containerEnv | {CHAIN_VAR_1, CHAIN_VAR_2, CHAIN_VAR_3}'
```

**Missing variable (non-strict)**:
```sh
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json | jq '.containerEnv.MISSING_ENV'
```

### Advanced: Set Maximum Depth
Control the maximum nesting depth for variable substitution:

```sh
cd examples/configuration/nested-variables
deacon config substitute --config devcontainer.json --max-depth 3
```

### Advanced: Enable/Disable Nested Substitution
Toggle nested substitution feature:

```sh
cd examples/configuration/nested-variables
deacon config substitute --config devcontainer.json --enable-nested true
```

## Expected Results

After variable substitution (assuming workspace at `/home/user/project`):

```json
{
  "WORKSPACE_ROOT": "/home/user/project",
  "PROJECT_DIR": "/home/user/project/project",
  "LOG_PATH": "/home/user/project/project/logs",
  "CONFIG_PATH": "/home/user/project/config",
  "USER_HOME": "/home/user",
  "NESTED_PATH": "/home/user/project-user/data",
  "MISSING_ENV": "${localEnv:NONEXISTENT_VARIABLE}",
  "CHAIN_VAR_1": "/home/user/project",
  "CHAIN_VAR_2": "/home/user/project/nested",
  "CHAIN_VAR_3": "/home/user/project/nested/deep"
}
```

Note: `MISSING_ENV` remains unresolved in non-strict mode.

## Strict Mode Behavior

With `--strict-substitution`, the command will fail:

```
Error: Variable substitution error

Caused by:
    Unresolved variable: ${localEnv:NONEXISTENT_VARIABLE}
```

## Key Observations

1. **Multi-pass Resolution**: Variables are resolved in multiple passes to handle nesting
2. **Dependency Order**: Variables are resolved in dependency order (dependencies first)
3. **Context Separation**: Different contexts (local, container, remote) are resolved at different times
4. **Graceful Degradation**: Non-strict mode leaves unresolved variables as placeholders
5. **Error Detection**: Strict mode catches configuration errors early
6. **Path Construction**: Complex paths can be built from simpler components

## Testing Strategies

### Test with Different Environments
Set environment variables before running:
```sh
export NONEXISTENT_VARIABLE="now-it-exists"
cd examples/configuration/nested-variables
deacon read-configuration --config devcontainer.json | jq '.containerEnv.MISSING_ENV'
```

### Test Strict Mode Validation
```sh
cd examples/configuration/nested-variables
# This should succeed
deacon config substitute --config devcontainer.json --strict-substitution 2>&1 | grep -i error
```

### Verify Nesting Depth
```sh
cd examples/configuration/nested-variables
# Test with different max depths
deacon config substitute --config devcontainer.json --max-depth 1
deacon config substitute --config devcontainer.json --max-depth 2
deacon config substitute --config devcontainer.json --max-depth 10
```

## Common Patterns

### Pattern: Base Path + Derived Paths
```json
{
  "BASE_DIR": "${localWorkspaceFolder}",
  "SRC_DIR": "${containerEnv:BASE_DIR}/src",
  "BUILD_DIR": "${containerEnv:BASE_DIR}/build",
  "DIST_DIR": "${containerEnv:BASE_DIR}/dist"
}
```

### Pattern: User-Specific Paths
```json
{
  "USER": "${localEnv:USER}",
  "USER_CACHE": "/tmp/cache-${localEnv:USER}",
  "USER_CONFIG": "${localEnv:HOME}/.config/myapp"
}
```

### Pattern: Environment-Dependent Values
```json
{
  "ENV": "${localEnv:ENVIRONMENT}",
  "API_URL": "${localEnv:API_URL}",
  "LOG_LEVEL": "${localEnv:LOG_LEVEL}"
}
```

## Notes

- **Offline-friendly**: All substitution happens locally without external dependencies
- **Order matters**: Variable definitions should appear before they are referenced (though multi-pass helps)
- **Performance**: Deep nesting may require multiple passes - use `--max-depth` to limit
- **Debugging**: Use `--dry-run` to preview substitutions without applying them
- **Security**: Never hardcode secrets - use `${localEnv:SECRET_NAME}` with secrets files
- **Current Limitation**: Nested `containerEnv` substitution (e.g., `${containerEnv:VAR1}` in `VAR2`) has partial support - simple variable references work but complex nested paths may not fully resolve in all contexts

## Spec References

Per subcommand-specs/*/SPEC.md "Variable Substitution":
- Supported variables: `localWorkspaceFolder`, `containerWorkspaceFolder`, `localEnv:*`, `containerEnv:*`, `remoteEnv:*`
- Substitution contexts: before and after container creation
- Multi-pass resolution for nested references
- Strict mode option for validation
