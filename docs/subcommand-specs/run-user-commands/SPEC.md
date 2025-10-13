# 1. Subcommand Overview

- Purpose: Execute configured devcontainer lifecycle commands inside an existing container in a deterministic, spec-compliant order, with proper environment resolution, secrets injection, logging, and idempotency.
- User Personas:
  - Devs resuming work who want to (re)run onCreate/updateContent/post* hooks after an image build or container restart.
  - CI systems prebuilding images and running only initialization/update steps.
  - Tooling/extensions that need to trigger dotfiles installation and personalization without recreating the container.
- Specification References:
  - Dev Container Spec – Lifecycle and Commands (initialize/onCreate/updateContent/postCreate/postStart/postAttach, waitFor, userEnvProbe, remoteEnv)
    - https://containers.dev/implementors/spec (Lifecycle Hooks, Environment Resolution)
  - Features metadata influencing lifecycle command ordering
- Related Commands:
  - `up` creates/starts the container and may run hooks as part of setup.
  - `exec` runs arbitrary commands in an existing container (no lifecycle semantics).
  - `read-configuration` shows effective configuration (useful to debug what run-user-commands will run).

# 2. Command-Line Interface

- Full Syntax: `devcontainer run-user-commands [options]`
- Flags and Options:
  - Container selection and config:
    - `--container-id <ID>` Optional. Target container ID.
    - `--id-label <name=value>` Optional, repeatable. Used to find the container when `--container-id` is not provided. If neither `--container-id` nor `--id-label` is set, `--workspace-folder` is used to infer labels.
    - `--workspace-folder <PATH>` Optional. Root where config discovery starts; also used to infer labels when locating the container.
    - `--config <PATH>` Optional. Explicit devcontainer.json path.
    - `--override-config <PATH>` Optional. Override devcontainer.json path (required when there is no devcontainer.json otherwise).
  - Docker tooling and state:
    - `--docker-path <PATH>` Optional. Docker CLI binary path.
    - `--docker-compose-path <PATH>` Optional. Docker Compose CLI path.
    - `--container-data-folder <PATH>` Optional. In‑container folder for user data/state (default `.devcontainer` under the remote home).
    - `--container-system-data-folder <PATH>` Optional. In‑container folder for system state (default `/var/devcontainer`).
    - `--mount-workspace-git-root` Optional boolean, default true. Mount behavior parity with other commands; impacts config discovery and labeling.
    - `--container-session-data-folder <PATH>` Optional. Cache folder for CLI data in the container (e.g., userEnvProbe cache files).
  - Lifecycle controls:
    - `--skip-non-blocking-commands` Optional boolean. Stops after reaching `waitFor` (default `updateContentCommand`), returning `"skipNonBlocking"`.
    - `--prebuild` Optional boolean. Stops after `onCreateCommand` and `updateContentCommand` (re‑running updateContent if it ran before), returning `"prebuild"`.
    - `--stop-for-personalization` Optional boolean. Stops after dotfiles installation, returning `"stopForPersonalization"`.
    - `--skip-post-attach` Optional boolean. Do not run `postAttachCommand`.
  - Environment:
    - `--default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}` Optional. Default for `userEnvProbe` when not set in config.
    - `--remote-env <NAME=VALUE>` Optional, repeatable. Extra env vars for command execution inside the container.
    - `--secrets-file <PATH>` Optional. JSON file with key/value secret env vars. Values are injected into the environment and masked from logs.
  - Dotfiles:
    - `--dotfiles-repository <URL|owner/repo|./path>` Optional. Dotfiles source; `owner/repo` expands to `https://github.com/<owner>/<repo>.git`.
    - `--dotfiles-install-command <CMD>` Optional. Path to script to run after cloning; falls back to common install/bootstrap/setup names.
    - `--dotfiles-target-path <PATH>` Optional, default `~/dotfiles`.
  - Logging/terminal:
    - `--log-level {info|debug|trace}` Optional, default `info`.
    - `--log-format {text|json}` Optional, default `text`.
    - `--terminal-columns <N>` Optional. Requires `--terminal-rows`.
    - `--terminal-rows <N>` Optional. Requires `--terminal-columns`.
  - Hidden/testing:
    - `--skip-feature-auto-mapping` Optional hidden boolean for internal testing.
  
