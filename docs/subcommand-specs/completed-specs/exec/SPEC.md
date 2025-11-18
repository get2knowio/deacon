# Exec Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Execute a command inside an existing dev container, applying devcontainer.json semantics (remoteUser, remoteEnv, userEnvProbe, workspace mapping) so the command runs as if inside the configured development environment.
- User Personas:
  - Developers: Run build/test/tools inside the dev container using host terminals or scripts.
  - CI/Automation: Execute tasks in a known, reproducible container environment.
  - Maintainers: Reproduce issues by running diagnostic commands inside the active container.
- Specification References:
  - Dev Container configuration (remoteUser, remoteEnv, userEnvProbe): https://containers.dev/implementors/spec
  - Merged configuration and image metadata labels: https://containers.dev/implementors/spec
- Related Commands:
  - `up`: Ensures the container exists and is running; `exec` assumes a valid container.
  - `run-user-commands` / `set-up`: Apply lifecycle hooks; `exec` is ad-hoc command execution afterward.
  - `read-configuration`: Inspect effective configuration that influences execution.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer exec [options] <cmd> [args..]`
- Flags and Options:
  - Container selection and config:
    - `--workspace-folder <PATH>` Optional. Used to infer the target container id-labels when `--container-id`/`--id-label` are not supplied.
    - `--container-id <ID>` Optional. Explicit container ID to target.
    - `--id-label <name=value>` Optional, repeatable. Label selector(s) to find the container. If none specified and no `--container-id`, inferred from `--workspace-folder` using labels `devcontainer.local_folder` and `devcontainer.config_file`.
    - `--config <PATH>` Optional. devcontainer.json path to consider for environment/remoteUser resolution.
    - `--override-config <PATH>` Optional. Override devcontainer.json path; required when no base config exists but overrides are desired.
    - `--mount-workspace-git-root` Optional boolean, default true. Affects workspace resolution when reading config; has no effect if only `--container-id`/`--id-label` are provided.
  - Execution environment:
    - `--remote-env <name=value>` Optional, repeatable. Extra environment variables injected into the process in the container (value may be empty).
    - `--default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}` Optional, default `loginInteractiveShell`. Used when the config does not set `userEnvProbe`.
  - Docker tooling and data folders:
    - `--docker-path <PATH>` Optional. Docker CLI path (default `docker`).
    - `--docker-compose-path <PATH>` Optional. Docker Compose CLI path (auto-detected when not provided).
    - `--user-data-folder <PATH>` Optional. Host folder persisted across sessions for CLI state.
    - `--container-data-folder <PATH>` Optional. Container-side data folder for user state.
    - `--container-system-data-folder <PATH>` Optional. Container system state folder.
  - Logging and terminal:
    - `--log-level {info|debug|trace}` Optional, default `info`.
    - `--log-format {text|json}` Optional, default `text`.
   - `--terminal-columns <N>` Optional. Requires `--terminal-rows`.
   - `--terminal-rows <N>` Optional. Requires `--terminal-columns`.
   - Hidden/testing:
   - `--skip-feature-auto-mapping` Optional hidden boolean. Parity/testing only.

## PTY Sizing Limits

When a PTY is allocated, the CLI applies terminal sizing using the following mechanism:
- **Primary mechanism**: The CLI calls the Docker API/container runtime resize endpoint to set rows and columns.
- **Fallback mechanism**: Additionally, `COLUMNS` and `LINES` environment variables are injected into the exec process as a best-effort fallback.
- **Important**: We do NOT run `stty` inside the container to set terminal size.

**Platform and runtime limitations**:
- Some containers or shells may ignore the initial size until a resize event occurs from a controlling PTY.
- Some container runtimes may not fully support the resize API.
- The `--terminal-columns` and `--terminal-rows` flags provide best-effort hints but may be ignored by the runtime or the container process.
- For precise sizing requirements, use interactive shells that properly handle TTY resize events.

 - Flag Taxonomy:

  - Required: Positional `<cmd>` is required. At least one of `--container-id`, `--id-label`, or `--workspace-folder` must be provided.
  - Optional: All other flags.
  - Mutually exclusive groups: None. Paired requirement: `--terminal-columns` implies `--terminal-rows` and vice versa.
  - Deprecated: None.
- Argument Validation Rules:
  - `--id-label` must match `<name>=<value>` (regex `/.+=.+/`). Multiple occurrences allowed.
  - `--remote-env` must match `<name>=<value>` (regex `/.+=.*/`), where value may be empty.
  - If none of `--container-id`, `--id-label`, or `--workspace-folder` are set: error “Missing required argument: One of --container-id, --id-label or --workspace-folder is required.”
  - Positional `<cmd>` required; `[args..]` captured verbatim (parsing halts at non-option for `exec`).

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(argv) -> ParsedInput:
    // yargs-equivalent settings: boolean-negation=false; halt-at-non-option=true when subcommand is exec
    parsed.user_data_folder = argv['--user-data-folder']
    parsed.docker_path = argv['--docker-path']
    parsed.docker_compose_path = argv['--docker-compose-path']
    parsed.container_data_folder = argv['--container-data-folder']
    parsed.container_system_data_folder = argv['--container-system-data-folder']
    parsed.workspace_folder = resolve_path(CWD, argv['--workspace-folder']) if set
    parsed.mount_workspace_git_root = argv.get_bool('--mount-workspace-git-root', default=true)
    parsed.container_id = argv['--container-id']
    parsed.id_labels = to_array(argv['--id-label']) // preserve order
    parsed.config_file = resolve_uri(CWD, argv['--config']) if set
    parsed.override_config_file = resolve_uri(CWD, argv['--override-config']) if set
    parsed.log_level = map_log_level(argv['--log-level'] OR 'info')
    parsed.log_format = argv['--log-format'] OR 'text'
    parsed.term_cols = argv['--terminal-columns']
    parsed.term_rows = argv['--terminal-rows']
    parsed.default_user_env_probe = argv['--default-user-env-probe'] OR 'loginInteractiveShell'
    parsed.remote_env_kv = to_array(argv['--remote-env'])
    parsed.skip_feature_auto_mapping = argv.get_bool('--skip-feature-auto-mapping', default=false)
    parsed.cmd = argv.positional('<cmd>')
    parsed.args = argv.positional('[args..]') OR []

    // Validation
    REQUIRE parsed.cmd IS NOT EMPTY
    IF parsed.id_labels EXISTS AND ANY(l NOT MATCHES /.+=.+/) THEN ERROR('id-label must match <name>=<value>')
    IF parsed.remote_env_kv EXISTS AND ANY(e NOT MATCHES /.+=.*/) THEN ERROR('remote-env must match <name>=<value>')
    IF NOT (parsed.container_id OR parsed.id_labels OR parsed.workspace_folder) THEN
        ERROR('One of --container-id, --id-label or --workspace-folder is required.')
    IF exactly_one_of(term_cols, term_rows) THEN ERROR('terminal-columns requires terminal-rows and vice versa')
    RETURN parsed
END FUNCTION
```

