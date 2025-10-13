# Exec Subcommand Implementation Gap Analysis

**Date**: October 13, 2025  
**Specification Version**: Based on `SPEC.md`, `DATA-STRUCTURES.md`, `DIAGRAMS.md` in `/workspaces/deacon/docs/subcommand-specs/exec/`  
**Implementation Version**: Current state in `/workspaces/deacon/crates/deacon/src/commands/exec.rs`

## Executive Summary

The current `exec` subcommand implementation provides basic command execution functionality but has **significant gaps** compared to the specification. While core execution works, many flags and features from the spec are missing or incorrectly implemented.

**Overall Compliance: ~50%**

### Critical Missing Features
1. Missing container/config resolution flags (`--container-id`, `--config`, `--override-config`)
2. Missing `--workspace-folder` flag (uses implicit current directory)
3. Missing environment probe system (`--default-user-env-probe`, `userEnvProbe`)
4. Missing `--remote-env` flag (has `--env` but semantics differ)
5. Missing Docker/tooling flags (`--docker-path`, `--docker-compose-path`)
6. Missing data folder flags
7. Missing terminal dimension flags
8. No configuration merging with image metadata
9. No variable substitution with container environment
10. Exit code mapping for signals incomplete

---

## 1. CLI Interface Gaps

### Specification Requirements
```
devcontainer exec [options] <cmd> [args..]

Container selection and config:
  --workspace-folder <PATH>
  --container-id <ID>
  --id-label <name=value>          (repeatable)
  --config <PATH>
  --override-config <PATH>
  --mount-workspace-git-root       (boolean, default: true)

Execution environment:
  --remote-env <name=value>        (repeatable, value may be empty)
  --default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}

Docker tooling and data folders:
  --docker-path <PATH>
  --docker-compose-path <PATH>
  --user-data-folder <PATH>
  --container-data-folder <PATH>
  --container-system-data-folder <PATH>

Logging and terminal:
  --log-level {info|debug|trace}
  --log-format {text|json}
  --terminal-columns <N>
  --terminal-rows <N>

Hidden/testing:
  --skip-feature-auto-mapping      (hidden boolean)
```

### Current Implementation
```rust
Exec {
    user: Option<String>,               // --user
    no_tty: bool,                       // --no-tty
    env: Vec<String>,                   // --env
    workdir: Option<String>,            // -w, --workdir
    id_label: Vec<String>,              // --id-label
    service: Option<String>,            // --service (extension)
    command: Vec<String>,               // positional
}

// Plus global flags:
// --workspace-folder, --config, --config-override, --secrets-file,
// --log-level, --log-format, --user-data-folder
```

### Gaps

| Feature | Spec | Implementation | Status | Severity |
|---------|------|----------------|--------|----------|
| `--workspace-folder` | Optional | Global flag | ✅ Present (different level) | **LOW** |
| `--container-id` | Optional | ❌ Not present | ❌ Missing | **CRITICAL** |
| `--id-label` | Repeatable | ✅ Present | ✅ Correct | **PASS** |
| `--config` | Optional | Global flag | ✅ Present (different level) | **LOW** |
| `--override-config` | Optional | Global flag (`--config-override`) | ✅ Present (different level) | **LOW** |
| `--mount-workspace-git-root` | Boolean, default true | ❌ Not present | ❌ Missing | **MEDIUM** |
| `--remote-env` | Repeatable | `--env` (different semantics) | ⚠️ Partial | **HIGH** |
| `--default-user-env-probe` | Enum | ❌ Not present | ❌ Missing | **CRITICAL** |
| `--docker-path` | Optional | ❌ Not present | ❌ Missing | **MEDIUM** |
| `--docker-compose-path` | Optional | ❌ Not present | ❌ Missing | **MEDIUM** |
| `--user-data-folder` | Optional | Global flag | ✅ Present (different level) | **LOW** |
| `--container-data-folder` | Optional | ❌ Not present | ❌ Missing | **MEDIUM** |
| `--container-system-data-folder` | Optional | ❌ Not present | ❌ Missing | **MEDIUM** |
| `--terminal-columns` | Paired with rows | ❌ Not present | ❌ Missing | **LOW** |
| `--terminal-rows` | Paired with columns | ❌ Not present | ❌ Missing | **LOW** |
| `--skip-feature-auto-mapping` | Hidden testing | ❌ Not present | ❌ Missing | **LOW** |
| `--user` | ⚠️ Not in spec | ✅ Present | ⚠️ Extension | **LOW** |
| `--no-tty` | ⚠️ Not in spec | ✅ Present | ⚠️ Extension | **LOW** |
| `-w`, `--workdir` | ⚠️ Not in spec | ✅ Present | ⚠️ Extension | **LOW** |
| `--service` | ⚠️ Not in spec | ✅ Present | ⚠️ Extension | **LOW** |