- Flag Taxonomy:
  - Required vs Optional: At least one of `--container-id`, `--id-label`, or `--workspace-folder` is required. All other flags are optional.
  - Paired requirements: `--terminal-columns` implies `--terminal-rows` and vice versa.
  - Mutually exclusive: None enforced; `--container-id` may be combined with `--id-label` (labels then contribute to substitution, not lookup).
  - Deprecated: None.

- Argument Validation Rules:
  - `--id-label` must match `<name>=<value>`; regex `/.+=.+/` (value must be non‑empty).
  - `--remote-env` must match `<name>=<value>`; regex `/.+=.*/` (value may be empty).
  - Container selection: if none of `--container-id`, `--id-label`, or `--workspace-folder` are present → error.
  - `--terminal-columns` and `--terminal-rows` must be numbers and provided together.

# 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    // yargs-driven parsing with validation hooks
    PARSE flags as described in Section 2
    VALIDATE:
        - id-label items match .+=.+
        - remote-env items match .+=.*
        - at least one of container-id, id-label, workspace-folder
        - terminal-columns and terminal-rows pairing
    NORMALIZE:
        - resolve workspace-folder/config paths relative to CWD
        - coerce repeatable options to string[]
    RETURN ParsedInput
END FUNCTION
```

# 4. Configuration Resolution

- Sources (highest precedence first):
  1) Command-line flags (`--remote-env`, `--default-user-env-probe`, dotfiles flags, etc.)
  2) Environment variables (used for substitution and CLI host behavior)
  3) devcontainer.json (and any override) discovered from `--workspace-folder`/`--config`
  4) Image metadata embedded into the container (Feature and config metadata)
  5) Defaults (e.g., `waitFor` defaults to `updateContentCommand`)

- Discovery and merge:
  - Resolve `workspaceFolder`, `configFile`, `overrideConfigFile`.
  - If discovery is requested (workspace or override given) but config is not found → error with message “Dev container config (...) not found.”
  - Load config (with features) and produce `config0` (or a trivial empty config wrapper when missing).
  - Find target container and derive id labels:
    - If `--container-id`: inspect directly; else compute labels from workspace folder and config file path and find the container.
    - If not found → bail out (“Dev container not found.”).
  - Apply substitution layers:
    - Before-container: `${devcontainerId}` derived from provided/implicit labels.
    - In-container: `${containerEnv:VAR}` using the container’s current environment.
  - Read image metadata from the container and merge into the config:
    - Produce `MergedDevContainerConfig` with ordered lifecycle command arrays and effective properties (`waitFor`, `remoteEnv`, etc.).
  - Create `ContainerProperties` from the container (`env`, `user`, `homeFolder`, timestamps, shell, exec functions, etc.).
  - Final substitution pass of the merged configuration using the actual `ContainerProperties.env`.
  - Probe remote env (Section 5) with caching if `--container-session-data-folder` is provided.

- Merge Algorithm (high-level):

```pseudocode
FUNCTION merge_configuration(userConfig, imageMetadata) -> MergedConfig:
    COPY userConfig minus replaceProperties
    merged.entrypoints = collect(imageMetadata.entrypoint)
    merged.mounts = merge_unique_targets(imageMetadata.mounts)
    merged.customizations = fold(imageMetadata.customizations)
    FOR each lifecycleHook IN [onCreate, updateContent, postCreate, postStart, postAttach]:
        merged.<hook>Commands = collect(imageMetadata.<hook>Command)
    merged.waitFor = last_defined(imageMetadata.waitFor)
    merged.remoteEnv = shallow_merge(imageMetadata.remoteEnv)
    merged.containerEnv = shallow_merge(imageMetadata.containerEnv)
    // ... additional properties per TypeScript reference
    RETURN merged
