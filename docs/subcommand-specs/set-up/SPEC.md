# Set-Up Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Convert an already-running container into a Dev Container by applying devcontainer configuration and image metadata at runtime, executing lifecycle hooks, injecting remote environment, optionally installing dotfiles, and returning updated configuration snapshots.
- User Personas:
  - Developers: Attach tooling to an existing container started by other means (e.g., docker run) and bring it under Dev Container lifecycle without rebuilding.
  - CI/Automation: Prepare ephemeral containers to run project tasks with the same lifecycle hooks used by up/build flows.
  - Platform integrators: Run lifecycle scripts against arbitrary containers for personalization.
- Specification References:
  - Development Containers Spec: Lifecycle Scripts, Environment, Variables & Substitution, Features metadata, Remote User and UID, Customizations.
  - See containers.dev implementors spec for: devcontainer.json properties, lifecycle ordering, variable expansion semantics, and image metadata conventions.
- Related Commands:
  - `up`: Builds/creates the container and then performs setup (set-up is a subset that assumes the container exists).
  - `run-user-commands`: Executes lifecycle commands like set-up, but across more sources and with additional flags (e.g., secrets-file) and discovery modes.
  - `read-configuration`: Returns the resolved/merged configuration without executing lifecycle hooks.
  - `exec`: Executes one-off commands inside a running container once it has been set up.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer set-up --container-id <id> [options]`
- Flags and Options:
  - Container and config:
    - `--container-id <string>` (required): Target container id.
    - `--config <path>`: Optional path to a devcontainer.json; used to augment/override metadata embedded in the image.
  - Logging/terminal:
    - `--log-level <info|debug|trace>` (default info)
    - `--log-format <text|json>` (default text)
    - `--terminal-columns <number>` (implies `--terminal-rows`)
    - `--terminal-rows <number>` (implies `--terminal-columns`)
  - Lifecycle control:
    - `--skip-post-create` [boolean, default false]: Skip all lifecycle hooks (onCreate, updateContent, postCreate, postStart, postAttach) and dotfiles installation.
    - `--skip-non-blocking-commands` [boolean, default false]: Stop after the configured `waitFor` hook (default updateContent).
  - Environment and dotfiles:
    - `--remote-env <name=value>` (repeatable): Extra remote env to inject when running hooks.
    - `--dotfiles-repository <url or owner/repo>`
    - `--dotfiles-install-command <string>`
    - `--dotfiles-target-path <path>` (default `~/dotfiles`)
    - `--container-session-data-folder <path>`: Cache location inside container for probes (e.g., userEnvProbe cache).
  - Data folders:
    - `--user-data-folder <path>`: Host persisted state (used indirectly by logging/caching).
    - `--container-data-folder <path>`: Inside-container user data root (default `~/.devcontainer`).
    - `--container-system-data-folder <path>`: Inside-container system data root (default `/var/devcontainer`).
  - Output shaping:
    - `--include-configuration` [boolean]: Include updated configuration in result.
    - `--include-merged-configuration` [boolean]: Include merged configuration in result.
  - Docker path:
    - `--docker-path <string>`: Path to the Docker CLI.
- Flag Taxonomy:
  - Required: `--container-id`.
  - Optional: All others.
  - Mutually influencing pairs:
    - `--terminal-columns` implies `--terminal-rows` and vice versa.
    - `--skip-post-create` disables lifecycle hooks and dotfiles entirely.
  - Deprecated: none.
- Argument Validation Rules:
  - `--remote-env` must match `<name>=<value>`.
  - `--terminal-columns` requires `--terminal-rows`; `--terminal-rows` requires `--terminal-columns`.
  - `--config` path must resolve to a file if provided; otherwise: error “Dev container config (<path>) not found.”

## 3. Input Processing Pipeline
```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    REQUIRE args.container_id
    VALIDATE each args.remote_env matches /.+=.*/
    VALIDATE terminal dims: columns implies rows, rows implies columns

    NORMALIZE:
        addRemoteEnvs := toArray(args.remote_env)
        configFile := args.config ? fs.resolveCwd(args.config) : undefined

    RETURN {
        containerId: args.container_id,
        configFile,
        logLevel: mapLogLevel(args.log_level),
        logFormat: args.log_format,
        terminal: { columns: args.terminal_columns, rows: args.terminal_rows }?
        lifecycle: {
            postCreateEnabled: !args.skip_post_create,
            skipNonBlocking: args.skip_non_blocking_commands,
        },
        remoteEnv: parseEnvList(addRemoteEnvs),
        dotfiles: { repository, installCommand, targetPath },
        containerDataFolder: args.container_data_folder,
        containerSystemDataFolder: args.container_system_data_folder,
        containerSessionDataFolder: args.container_session_data_folder,
        includeConfig: args.include_configuration,
        includeMergedConfig: args.include_merged_configuration,
        dockerPath: args.docker_path,
        userDataFolder: args.user_data_folder,
    }