**Impact**: Missing critical flags prevent proper container selection and environment configuration.

---

## 2. Argument Validation Gaps

### Specification Requirements

```pseudocode
// Validation rules:
- id_label must match /.+=.+/ (name=value with non-empty value)
- remote_env must match /.+=.*/ (name=value, value may be empty)
- One of --container-id, --id-label, or --workspace-folder required
- terminal-columns requires terminal-rows and vice versa
- cmd (positional) is required
```

### Current Implementation

```rust
// In execute_exec_with_docker:
if args.command.is_empty() {
    return Err(anyhow::anyhow!("No command specified for exec"));
}

// id_label validation:
for label in id_labels {
    if !label.contains('=') {
        return Err(anyhow::anyhow!(
            "Invalid id-label format: '{}'. Expected KEY=VALUE",
            label
        ));
    }
}

// env validation:
for env_var in &args.env {
    if let Some((key, value)) = env_var.split_once('=') {
        env_map.insert(key.to_string(), value.to_string());
    } else {
        return Err(anyhow::anyhow!(
            "Invalid environment variable format: '{}'. Expected KEY=VALUE",
            env_var
        ));
    }
}
```

### Gaps

| Validation | Spec | Implementation | Status |
|------------|------|----------------|--------|
| `id-label` format (`name=value`) | ✅ Required | ✅ Validated | **PASS** |
| `id-label` non-empty value | ✅ Required | ⚠️ Only checks for `=` | **PARTIAL** |
| `remote-env` format | ✅ `name=value` | ✅ `env` validated as `KEY=VALUE` | **PASS** |
| `remote-env` allows empty value | ✅ Allowed | ❌ Not supported | **FAIL** |
| One of container selection flags required | ✅ Required | ⚠️ Implicitly via config discovery | **PARTIAL** |
| Terminal dimensions paired | ✅ Required | ❌ Flags don't exist | **N/A** |
| Command required | ✅ Required | ✅ Validated | **PASS** |

**Impact**: 
- Cannot pass empty environment values (e.g., `--remote-env FOO=`)
- `id-label` doesn't validate that value is non-empty (spec says `/.+=.+/`)

---

## 3. Configuration Resolution Gaps

### Specification Requirements

**Configuration Sources (precedence order)**:
1. Command-line flags
2. Environment variables (for substitution)
3. Configuration files (devcontainer.json + overrides)
4. Default values

**Discovery and Selection**:
- If `--config` provided, use it
- Else if `--workspace-folder` provided, discover under it
- If config flags provided but config not found → error
- Target container via `--container-id` or inferred id-labels

**Merge Algorithm**:
- Read devcontainer metadata labels from container → `imageMetadata`
- Merge `config` with `imageMetadata` → `mergedConfig`
- Apply container-aware substitution: `containerSubstitute(platform, configPath, containerEnv, mergedConfig)`

### Current Implementation

