# Contract: Lifecycle Command Execution

**Feature Branch**: `012-fix-lifecycle-formats`
**Date**: 2026-02-21

## Overview

This contract defines the execution behavior for all three lifecycle command formats across both container and host execution paths.

## Format Detection

Input: `serde_json::Value` from devcontainer.json or feature metadata.

| JSON Type | Parsed As | Execution Strategy |
|-----------|-----------|-------------------|
| `null` | No-op | Skip silently |
| `""` (empty string) | No-op | Skip silently |
| `"cmd"` (non-empty string) | `Shell(cmd)` | Execute through shell |
| `[]` (empty array) | No-op | Skip silently |
| `["prog", "arg1", ...]` | `Exec(vec)` | Direct OS execution |
| `{}` (empty object) | No-op | Skip silently |
| `{"key": value, ...}` | `Parallel(map)` | Concurrent execution |
| Other (number, boolean) | Error | Fail with validation error |

## Shell Execution (String Format)

### Container Path
```
Input:  "npm install && npm build"
Docker: docker exec <container> sh -c "npm install && npm build"
```

When `use_login_shell` is true, uses detected shell (e.g., `bash -lic`).

### Host Path
```
Input:  "npm install && npm build"
Host:   sh -c "npm install && npm build"     (Unix)
        cmd /C "npm install && npm build"     (Windows)
```

## Exec-Style Execution (Array Format)

### Container Path
```
Input:  ["npm", "install", "--save-dev"]
Docker: docker exec <container> npm install --save-dev
```

No shell wrapper. Arguments passed as-is to the Docker exec API.
Shell metacharacters in arguments are NOT interpreted.

### Host Path
```
Input:  ["npm", "install", "--save-dev"]
Host:   Command::new("npm").args(["install", "--save-dev"])
```

Direct OS process invocation. No shell interpretation.

## Parallel Execution (Object Format)

### Container Path
```
Input:  {
  "install": "npm install",
  "build": ["npm", "run", "build"]
}
Execution:
  Task 1 (concurrent): docker exec <container> sh -c "npm install"
  Task 2 (concurrent): docker exec <container> npm run build
  Wait for all tasks to complete.
```

### Host Path
```
Input:  {
  "prep": "mkdir -p .cache",
  "check": ["git", "status"]
}
Execution:
  Thread 1 (concurrent): sh -c "mkdir -p .cache"
  Thread 2 (concurrent): Command::new("git").args(["status"])
  Wait for all threads to complete.
```

## Object Value Type Handling

| Value Type | Behavior |
|-----------|----------|
| String | Execute as Shell command |
| Array (all strings) | Execute as Exec command |
| Array (non-string elements) | Validation error for the entry |
| Null | Skip entry (no-op) |
| Empty string | Skip entry (no-op) |
| Empty array | Skip entry (no-op) |
| Other (number, boolean, nested object) | Skip with diagnostic log |

## Error Behavior

### Sequential Commands (String/Array)
- Fail-fast: stop execution on first failure
- Report source attribution (feature ID or "config")

### Parallel Commands (Object)
- Wait for ALL commands to complete (no cancellation)
- If ANY command fails, phase reports failure
- Error message includes all failed command keys and exit codes
- Non-failed commands are NOT cancelled

## Progress Events

### Shell/Exec Commands
```
LifecycleCommandBegin { phase, command_id: "{phase}-{index}" }
LifecycleCommandEnd   { phase, command_id: "{phase}-{index}", exit_code, success }
```

### Parallel Commands
```
LifecyclePhaseBegin   { phase, commands: [key1, key2, ...] }
LifecycleCommandBegin { phase, command_id: "{phase}-{key}" }
LifecycleCommandEnd   { phase, command_id: "{phase}-{key}", exit_code, success }
LifecyclePhaseEnd     { phase, success }
```

## Variable Substitution

- **Shell**: Substitute the entire command string
- **Exec**: Substitute each element independently
- **Parallel**: Substitute each value's string/array elements recursively

Variables like `${containerWorkspaceFolder}`, `${localWorkspaceFolder}`, and `${containerEnv:VAR}` are resolved before execution.

## Lifecycle Phases

All six phases support all three formats:

| Phase | Execution Context | Notes |
|-------|------------------|-------|
| `initializeCommand` | Host | Runs before container creation |
| `onCreateCommand` | Container | Blocking, runs during creation |
| `updateContentCommand` | Container | Blocking, content sync |
| `postCreateCommand` | Container | Blocking, post-creation setup |
| `postStartCommand` | Container | Non-blocking (deferred) |
| `postAttachCommand` | Container | Non-blocking (deferred) |

## Exit Codes

| Scenario | Exit Behavior |
|----------|--------------|
| Shell command exits non-zero | Phase fails with exit code |
| Exec command exits non-zero | Phase fails with exit code |
| Any parallel entry exits non-zero | Phase fails after all complete |
| Exec program not found | Phase fails with exec error |
| Invalid array element type | Validation error before execution |