END FUNCTION
```

- Variable Substitution Rules:
  - Host/local context: `${env:V}`, `${localEnv:V}`, `${localWorkspaceFolder}`, `${localWorkspaceFolderBasename}`.
  - Container context: `${containerEnv:V}`, `${containerWorkspaceFolder}`, `${containerWorkspaceFolderBasename}`.
  - ID labels: `${devcontainerId}` available pre‑container for label derivations.
  - Missing variable errors: `${env:}` without a name is an error; missing values default to empty string unless a default is provided: `${env:NAME:default}`.

# 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    SETUP CLI host (exec/ptyExec), logging (stderr), and dimensions
    READ secrets file if provided; prepare secret masking for logs
    CREATE Docker resolver parameters with lifecycle context

    // Phase 2: Pre-execution validation
    DISCOVER config (may be undefined) and locate container (or bail out)
    BUILD substitution-aware config chain (before-container, then container)
    READ image metadata from container and MERGE with config
    CONSTRUCT ContainerProperties (user, env, shell, paths, exec fns)
    PROBE remote env (userEnvProbe + container/remote env merge) with optional cache

    // Phase 3: Main execution
    LIFECYCLE = lifecycleCommandOriginMapFromMetadata(imageMetadata)
    RESULT = runLifecycleHooks(params, LIFECYCLE, containerProps, mergedConfig, remoteEnv, secrets, stopForPersonalization)
        - onCreateCommand (idempotent by marker)
        - updateContentCommand (idempotent by marker; rerun if --prebuild)
        - if prebuild -> return 'prebuild'
        - postCreateCommand (idempotent by marker)
        - dotfiles install (if dotfiles.* set)
        - if stop-for-personalization -> return 'stopForPersonalization'
        - postStartCommand (idempotent by marker)
        - postAttachCommand (skipped if --skip-post-attach)
        - if skip-non-blocking-commands met per waitFor -> return 'skipNonBlocking'

    // Phase 4: Post-execution
    WRITE single-line JSON result to stdout: { outcome: 'success', result: RESULT }
    RETURN success
CATCH ContainerError or other error AS e:
    WRITE single-line JSON error to stdout: { outcome: 'error', message, description }
    RETURN error
FINALLY:
    DISPOSE resources (listeners, pty, log handlers)
END FUNCTION
```

# 6. State Management

- Persistent State (container):
  - Marker files in `userDataFolder` (default: `${HOME}/.devcontainer`):
    - `.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker`, `.postStartCommandMarker`.
    - Content is a timestamp (container CreatedAt/StartedAt). `updateMarkerFile` creates/updates atomically.
  - Optional env cache in `--container-session-data-folder`: `env-<userEnvProbe>.json` (JSON of probed env).
- Cache Management:
  - `probeRemoteEnv` reads cache first; on miss, runs probe and writes cache.
  - Secrets are not cached; they are only injected at runtime and redacted from logs.
- Lock Files: None.
- Idempotency:
  - Hooks are conditional on marker files; if marker timestamp matches state, hooks are skipped.
  - `--prebuild` forces `updateContentCommand` rerun if it ran before.

# 7. External System Interactions

- Docker/Container Runtime

```pseudocode
FUNCTION interact_with_docker(operation):
    // exec inside container with optional PTY
    docker exec [-i -t] [-u <user>] [-e KEY=VAL]* [-w <cwd>] <container> <cmd> [args...]
    // Container detection and metadata
    docker inspect <container>
    // Compose CLI available via configured docker-compose path when needed
END FUNCTION
```

- OCI Registries
  - Not directly used by this subcommand (relies on metadata already embedded in the image/container). Auth and pulls occur earlier (e.g., `up`/`build`).

- File System
  - In-container read/write of marker files and optional env cache.
  - Dotfiles installation may clone from network via Git inside the container.
  - Symlink/permissions: marker creation uses shell (`mkdir -p`, `noclobber`) and is robust to missing parents. Failure to write a marker results in skipping that hook.

# 8. Data Flow Diagrams

```
┌───────────────┐
│ CLI Arguments │
└──────┬────────┘
       │ parse/validate
       ▼
┌─────────────────────┐
│ Resolve Config      │
│ + Locate Container  │
└──────┬──────────────┘
       │ inspect/env
       ▼
┌─────────────────────┐
│ Build Merged Config │
│ + Substitutions     │
└──────┬──────────────┘
       │ probe env
       ▼
┌─────────────────────┐
│ Run Lifecycle Hooks │
└──────┬──────────────┘
       │ result enum
       ▼
┌─────────────────────┐
│ JSON to stdout      │
└─────────────────────┘
```

# 9. Error Handling Strategy