```rust
// Container resolution:
let container_id = if !args.id_label.is_empty() {
    resolve_target_container_by_labels(docker_client, &args.id_label).await?
} else {
    // Load config and use ContainerIdentity labels
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));
    let config = if let Some(config_path) = args.config_path.as_ref() {
        ConfigLoader::load_from_path(config_path)?
    } else {
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        ConfigLoader::load_from_path(config_location.path())?
    };
    resolve_target_container(docker_client, workspace_folder, &config, args.service.as_deref()).await?
};
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| `--container-id` flag for explicit container ID | ✅ Required | ❌ Missing | **FAIL** |
| Config discovery from `--workspace-folder` | ✅ Required | ✅ Implemented | **PASS** |
| Error when config flags provided but not found | ✅ Required | ✅ Implemented | **PASS** |
| Read image metadata labels from container | ✅ Required | ❌ Not implemented | **FAIL** |
| Merge config with image metadata | ✅ Required | ❌ Not implemented | **FAIL** |
| Container-aware variable substitution | ✅ Required | ❌ Not implemented | **FAIL** |
| Support `--override-config` | ✅ Optional | ✅ Global flag exists | **PARTIAL** |

**Impact**: 
- Cannot target arbitrary container by ID
- Config is not merged with image metadata labels
- No variable substitution with container environment
- Missing `remoteUser`, `remoteEnv` resolution from merged config

---

## 4. Core Execution Logic Gaps

### Specification Pseudocode

```pseudocode
// Phase 3: Main execution
image_metadata = get_image_metadata_from_container(container, config, features, id_labels)
merged = merge_configuration(config.config, image_metadata)
props = create_container_properties(docker, container.Id, configs?.workspaceConfig.workspaceFolder, merged.remoteUser)
updated = container_substitute(cli_host.platform, config.config.configFilePath, props.env, merged)
remote_env_base = probe_remote_env(cli_host, props, updated)   // userEnvProbe with caching
remote_env = merge(remote_env_base, kv_list_to_map(input.remote_env_kv), updated.remoteEnv)
remote_cwd = props.remoteWorkspaceFolder OR props.homeFolder
```

### Current Implementation

```rust
// Determine working dir
let working_dir = if let Some(ref cli_workdir) = args.workdir {
    cli_workdir.clone()
} else if !args.id_label.is_empty() {
    String::from("/")
} else {
    // Load config to get workspace folder
    determine_container_working_dir(&config, workspace_folder)
};

// Create exec config
let exec_config = ExecConfig {
    user: args.user.clone(),
    working_dir: Some(working_dir.clone()),
    env: env_map,  // Only from --env flag
    tty: should_use_tty,
    interactive: true,
    detach: false,
    silent: false,
};

docker_client.exec(&container_id, &args.command, exec_config).await
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Get image metadata from container | ✅ Required | ❌ Not implemented | **FAIL** |
| Merge config with image metadata | ✅ Required | ❌ Not implemented | **FAIL** |
| Resolve `remoteUser` from merged config | ✅ Required | ⚠️ Uses `--user` flag only | **PARTIAL** |
| Container-aware variable substitution | ✅ Required | ❌ Not implemented | **FAIL** |
| Probe remote environment (`userEnvProbe`) | ✅ Required | ❌ Not implemented | **FAIL** |
| Merge environments (shell → CLI → config) | ✅ Required | ⚠️ Only CLI env | **PARTIAL** |
| Use `remoteWorkspaceFolder` or `homeFolder` as CWD | ✅ Required | ⚠️ Uses config or `/` | **PARTIAL** |
| Support `--default-user-env-probe` | ✅ Required | ❌ Not implemented | **FAIL** |
| Cache userEnvProbe results | ✅ Optional | ❌ Not implemented | **FAIL** |

**Impact**:
- Environment variables from config (`remoteEnv`) not applied
- Shell environment from user profile not loaded
- `remoteUser` not resolved from config
- No PATH merging or environment probe
- Working directory doesn't properly use `remoteWorkspaceFolder`

---

## 5. Environment Handling Gaps

### Specification Requirements

**Environment Merge Order**:
1. `shell_env` from `userEnvProbe` (runs shell to collect environment)
2. CLI `--remote-env` additions
3. Config `remoteEnv` from merged configuration