## 4. Configuration Resolution
- Configuration Sources (highest precedence first):
  1) Command-line flags (container selection, logging, env probe default, remote-env additions)
  2) Environment variables used during variable substitution (e.g., `${env:VAR}`, `${localEnv:VAR}`)
  3) Configuration files: `devcontainer.json` (from `--config`, or discovered under `--workspace-folder`), plus optional `--override-config`
  4) Default values (e.g., `default-user-env-probe=loginInteractiveShell`)
- Discovery and selection:
  - If `--config` is provided, use it. Else if `--workspace-folder` is provided, attempt `.devcontainer/devcontainer.json`, then `.devcontainer.json`. If `--override-config` is provided without a base config, require it explicitly.
  - If any of `--workspace-folder`, `--config`, or `--override-config` are provided but no config is found/readable: error “Dev container config (<path>) not found.”
- Merge Algorithm:
  - Determine target container via `--container-id` or id-labels inferred from `--workspace-folder` and optional `--config` path.
  - Read devcontainer metadata labels from the container to produce `imageMetadata`. Merge `config` with `imageMetadata` → `mergedConfig`.
  - Apply container-aware substitution on `mergedConfig` using the container’s environment (`containerSubstitute(platform, configPath, containerEnv, mergedConfig)`).
- Variable Substitution:
  - Apply standard substitution rules for `${env:VAR}`, `${localEnv:VAR}`, `${workspaceFolder}` and related variables as defined by the spec.
  - Container-aware substitution uses container environment values for `${env:VAR}` when the command runs inside the container.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(input: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    cli_host = detect_cli_host()
    session_start = now()
    log = create_logger(level=input.log_level, format=input.log_format,
                        terminal_dimensions=(input.term_cols,input.term_rows) OR detect_tty())
    is_tty = (stdin_is_tty AND stdout_is_tty) OR (input.log_format == 'json')
    docker = configure_docker(cli_host, input.docker_path, input.docker_compose_path)

    // Phase 2: Pre-execution validation
    workspace = input.workspace_folder ? workspace_from_path(cli_host.path, input.workspace_folder) : undefined
    config_uri = resolve_config_uri(workspace, input.config_file, input.override_config_file)
    configs = config_uri ? read_devcontainer_config(cli_host, workspace, config_uri, input.mount_workspace_git_root, input.override_config_file) : undefined
    IF (input.config_file OR input.workspace_folder OR input.override_config_file) AND configs IS UNDEFINED THEN
        ERROR("Dev container config (<path>) not found.")
    config = configs?.config OR substitute_with_host_env(cli_host.platform, cli_host.env)

    { container, id_labels } = find_container_and_labels(docker, input.container_id, input.id_labels, input.workspace_folder, config_uri?.fsPath)
    IF NOT container THEN bail_out(log, 'Dev container not found.')

    // Phase 3: Main execution
    image_metadata = get_image_metadata_from_container(container, config, /*features*/undefined, id_labels)
    merged = merge_configuration(config.config, image_metadata)
    props = create_container_properties(docker, container.Id, configs?.workspaceConfig.workspaceFolder, merged.remoteUser)
    updated = container_substitute(cli_host.platform, config.config.configFilePath, props.env, merged)
    remote_env_base = probe_remote_env(cli_host, props, updated)   // userEnvProbe with caching if available
    remote_env = merge(remote_env_base, kv_list_to_map(input.remote_env_kv), updated.remoteEnv)
    remote_cwd = props.remoteWorkspaceFolder OR props.homeFolder

    exec_options = { remoteEnv: remote_env, pty: is_tty, print: 'continuous' }
    run_remote_command(log, stdin/stdout/stderr based on log_format, props, [input.cmd]+input.args, remote_cwd, exec_options)

    // Phase 4: Post-execution
    RETURN Success(code=0)
CATCH err:
    // Surface remote exit status or local failure
    return map_to_exit(err.code, err.signal)  // code OR (128+signal) OR 1
FINALLY:
    dispose_logger()
END FUNCTION
```

## 6. State Management
- Persistent State: None specific to `exec`. The `--user-data-folder` may be used by shared components (e.g., temp/cache) but `exec` does not create or manage its own files.
- Cache Management: userEnvProbe results may be cached when a container session data folder is available; `exec` does not mandate it. PATH merging is computed on each run if not cached.
- Lock Files: None.
- Idempotency: Running the same `exec` multiple times is safe and produces the same side effects as re-running the command in the container.

## 7. External System Interactions

### Docker/Container Runtime
```pseudocode
FUNCTION interact_with_docker_exec(container_id, user, env, cwd, cmd, args, pty) -> Result:
    argv = ['exec', '-i']
    IF pty THEN argv += ['-t']
    IF user THEN argv += ['-u', user]
    FOR EACH (k,v) IN env: argv += ['-e', `${k}=${v}`]
    IF cwd THEN argv += ['-w', cwd]
    argv += [container_id, cmd] + args
    // spawn via plain or PTY runner
    return docker(argv)
END FUNCTION
```
- Expected behavior:
  - Non-PTY mode: separate stdout/stderr; binary-safe stdin/out; exit status is child process code.
  - PTY mode: merged stream; interactive input supported; exit status derived from PTY exit (code/signal).
- Error handling:
  - Docker CLI not found/unavailable: local error surfaced; exit non-zero.
  - Container stopped: `docker exec` fails; error propagated.

### OCI Registries
- None. `exec` does not contact registries.

### File System
- Reads local config files if provided via `--config` or discovered via `--workspace-folder`.
- No writes. Cross-platform paths handled by the CLI host utilities; in-container paths are POSIX-style.

## 8. Data Flow Diagrams

```
┌─────────────────┐
│ User Input      │
└────────┬────────┘
         │
         ▼
┌──────────────────────────┐
│ Parse & Validate Args    │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐
│ Resolve Config/Container │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐      docker exec/PT
│ Compute Env (userEnvProbe│──────▶│  Run Command   │
│ + remoteEnv merge)       │       └─────────────────┘
└────────┬─────────────────┘                │
         │                                  ▼
         ▼                         ┌─────────────────┐
┌──────────────────────────┐       │ Status/Exitcode │
│ Select CWD (workspace or │       └─────────────────┘
│ home)                    │
└──────────────────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Invalid `--id-label` or `--remote-env` format: parser error, exit 1.
  - Missing container selection (`--container-id`, `--id-label`, `--workspace-folder`): parser error, exit 1.
  - Config path provided but not found: error “Dev container config (<path>) not found.”
  - Dev container not found: error “Dev container not found.”
- System Errors:
  - Docker CLI unavailable: fail fast with clear message; exit non-zero.
  - `docker exec` failures (container stopped, permission issues): propagate exit status; include stderr.
- Configuration Errors:
  - Invalid/malformed devcontainer.json: surface parse error; exit non-zero.
- Exit Code Mapping:
  - If remote returns numeric code: use it.
  - Else if terminated by signal: return `128 + signal` (POSIX convention).
  - Else: return `1`.

## 10. Output Specifications
- Standard Output (stdout):
  - Text mode: stream the command’s stdout; stderr is streamed to stderr. No header. No trailing JSON payload.
  - JSON mode: command output is emitted through JSON log events (merged stream when PTY). No separate success JSON payload.
  - Quiet mode: not applicable; use `--log-level` to reduce non-command logs.
- Standard Error (stderr):
  - Logs and errors are written according to `--log-level` and format. In text mode, human-readable lines; in JSON mode, structured events.
- Exit Codes:
  - See Error Handling → Exit Code Mapping.

## 11. Performance Considerations
- Caching Strategy: Optional userEnvProbe caching when session cache is available; otherwise computed per run.
- Parallelization: Not applicable; single command execution.
- Resource Limits: PTY allocation when possible; honor terminal dimensions and resize events.
- Optimization Opportunities: Reuse session-level env cache to avoid repeated shell startups; avoid unnecessary config file reads when `--container-id` is used without config flags.

## 12. Security Considerations
- Secrets Handling: Values passed via `--remote-env` may contain secrets; they are injected into the container process environment and may appear in process listings inside the container. Avoid logging secret values; if a secret-masking facility is present in the logger, enable it.
- Privilege Model: The command runs as `remoteUser` (or container default or root). No elevation is performed by `exec` itself.
- Input Sanitization: Validate `--id-label`/`--remote-env` formats; pass environment via `-e KEY=VAL` to `docker exec` to avoid shell injection.
- Container Isolation: Execution occurs within the boundaries of the selected container; host is unaffected except for I/O.

## 13. Cross-Platform Behavior
| Aspect | Linux | macOS | Windows | WSL2 |
|--------|-------|-------|---------|------|
| Path handling | POSIX; local paths resolved via CLI host | Same | Uses `docker.exe`; host paths use `\` but in-container paths are POSIX | Linux-like inside WSL; host/workspace paths may be Windows-style |
| Docker socket | `/var/run/docker.sock` | Docker Desktop | Docker Desktop | WSL distro Docker integration |
| PTY behavior | Standard TTY; resize events supported | Same | Requires PTY library; fallback to non-PTY if unavailable | Same as Linux |
| User ID mapping | Remote user per config/metadata | Same | Same | Same |

## 14. Edge Cases and Corner Cases
- Container found but stopped → `docker exec` fails; surface error and non-zero exit.
- `--remote-env` with empty value → inject as empty variable.
- Binary stdin/stdout passthrough in non-PTY mode.
- Large outputs in PTY mode → merged stream; ensure continuous printing.
- Terminal size not provided and stdout not a TTY → run non-PTY to avoid `-t` requirement.
- Config discovery requested but no config present → explicit error.

## 15. Testing Strategy

```pseudocode
TEST SUITE for exec:
    TEST "happy path":
        GIVEN running container for workspace
        WHEN devcontainer exec --workspace-folder <ws> echo hi
        THEN exit 0 AND stdout == "hi\n"

    TEST "no PTY":
        GIVEN non-TTY invocation
        WHEN devcontainer exec --workspace-folder <ws> [ ! -t 1 ]
        THEN exit 0

    TEST "PTY":
        GIVEN TTY invocation
        WHEN devcontainer exec --workspace-folder <ws> [ -t 1 ]
        THEN exit 0

    TEST "exit code propagation":
        WHEN devcontainer exec --workspace-folder <ws> sh -c 'exit 123'
        THEN exit code == 123

    TEST "binary streaming":
        WHEN piping 256-byte buffer to 'cat'
        THEN stdout equals stdin

    TEST "remote-env injection":
        WHEN --remote-env FOO=BAR --remote-env BAZ=
        THEN inside printenv shows FOO=BAR and BAZ=""

    TEST "container not found":
        WHEN no container matches selection
        THEN error "Dev container not found." and exit non-zero

    TEST "config path not found":
        WHEN --config points to missing file
        THEN error "Dev container config (...) not found."
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: None.
- Breaking Changes: None required; ensure parity with reference CLI behavior for TTY detection, env merging order, and exit code mapping.
- Compatibility Shims: None.

---

### Design Decisions (Critical Analysis)

#### Design Decision: Default `userEnvProbe=loginInteractiveShell`
Implementation Behavior:
Default probe runs the container’s shell as a login+interactive session to collect environment, falling back to `printenv` if needed; results are optionally cached per session.

Specification Guidance:
The spec defines `userEnvProbe` and its allowed values; default is implementation-defined.

Rationale:
Login + interactive ensures user initialization scripts are applied (PATH, environment managers), matching developer expectations.

Alternatives Considered:
`interactiveShell` or `loginShell` only, or `none` to skip probing.

Trade-offs:
More robust env at the cost of startup latency; caching mitigates recurring cost.

#### Design Decision: Env merge order (shell → CLI `--remote-env` → config `remoteEnv`)
Implementation Behavior:
`probeRemoteEnv` produces base env, then merges CLI-provided `--remote-env`, then merges config’s `remoteEnv`, where later entries override earlier ones.

Specification Guidance:
Spec allows setting environment via configuration; CLI flags augment runtime behavior. Merge order is not explicitly mandated.

Rationale:
Config represents the authoritative environment for the dev container; CLI flags are ad-hoc additions for a single invocation.

Alternatives Considered:
CLI should override config; or deep/conditional merges.

Trade-offs:
Prioritizing config improves reproducibility but may surprise users expecting CLI to win; document clearly.

#### Design Decision: PTY selection heuristic
Implementation Behavior:
Use PTY when both stdin and stdout are TTYs, or when `--log-format json` is requested (to provide consistent streaming behavior).

Specification Guidance:
Spec does not mandate PTY behavior; terminal behavior is implementation-defined.

Rationale:
PTY enables interactive workflows while avoiding `-t` errors when not attached to a TTY. JSON mode benefits from uniform streaming.

Alternatives Considered:
Always PTY or always non-PTY.

Trade-offs:
PTY merges stdout/stderr and may affect signal handling; non-PTY is better for binary streams.

#### Design Decision: Remote CWD selection (workspace or home)
Implementation Behavior:
If a remote workspace folder is known, use it; otherwise fall back to the user’s home inside the container.

Specification Guidance:
Spec implies workspace-relative execution; falling back to home is reasonable when no workspace is mounted.

Rationale:
Commands typically expect to run within the workspace; home is a safe fallback.

Alternatives Considered:
Always use `/` or image working directory.

Trade-offs:
Home may differ from project context; safer than arbitrary system paths.

#### Design Decision: Exit code mapping for signals
Implementation Behavior:
If the process exits by signal, report `128 + signal` per POSIX convention; otherwise use the numeric exit code or `1`.

Specification Guidance:
Spec does not prescribe exit code mapping for signal termination; POSIX convention is widely used.

Rationale:
Provides consistent, script-friendly exit statuses.

Alternatives Considered:
Return `1` for all signal terminations.

Trade-offs:
Slight learning curve; documented behavior aligns with common CLI tools.