- User Errors
  - Invalid `--id-label` or `--remote-env` formats → CLI argument error (exit 1), message indicates required format.
  - Missing container selection (`--container-id`/`--id-label`/`--workspace-folder`) → CLI argument error.
  - Config not found → JSON error: `{ outcome: 'error', message: 'Dev container config (...) not found.', description: ... }`.
- System Errors
  - Docker unavailable / inspect fails → wrapped `ContainerError` → JSON error, exit 1.
  - Network failures during dotfiles clone → `ContainerError` with description; execution stops, exit 1.
- Configuration Errors
  - Invalid devcontainer.json or substitution failures → `ContainerError` with description referencing the source (e.g., variable without name).
- Execution Failures
  - If any lifecycle command fails (non‑zero exit), remaining hooks are skipped; error logged and JSON error returned. SIGINT is treated as “interrupted” with a descriptive message.

# 10. Output Specifications

- Standard Output (stdout)
  - Always emits a single JSON line.
  - Success schema:
    - `{ "outcome": "success", "result": "skipNonBlocking"|"prebuild"|"stopForPersonalization"|"done" }`
  - Error schema:
    - `{ "outcome": "error", "message": string, "description": string }`
- Standard Error (stderr)
  - All logs, progress, and diagnostics. `--log-format json` switches to JSON log events.
  - Secrets redacted: any occurrence of secret values is replaced with `********`.
- Exit Codes
  - `0` on success (including `skipNonBlocking`, `prebuild`, `stopForPersonalization`).
  - `1` on error (argument, configuration, or runtime failures).

# 11. Performance Considerations

- Caching Strategy: Remote env probe result cached per `userEnvProbe` mode in `--container-session-data-folder`.
- Parallelization: When a lifecycle command uses object syntax, values run in parallel; output is buffered per command to avoid interleaving.
- Resource Limits: Commands execute under a PTY by default for interactive‑style output; terminal dimensions may be provided to render progress correctly.
- Optimization Opportunities: Reuse probed env/cache across invocations; avoid redundant metadata reads when container state hasn’t changed.

# 12. Security Considerations

- Secrets Handling: Loaded from `--secrets-file` JSON and merged into the execution env; secret values are masked in logs. Keys beginning with `BASH_FUNC_` (FUNC env exports) are filtered.
- Privilege Escalation: This subcommand does not modify system files; hooks run as the configured container user. Dotfiles install runs as the current user. (Contrast: `up` may patch /etc files.)
- Input Sanitization: Substitutions only allow specific variable forms; missing variables either error (no name) or expand to empty/defaults. Docker exec arguments are passed as separate args to avoid shell injection when using array command syntax.
- Container Isolation: Commands run inside the target container. No host file modifications occur except reading secrets from host filesystem.

# 13. Cross-Platform Behavior

| Aspect | Linux | macOS | Windows | WSL2 |
|--------|-------|-------|---------|------|
| Path handling | POSIX paths inside container; host paths resolved with Node path for host OS | Same as Linux | Host path resolution uses Windows semantics; container paths remain POSIX | Host path resolution via Windows; container remains POSIX |
| Docker socket | `/var/run/docker.sock` via Docker Desktop/Engine | Docker Desktop | Docker Desktop | Docker Desktop/WSL integration |
| User ID mapping | N/A for this subcommand; commands run as configured remote user | Same | Same | Same |

# 14. Edge Cases and Corner Cases

- Empty configuration (no hooks): No-ops; returns success.
- Missing container: Bail out with error before execution.
- Circular dependencies in features: Deterministic lifecycleCommandOriginMap order from metadata; executed in order provided by image metadata.
- Network partitions during dotfiles: Error and stop.
- Container exits during hooks: Exec returns non‑zero → error and stop.
- Permission denied on marker folder: Marker update fails → hook considered not runnable (skipped).
- Read-only filesystem: Marker writes fail similarly; hooks may be skipped.

# 15. Testing Strategy