END FUNCTION
```

## 4. Configuration Resolution
- Configuration Sources (precedence high → low):
  1) Command-line `--config` file (when provided).
  2) Image metadata embedded in the running container (Features and prior devcontainer settings).
  3) Default values.
- Merge Algorithm:
  - If `--config` provided, read and parse devcontainer.json into `config0`.
  - Derive `imageMetadata` by inspecting the container’s image labels and Features.
  - Compute `mergedConfig = mergeConfiguration(config.config, imageMetadata)`:
    - Replace properties become collected fields: `entrypoints`, `onCreateCommands`, `updateContentCommands`, `postCreateCommands`, `postStartCommands`, `postAttachCommands`.
    - Merge boolean and array properties using union/override rules from metadata.
    - Merge `remoteEnv` and env-like maps via shallow map overlay from metadata order (later entries override earlier).
- Variable Substitution:
  - Pre-container substitution for `${devcontainerId}` is not used in set-up (no id labels provided for this flow).
  - Container substitution is applied to both config and merged config using container environment:
    - `${containerEnv:VAR}` → value from target container env (case-insensitive on Windows-like contexts).
    - `${env:VAR}`/`${localEnv:VAR}` are not used in set-up post-merge, but may be present in `--config` and are expanded during initial read if needed.
  - Errors: Missing variable name in substitution raises an error pointing at the config file if available.

## 5. Core Execution Logic
```pseudocode
FUNCTION execute_set_up(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    params := createDockerParams({
        dockerPath: parsed.dockerPath,
        containerDataFolder: parsed.containerDataFolder,
        containerSystemDataFolder: parsed.containerSystemDataFolder,
        containerSessionDataFolder: parsed.containerSessionDataFolder,
        configFile: parsed.configFile,
        logLevel: parsed.logLevel,
        logFormat: parsed.logFormat,
        terminalDimensions: parsed.terminal,
        defaultUserEnvProbe: default, // overridable by flag
        postCreateEnabled: parsed.lifecycle.postCreateEnabled,
        skipNonBlocking: parsed.lifecycle.skipNonBlocking,
        remoteEnv: parsed.remoteEnv,
        persistedFolder: parsed.userDataFolder,
        dotfiles: parsed.dotfiles,
    })

    // Phase 2: Container discovery and config
    container := docker.inspect(parsed.containerId)
    IF not container THEN bailOut('Dev container not found.')

    config0 := parsed.configFile ? readConfig(parsed.configFile) : { raw: {}, config: {}, substitute: substitute(localCtx, v) }
    config := addSubstitution(config0, beforeContainerSubstitute(undefined, ·))
    imageMetadata := getImageMetadataFromContainer(container, config)
    mergedConfig := mergeConfiguration(config.config, imageMetadata)
    containerProps := createContainerProperties(params, container.Id, undefined, mergedConfig.remoteUser)

    // Phase 3: Main execution (inside container)
    // 3a) System patching (once per container)
    patchEtcEnvironment(params, containerProps)
    patchEtcProfile(params, containerProps)

    // 3b) Variable substitution using live container env
    updatedConfig := containerSubstitute(platform, config.configFilePath, containerProps.env, config)
    updatedMerged := containerSubstitute(platform, mergedConfig.configFilePath, containerProps.env, mergedConfig)

    // 3c) Lifecycle execution & dotfiles
    IF lifecycleHook.enabled THEN
        remoteEnvP := probeRemoteEnv(params, containerProps, updatedMerged) // userEnvProbe + remoteEnv merge
        secretsP := {} // set-up does not accept --secrets-file
        runLifecycleHooks(params, lifecycleCommandOriginMapFromMetadata(imageMetadata), containerProps, updatedMerged, remoteEnvP, secretsP, stopForPersonalization=false)
    ENDIF

    // Phase 4: Post-execution — return updated configs
    RETURN Success({
        configuration: parsed.includeConfig ? updatedConfig : undefined,
        mergedConfiguration: parsed.includeMergedConfig ? updatedMerged : undefined,
    })
END FUNCTION
```

## 6. State Management
- Persistent State Inside Container:
  - User data folder: `getUserDataFolder(home, params)` → `~/.devcontainer` by default or `--container-data-folder`.
  - System var folder: `/var/devcontainer` by default or `--container-system-data-folder`.
  - Marker files used to ensure idempotency:
    - `~/.devcontainer/.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker`, `.postStartCommandMarker` — control re-run behavior per container lifecycle.
    - `/var/devcontainer/.patchEtcEnvironmentMarker`, `.patchEtcProfileMarker` — ensure `/etc/environment` and `/etc/profile` patches only run once.
    - Dotfiles marker at target path — prevents re-installation.
  - User env probe cache under `--container-session-data-folder` when provided.
- Cache Management:
  - Environment probe results cached for subsequent runs; invalidated on container changes.
- Lock Files:
  - Not used by set-up (Feature lockfiles apply to build flows).
- Idempotency:
  - Safe to re-run; markers prevent repeating heavy operations (patches, dotfiles, lifecycle hooks unless timestamps change or rerun is requested via other commands).

## 7. External System Interactions

### Docker/Container Runtime
```pseudocode
FUNCTION interact_with_docker(operation) -> Result:
    CASE operation.type OF
        'inspect-container':
            RUN: docker inspect <containerId>
        'exec-root':
            RUN: docker exec -i [-t] -u root <id> <cmd>
        'exec-user':
            RUN: docker exec -i [-t] -u <remoteUser|default> <id> <cmd>
        'stream-logs':
            Capture stdout/stderr from exec to CLI log
    HANDLE PTY allocation only when stdin is TTY; otherwise non-PTY path
END FUNCTION
```
- Commands executed during set-up:
  - Launch shell server for user/root as needed (multiplexed `docker exec`).
  - Patch `/etc/environment` by appending container env pairs; Patch `/etc/profile` to preserve PATH on login shells.
  - Execute lifecycle hooks in order: onCreate, updateContent, postCreate, postStart, postAttach (subject to `waitFor` and `skip-non-blocking-commands`).
  - Clone and run dotfiles scripts if configured.

### File System
- Reads: optional devcontainer.json from `--config`.
- Writes (inside container): marker files, dotfiles folder, temporary scripts via shell heredocs.

## 8. Data Flow Diagrams
```
┌──────────────┐      ┌────────────────────┐      ┌──────────────────────┐
│ CLI Options  │ ───▶ │ Parse & Normalize  │ ───▶ │ Create Docker Params │
└─────┬────────┘      └─────────┬──────────┘      └─────────┬────────────┘
      │                         │                            │
      │                         ▼                            ▼
      │                  ┌──────────────┐              ┌───────────────┐
      │                  │ Inspect Ctr  │─────────────▶│ ContainerProps │
      │                  └──────┬───────┘              └──────┬────────┘
      │                         │                             │
      │                         ▼                             │
      │                  ┌──────────────┐                     │
      │                  │ Read Config  │                     │
      │                  └──────┬───────┘                     │
      │                         ▼                             │
      │                  ┌──────────────┐                     │
      │                  │ Merge Config │◀──── Image Labels ──┘
      │                  └──────┬───────┘
      │                         ▼
      │                  ┌──────────────┐
      │                  │ setupInCont. │ (patches, env, hooks, dotfiles)
      │                  └──────┬───────┘
      │                         ▼
      │                  ┌──────────────┐
      └─────────────────▶│  JSON Result │ (updated configs)
                         └──────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Missing `--container-id` → argument validation error.
  - Invalid `--remote-env` format → argument validation error.
  - `--config` path not found → ContainerError with description “Dev container config (<path>) not found.”
- System Errors:
  - Container not found (`docker inspect` fails) → bail out “Dev container not found.”
  - Lifecycle command failure → error with message and combined stdout/stderr; subsequent lifecycle commands stop.
  - Root operations failing (e.g., patching `/etc/*`) → best-effort; continues when markers cannot be written, but logs warnings; does not abort set-up unless critical.
- Output/Exit:
  - stdout contains a single JSON line with `outcome` field; stderr contains logs according to `--log-format`.
  - Exit code 0 on success; 1 on error.

## 10. Output Specifications
- Success (stdout JSON):
  - `{ "outcome": "success", "configuration"?: object, "mergedConfiguration"?: object }`
  - Fields included only when `--include-configuration` / `--include-merged-configuration` are set.
- Error (stdout JSON):
  - `{ "outcome": "error", "message": string, "description": string }`
- Notes:
  - Unlike `up`, set-up does not include `containerId` in the JSON result.
  - Logs/progress are emitted to stderr and adhere to log level/format.

## 11. Performance Considerations
- Minimize container execs by launching a persistent shell server for the user and root when necessary.
- Cache user environment probe results under `--container-session-data-folder`.
- Lifecycle commands may run multiple steps; object syntax enables parallelism with buffered output per step.
- Marker files prevent repeated work across invocations.

## 12. Security Considerations
- No secrets file in set-up; only remote env from CLI is injected to lifecycle.
- Root operations are limited to minimal `/etc` patching guarded by a system marker file.
- Dotfiles cloning executes code inside the container; respect marker, log commands, and avoid exposing secrets.
- Variable substitution only expands `${containerEnv:...}` within container context to avoid leaking host env.

## 13. Cross-Platform Behavior
- Linux/macOS: Standard docker exec behavior; PTY allocated only when stdin is a TTY.
- Windows hosts: Handled by Docker Desktop; path handling uses URI utilities in config read path; inside-container behavior remains POSIX (`/bin/sh`).
- WSL: File path resolution for `--config` uses host FS semantics; inside-container remains Linux.

## 14. Edge Cases and Corner Cases
- Container started without `PATH` → set-up still computes env; PATH patcher preserves existing values.
- Missing HOME or non-writable home → home detection falls back to `/root` unless user is root or writable home.
- Lifecycle hooks absent → no-ops; set-up remains success.
- `--skip-post-create` → skips hooks and dotfiles; still performs `/etc` patches and returns updated substitutions.
- Remote user from metadata differs from container user → createContainerProperties will exec with that user; fallback to root when necessary for system patches.

## 15. Testing Strategy
```pseudocode
TEST SUITE "set-up":
  TEST "config postAttachCommand": run container, set-up with --config; verifies marker file created by postAttach
  TEST "metadata postCreateCommand": image label carries lifecycle; verifies marker file created by postCreate
  TEST "include-config": set-up returns configuration and mergedConfiguration blobs
  TEST "remote-env substitution": container env TEST_CE propagates to merged "TEST_RE"
  TEST "invalid remote-env": argument validation error
  TEST "skip-post-create": set-up completes without running lifecycle and without dotfiles
  TEST "dotfiles install": with repository provided, marker prevents reinstall; respects explicit install command
END
```

## 16. Design Decisions (Critical Analysis)

#### Design Decision: Execute lifecycle hooks against an existing container
Implementation Behavior:
- Hooks run in order (onCreate → updateContent → postCreate → postStart → postAttach) using a persistent shell server, gated by markers and `waitFor` with `--skip-non-blocking-commands` semantics.
Specification Guidance:
- The implementors spec defines lifecycle hooks and their execution order; allows implementation to choose idempotency mechanisms.
Rationale:
- Align with TS behavior and ensure repeated set-up runs don’t duplicate work; markers ensure idempotency.
Alternatives Considered:
- Track hook completion only via labels; rejected because runtime events (postStart/postAttach) are per-container session.
Trade-offs:
- Marker files are simple and robust but live inside the container filesystem.

#### Design Decision: Variable substitution via `${containerEnv:VAR}`
Implementation Behavior:
- Perform a second substitution pass inside the container using current env to resolve containerEnv placeholders across configuration.
Specification Guidance:
- Variable substitution allows container and host env expansions; containerEnv must reflect the runtime container environment.
Rationale:
- Users expect `remoteEnv` and other settings to inherit live container env (e.g., variables from `docker run -e`).
Alternatives Considered:
- Resolve strictly from image metadata or host env; rejected because it would miss runtime-injected vars.
Trade-offs:
- Requires early shell server exec to detect env; negligible overhead mitigated by caching.

#### Design Decision: Patch `/etc/environment` and `/etc/profile`
Implementation Behavior:
- One-time, root-only patch with markers to append env pairs and preserve PATH handling in login shells.
Specification Guidance:
- Spec does not mandate patching system files; allowed to adjust for environment parity.
Rationale:
- Improve consistency so shells launched later inherit expected PATH and env entries.
Alternatives Considered:
- Only inject env for lifecycle exec; rejected due to mismatch with interactive shells.
Trade-offs:
- Requires root access; falls back gracefully when unavailable.

#### Design Decision: Dotfiles integration
Implementation Behavior:
- Clone repository to target path, auto-detect installer (`install.sh`, `bootstrap`, `setup`, including `script/*`) or run explicit command, with a marker to avoid reinstall.
Specification Guidance:
- Dotfiles are optional; implementations may integrate a convention-based installer.
Rationale:
- Matches reference CLI for portability of developer personalization.
Alternatives Considered:
- Host-side install or VS Code-only integration; not portable for headless CLI.
Trade-offs:
- Executes arbitrary repo code; bounded by container isolation and logs.

#### Design Decision: No `--secrets-file` in set-up
Implementation Behavior:
- Secrets are not part of set-up; remote env can be passed from CLI; secrets default to empty.
Specification Guidance:
- Spec does not require secrets support for set-up; run-user-commands covers this use case.
Rationale:
- Keep set-up minimal and focused; avoid introducing new inputs for an existing container path.
Alternatives Considered:
- Accept `--secrets-file`; deferred to run-user-commands for parity with TS CLI.
Trade-offs:
- Users needing secrets should prefer `run-user-commands`.

#### Design Decision: Result schema excludes container id
Implementation Behavior:
- Success JSON includes only updated configs when requested.
Specification Guidance:
- Spec does not constrain CLI output; consistent with reference CLI tests.
Rationale:
- Keep output minimal; container id is already known by the caller who provided it.
Alternatives Considered:
- Include container id; unnecessary duplication.
Trade-offs:
- None significant.