**User Environment Probe**:
```pseudocode
probe_remote_env(cli_host, props, updated):
  probe_mode = updated.userEnvProbe OR input.default_user_env_probe OR 'loginInteractiveShell'
  // Run shell command to collect environment based on probe mode
  // Cache results if session cache available
  // Merge with PATH from config if needed
```

### Current Implementation

```rust
// Only CLI --env flag:
let mut env_map = HashMap::new();
for env_var in &args.env {
    if let Some((key, value)) = env_var.split_once('=') {
        env_map.insert(key.to_string(), value.to_string());
    }
}

let exec_config = ExecConfig {
    // ...
    env: env_map,  // Only CLI env
    // ...
};
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Shell environment probe | ✅ Required | ❌ Not implemented | **FAIL** |
| `userEnvProbe` modes support | ✅ 4 modes | ❌ Not implemented | **FAIL** |
| `--default-user-env-probe` flag | ✅ Required | ❌ Missing | **FAIL** |
| Config `remoteEnv` merging | ✅ Required | ❌ Not implemented | **FAIL** |
| Environment merge order | ✅ Specified | ❌ Only CLI env | **FAIL** |
| Allow empty environment values | ✅ Required | ❌ Not supported | **FAIL** |
| Cache probe results | ✅ Optional | ❌ Not implemented | **FAIL** |

**Impact**:
- User shell initialization scripts not run (no `.bashrc`, `.profile`, etc.)
- PATH and other environment variables not properly set
- Cannot configure environment probe behavior
- Config-defined `remoteEnv` ignored

---

## 6. Exit Code Handling Gaps

### Specification Requirements

```pseudocode
Exit Code Mapping:
- If remote returns numeric code: use it
- Else if terminated by signal: return 128 + signal (POSIX convention)
- Else: return 1
```

### Current Implementation

```rust
match docker_client.exec(&container_id, &args.command, exec_config).await {
    Ok(result) => {
        tracing::info!("Command completed with exit code: {}", result.exit_code);
        std::process::exit(result.exit_code);  // Direct exit with code
    }
    Err(e) => {
        tracing::error!("Failed to execute command: {}", e);
        Err(e.into())  // Error propagation
    }
}
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Return numeric exit code | ✅ Required | ✅ Implemented | **PASS** |
| Map signal to `128 + signal` | ✅ Required | ❌ Not implemented | **FAIL** |
| Default to `1` on failure | ✅ Required | ⚠️ Implicit | **PARTIAL** |

**Impact**:
- Signal terminations not properly mapped to exit codes
- Cannot distinguish between different signal types

---

## 7. PTY/TTY Handling Gaps

### Specification Requirements

```pseudocode
PTY Selection Heuristic:
- Use PTY when both stdin and stdout are TTYs
- OR when --log-format json is requested
- PTY merges stdout/stderr
- Non-PTY keeps separate streams and is better for binary data
```

### Current Implementation

```rust
// Determine TTY allocation
let should_use_tty = !args.no_tty && CliDocker::is_tty();

let exec_config = ExecConfig {
    // ...
    tty: should_use_tty,
    interactive: true,  // Always interactive
    // ...
};
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| TTY detection (stdin & stdout) | ✅ Required | ✅ Implemented | **PASS** |
| `--log-format json` forces PTY | ✅ Required | ❌ Not implemented | **FAIL** |
| `--terminal-columns` support | ✅ Optional | ❌ Missing | **FAIL** |
| `--terminal-rows` support | ✅ Optional | ❌ Missing | **FAIL** |
| Terminal resize events | ⚠️ Implied | ❌ Not implemented | **FAIL** |

**Impact**:
- Cannot control terminal dimensions
- JSON log format doesn't influence PTY behavior
- No terminal resize support

---

## 8. Data Folder and Caching Gaps

### Specification Requirements

```
Data Folders:
--user-data-folder <PATH>           Host folder for CLI state
--container-data-folder <PATH>      Container-side data folder
--container-system-data-folder <PATH>   Container system state