```pseudocode
TEST SUITE: run-user-commands
  TEST "happy path":
    GIVEN container created with lifecycle hooks
    WHEN run-user-commands executes
    THEN outcome == success AND markers exist per hooks

  TEST "config in subfolder":
    GIVEN workspace-folder + config path
    WHEN run-user-commands executes
    THEN success and effects from subfolder config are present

  TEST "invalid workspace":
    GIVEN workspace-folder that does not exist
    WHEN run-user-commands executes
    THEN outcome == error AND message mentions config not found

  TEST "skip-non-blocking":
    GIVEN waitFor set to onCreateCommand
    WHEN run-user-commands --skip-non-blocking-commands
    THEN result == "skipNonBlocking" and subsequent hooks are not run

  TEST "prebuild":
    GIVEN updateContentCommand previously ran
    WHEN run-user-commands --prebuild
    THEN updateContentCommand reruns and result == "prebuild"

  TEST "skip-post-attach":
    WHEN run-user-commands --skip-post-attach
    THEN postAttachCommand markers do not appear

  TEST "secrets injection and masking":
    GIVEN secrets-file with a value also present in logs
    WHEN run-user-commands executes
    THEN hooks can read SECRET envs AND stderr has masked occurrences
END TEST SUITE
```

# 16. Migration Notes

- Deprecated Behavior: None.
- Breaking Changes: N/A for this subcommand compared to reference TypeScript behavior.
- Compatibility Shims: The subcommand mirrors TS CLI output contract: single JSON line to stdout; all logs to stderr; early return modes encoded in `result`.

# Critical Design Decisions (Analysis)

- WaitFor and Non-Blocking Semantics
  - Implementation: `skip-non-blocking-commands` exits after the configured `waitFor` hook runs; returns `"skipNonBlocking"` without running later hooks.
  - Spec Guidance: Lifecycle hooks may be long‑running; CLIs often expose a way to return control earlier. `waitFor` defines the synchronization point.
  - Rationale: Enables fast editor attach and CI prebuild workflows.
  - Alternatives: Fixed early‑exit at a specific hook; less flexible.
  - Trade-offs: Slight complexity in reporting; users must know current `waitFor`.

- Marker Files for Idempotency
  - Implementation: Hook‑specific markers store timestamps; on subsequent runs for the same container state, hooks are skipped.
  - Spec Guidance: Hooks should not rerun unnecessarily.
  - Rationale: Deterministic behavior across restarts and repeated invocations.
  - Alternatives: Store state in labels or volumes.
  - Trade-offs: Requires writable user data folder inside the container.

- Parallel Object Syntax for Commands
  - Implementation: Object form runs values concurrently; each key is a named command. Output is buffered to avoid interleaving.
  - Spec Guidance: Object syntax may indicate parallel steps.
  - Rationale: Faster execution for independent steps; clearer logs via step names.
  - Alternatives: Serialize object values; simpler but slower.
  - Trade-offs: Harder to attribute failures when many run in parallel.

- Secrets Injection and Redaction
  - Implementation: Read secrets from JSON; merge into env; redact values from logs.
  - Spec Guidance: Secrets must not leak into logs.
  - Rationale: Secure by default; supports CI scenarios and local overrides.
  - Alternatives: OS keyring; dynamic prompts.
  - Trade-offs: File handling and redaction cost; users must manage secret files securely.

- Remote Env Probe and Caching
  - Implementation: `userEnvProbe` via login/interactive shell; cached to `container-session-data-folder`.
  - Spec Guidance: Tools should respect shell init files for env.
  - Rationale: Accurate env matching interactive shells; improves subsequent performance.
  - Alternatives: Use `printenv` only; faster but incomplete.
  - Trade-offs: Adds shell dependency and potential flakiness; requires cache invalidation by probe mode.

- Substitution Strategy
  - Implementation: Two‑phase substitution: pre‑container (`${devcontainerId}`), then in‑container (`${containerEnv:VAR}`) plus host/container workspace paths.
  - Spec Guidance: Substitutions are well‑defined and context‑sensitive.
  - Rationale: Ensures correct values whether running outside or inside the container.
  - Alternatives: Single pass after container; loses local env semantics.
  - Trade-offs: More code paths; but predictable results.

- JSON Result and stderr Logs
  - Implementation: Single JSON line on stdout; everything else on stderr; exit code reflects success/failure only.
  - Spec Guidance: Programmatic consumption should be clean and stable.
  - Rationale: Machine‑readable results while keeping human logs intact.
  - Alternatives: Mixed stdout/stderr or multi‑line JSON; harder to consume.
  - Trade-offs: Requires consumers to read stdout only for results.