Caching:
- userEnvProbe results may be cached when container session data folder available
- PATH merging computed on each run if not cached
```

### Current Implementation

- `--user-data-folder` exists as global flag
- No container data folder flags
- No caching of environment probe results

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| `--user-data-folder` | ✅ Optional | ✅ Global flag | **PARTIAL** |
| `--container-data-folder` | ✅ Optional | ❌ Missing | **FAIL** |
| `--container-system-data-folder` | ✅ Optional | ❌ Missing | **FAIL** |
| Env probe caching | ✅ Optional | ❌ Not implemented | **FAIL** |

**Impact**:
- Cannot specify container data folders for session state
- No performance optimization via caching

---

## 9. Docker/Tooling Configuration Gaps

### Specification Requirements

```
--docker-path <PATH>           Docker CLI path (default: docker)
--docker-compose-path <PATH>   Docker Compose CLI path (auto-detected)
```

### Current Implementation

- Docker runtime is hardcoded via `CliDocker::new()`
- No ability to specify custom Docker CLI path
- Docker Compose path not configurable

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| `--docker-path` | ✅ Optional | ❌ Missing | **FAIL** |
| `--docker-compose-path` | ✅ Optional | ❌ Missing | **FAIL** |

**Impact**:
- Cannot use custom Docker CLI installation
- Cannot override Docker Compose path

---

## 10. Working Directory Resolution Gaps

### Specification Requirements

```pseudocode
remote_cwd = props.remoteWorkspaceFolder OR props.homeFolder
```

Where:
- `remoteWorkspaceFolder` comes from container properties (workspace mount)
- `homeFolder` is user's home directory in container

### Current Implementation

```rust
let working_dir = if let Some(ref cli_workdir) = args.workdir {
    cli_workdir.clone()
} else if !args.id_label.is_empty() {
    String::from("/")  // Default to root for id-label
} else {
    // Use config's workspaceFolder or default to /workspaces/{name}
    determine_container_working_dir(&config, workspace_folder)
};
```

### Gaps

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Use `remoteWorkspaceFolder` | ✅ Required | ⚠️ Uses config approximation | **PARTIAL** |
| Fallback to `homeFolder` | ✅ Required | ❌ Fallback to `/` | **FAIL** |
| `--workdir` override | ⚠️ Extension | ✅ Implemented | **EXTENSION** |

**Impact**:
- Doesn't properly resolve workspace mount point
- Fallback for id-label case is `/` instead of user home

---

## 11. Testing Gaps

### Existing Tests

From `smoke_exec.rs`:
- ✅ stdout without TTY
- ✅ Exit code propagation
- ✅ Working directory behavior
- ✅ `--env` merging
- ✅ TTY detection
- ✅ Compose/subfolder config

From `integration_exec.rs`:
- ✅ Missing config error
- ✅ Empty command error
- ✅ Valid config but no container error
- ✅ Invalid env format error
- ✅ Working directory config
- ✅ `--workdir` flag
- ✅ Workdir precedence

### Missing Tests

| Test Case | Spec Requirement | Status |
|-----------|------------------|--------|
| `--container-id` explicit targeting | §2 | ❌ Missing |
| `--remote-env` with empty value | §2 | ❌ Missing |
| `--default-user-env-probe` modes | §2, §5 | ❌ Missing |
| Image metadata merging | §4 | ❌ Missing |
| Variable substitution with container env | §4 | ❌ Missing |
| Environment probe (userEnvProbe) | §5 | ❌ Missing |
| Environment merge order (shell → CLI → config) | §5 | ❌ Missing |
| Config `remoteEnv` application | §5 | ❌ Missing |
| Signal exit code mapping (`128 + signal`) | §9 | ❌ Missing |
| Binary stdin/stdout passthrough | §14 | ❌ Missing |
| Terminal dimensions | §2 | ❌ Missing |
| Docker path configuration | §2 | ❌ Missing |

---

## 12. Summary of Gaps by Severity

### Critical (Blocks Core Functionality)
1. ❌ Missing `--container-id` flag
2. ❌ Missing `--default-user-env-probe` flag and environment probe system
3. ❌ No image metadata reading and merging
4. ❌ No container-aware variable substitution
5. ❌ No `userEnvProbe` implementation (shell environment not loaded)
6. ❌ Config `remoteEnv` not applied
7. ❌ `remoteUser` not resolved from merged config

### High (Limits Functionality)
8. ❌ `--remote-env` semantics different from spec (has `--env` instead)
9. ❌ Empty environment values not supported
10. ❌ Environment merge order not implemented
11. ❌ Signal exit code mapping incomplete
12. ❌ `--mount-workspace-git-root` flag missing

### Medium (Workflow Limitations)
13. ❌ Docker tooling flags missing (`--docker-path`, `--docker-compose-path`)
14. ❌ Container data folder flags missing
15. ❌ Working directory doesn't properly use `remoteWorkspaceFolder`/`homeFolder`
16. ❌ No environment probe caching

### Low (Minor Issues)
17. ❌ Terminal dimension flags missing
18. ❌ `--log-format json` doesn't influence PTY behavior
19. ⚠️ Some global flags should be subcommand-specific
20. ⚠️ Extensions (`--user`, `--no-tty`, `--workdir`, `--service`) not in spec

---

## 13. Design Decision Analysis

### Decision: Use `--env` instead of `--remote-env`

**Current Behavior**: Flag is named `--env`

**Spec Guidance**: Flag should be named `--remote-env` to clarify it sets environment in the remote (container) context

**Impact**: Naming inconsistency with specification

**Recommendation**: Rename to `--remote-env` or alias both for compatibility

---

### Decision: No environment probe system

**Current Behavior**: Environment comes only from `--env` flag

**Spec Guidance**: Should probe container shell environment using `userEnvProbe` setting, with configurable probe modes

**Impact**: Critical functionality missing - user shell initialization not run, PATH not properly set

**Recommendation**: Implement `ContainerEnvironmentProber` integration for exec command

---

### Decision: No image metadata merging

**Current Behavior**: Config is loaded but not merged with container labels

**Spec Guidance**: Should read `devcontainer.metadata` labels from container and merge with config

**Impact**: Container-specific settings like `remoteUser`, `remoteEnv` from labels not applied

**Recommendation**: Implement metadata extraction and merging logic

---

### Decision: Extensions beyond spec

**Current Behavior**: Provides `--user`, `--no-tty`, `--workdir`, `--service`

**Spec Guidance**: These flags not mentioned in spec

**Assessment**: Useful extensions
- `--user`: Overrides `remoteUser` from config (useful)
- `--no-tty`: Explicit TTY control (useful)
- `--workdir`: Overrides working directory (useful)
- `--service`: Docker Compose service targeting (useful for multi-service projects)

**Recommendation**: Keep as documented extensions

---

## 14. Recommended Refactoring Approach

### Phase 1: Critical Flags and Container Selection (Week 1)
1. Add `--container-id` flag
2. Add `--remote-env` flag (or rename `--env`)
3. Support empty environment values
4. Add `--default-user-env-probe` flag
5. Update argument parsing and validation

**Estimated Effort**: 2-3 days

### Phase 2: Environment Probe System (Week 2)
1. Implement `probe_remote_env` function
2. Support `userEnvProbe` modes from config
3. Integrate with existing `ContainerEnvironmentProber`
4. Implement environment merge order (shell → CLI → config)
5. Apply config `remoteEnv` to exec environment

**Estimated Effort**: 4-5 days

### Phase 3: Configuration Merging (Week 2-3)
1. Extract image metadata labels from container
2. Implement config + metadata merging
3. Resolve `remoteUser` from merged config
4. Implement container-aware variable substitution
5. Use `remoteWorkspaceFolder` and `homeFolder` properly

**Estimated Effort**: 3-4 days

### Phase 4: Remaining Features (Week 3)
1. Add Docker tooling flags
2. Add container data folder flags
3. Add terminal dimension flags
4. Implement signal exit code mapping
5. Add env probe caching
6. Implement `--mount-workspace-git-root` handling

**Estimated Effort**: 2-3 days

### Phase 5: Testing and Documentation (Week 4)
1. Add missing test cases
2. Update documentation
3. Add examples
4. Verify spec compliance

**Estimated Effort**: 2-3 days

**Total Estimated Effort**: 13-18 days

---

## 15. Breaking Changes Required

### CLI Changes (Minor)
1. Rename `--env` to `--remote-env` (or add alias for compatibility)
2. Consider moving some global flags to subcommand level

### Behavior Changes (Significant)
1. Environment will include shell environment (via probe)
2. Config `remoteEnv` will be applied
3. `remoteUser` from config will be used if `--user` not specified
4. Working directory resolution will change (use proper remote paths)
5. Signal exit codes will be mapped to `128 + signal`

### Migration Path
1. Phase in environment probe system with warnings
2. Document new environment handling behavior
3. Provide migration guide for users relying on current behavior

---

## 16. Compliance Checklist

Use this checklist to track implementation progress:

### Container Selection
- [ ] `--container-id` flag
- [ ] `--id-label` validation (non-empty value)
- [ ] `--workspace-folder` (currently global)
- [ ] `--config` (currently global)
- [ ] `--override-config` (currently global)
- [ ] `--mount-workspace-git-root` flag

### Environment
- [ ] `--remote-env` flag (or rename `--env`)
- [ ] Support empty environment values
- [ ] `--default-user-env-probe` flag
- [ ] Implement `userEnvProbe` system
- [ ] Environment merge order (shell → CLI → config)
- [ ] Apply config `remoteEnv`
- [ ] Cache probe results

### Configuration
- [ ] Extract image metadata from container
- [ ] Merge config with metadata
- [ ] Container-aware variable substitution
- [ ] Resolve `remoteUser` from merged config
- [ ] Use `remoteWorkspaceFolder`/`homeFolder` for CWD

### Tooling
- [ ] `--docker-path` flag
- [ ] `--docker-compose-path` flag
- [ ] `--user-data-folder` (currently global)
- [ ] `--container-data-folder` flag
- [ ] `--container-system-data-folder` flag

### Terminal
- [ ] `--terminal-columns` flag
- [ ] `--terminal-rows` flag
- [ ] `--log-format` influences PTY selection
- [ ] Terminal resize events

### Exit Codes
- [ ] Signal exit code mapping (`128 + signal`)
- [ ] Named signal mapping

### Testing
- [ ] Test `--container-id` targeting
- [ ] Test empty `--remote-env` values
- [ ] Test `userEnvProbe` modes
- [ ] Test image metadata merging
- [ ] Test variable substitution
- [ ] Test environment merge order
- [ ] Test signal exit codes
- [ ] Test terminal dimensions

---

## 17. Conclusion

The current `exec` implementation provides **basic command execution** but is missing significant spec-defined functionality:

**Major Gaps:**
1. **No environment probe system** - Critical for proper shell initialization
2. **No configuration merging** - Image metadata not integrated
3. **Missing container-id flag** - Cannot target arbitrary containers
4. **Environment handling incomplete** - Config `remoteEnv` ignored, no shell env

**What Works:**
- ✅ Basic command execution with exit code propagation
- ✅ `--id-label` container selection
- ✅ TTY detection
- ✅ Environment variable setting (via `--env`)
- ✅ Working directory control (via `--workdir`)
- ✅ Docker Compose service targeting (extension)

**Recommendation**: Treat environment probe and configuration merging as highest priority. These are critical for proper dev container behavior and affect all exec invocations.

**Priority**: HIGH - The exec command is central to dev container workflows. Spec compliance is essential for correct environment setup.

**Estimated Total Effort**: 13-18 developer days for full specification compliance.
