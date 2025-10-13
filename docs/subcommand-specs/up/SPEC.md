# Up Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Provision a development container from a devcontainer configuration, creating or reusing a container, applying Features and configuration, running lifecycle commands, and returning connection details.
- User Personas:
  - Application developers: Start a project in a dev container using `devcontainer.json` or Compose.
  - CI/Automation: Validate or prebuild images and run lifecycle commands in a headless environment.
  - Tooling integrators: Launch and connect to an environment to run additional tasks (e.g., tests).
- Specification References:
  - Development Containers Spec: Configuration, Lifecycle Scripts, Features, Mounts, Environment, Ports, User/UID, and Compose sections.
- Related Commands:
  - `set-up`: Convert an existing container into a dev container with the same lifecycle orchestration.
  - `build`: Build an image according to devcontainer configuration (mirrors a subset of up’s build pipeline).
  - `run-user-commands`: Execute lifecycle hooks separately (subset of what up runs when not skipped).
  - `exec`: Execute commands in a running container started by `up`.

## 2. Command-Line Interface
- Full Syntax:
  - devcontainer up [options]
- Flags and Options:
  - Docker/Compose paths and data:
    - --docker-path <string>
    - --docker-compose-path <string>
    - --container-data-folder <string>
    - --container-system-data-folder <string>
  - Workspace and config selection:
    - --workspace-folder <string>
    - --config <path-to-devcontainer.json>
    - --override-config <path-to-devcontainer.json>
    - --id-label <name=value> (repeatable)
    - --mount-workspace-git-root [boolean, default true]
  - Logging/terminal:
    - --log-level <info|debug|trace> (default info)
    - --log-format <text|json> (default text)
    - --terminal-columns <number> (implies --terminal-rows)
    - --terminal-rows <number> (implies --terminal-columns)
  - Runtime behavior:
    - --remove-existing-container [boolean, default false]
    - --build-no-cache [boolean, default false]
    - --expect-existing-container [boolean, default false]
    - --workspace-mount-consistency <consistent|cached|delegated> (default cached)
    - --gpu-availability <all|detect|none> (default detect)
    - --default-user-env-probe <none|loginInteractiveShell|interactiveShell|loginShell> (default loginInteractiveShell)
    - --update-remote-user-uid-default <never|on|off> (default on)
  - Lifecycle control:
    - --skip-post-create [boolean, default false] (skips onCreate/updateContent/postCreate/postStart/postAttach and dotfiles)
    - --skip-non-blocking-commands [boolean, default false]
    - --prebuild [boolean, default false] (stop after onCreate and updateContent; rerun updateContent if already ran)
    - --skip-post-attach [boolean, default false]
  - Additional mounts/env/cache/build:
    - --mount "type=<bind|volume>,source=<source>,target=<target>[,external=<true|false>]" (repeatable)
    - --remote-env <name=value> (repeatable)
    - --cache-from <string> (repeatable)
    - --cache-to <string>
    - --buildkit <auto|never> (default auto)
  - Features/dotfiles/metadata:
    - --additional-features <json>
    - --skip-feature-auto-mapping [boolean, default false]
    - --dotfiles-repository <url>
    - --dotfiles-install-command <string>
    - --dotfiles-target-path <path, default ~/dotfiles>
    - --container-session-data-folder <path>
    - --omit-config-remote-env-from-metadata [boolean]
    - --experimental-lockfile [boolean]
    - --experimental-frozen-lockfile [boolean]
    - --omit-syntax-directive [boolean]
  - Output shaping:
    - --include-configuration [boolean]
    - --include-merged-configuration [boolean]
    - --user-data-folder <path> (persisted host state)
  - Deprecated: none (TS reference has no deprecations for up).
- Validation Rules:
  - At least one of: --workspace-folder or --id-label is required.
  - At least one of: --workspace-folder or --override-config is required.
  - --mount must match regex: type=(bind|volume),source=([^,]+),target=([^,]+)(,external=(true|false))?
  - --remote-env must match: <name>=<value>
  - --terminal-columns implies --terminal-rows and vice versa.
  - Mutually-influencing flags:
    - --expect-existing-container prevents building/creating a new one; errors if missing.
    - --remove-existing-container forces removal before (re)create.
    - --skip-post-create and --prebuild are mutually shaping lifecycle execution order; see lifecycle section.

## 3. Input Processing Pipeline
```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    REQUIRE args.workspace_folder OR args.id_label
    REQUIRE args.workspace_folder OR args.override_config

    VALIDATE each args.mount matches mountRegex
    VALIDATE each args.remote_env matches name=value

    NORMALIZE:
        addRemoteEnvs := toArray(args.remote_env)
        addCacheFroms := toArray(args.cache_from)
        additionalFeatures := parseJSON(args.additional_features) OR {}
        providedIdLabels := toArray(args.id_label) OR undefined
        workspaceFolder := resolvePath(cwd, args.workspace_folder) OR undefined

    RETURN ParsedInput with normalized arrays, booleans, and resolved paths
END FUNCTION
```

## 4. Configuration Resolution
- Sources (highest to lowest precedence):
  - Command-line flags (e.g., additional mounts, remote-env, lifecycle gating flags)
  - Environment variables (e.g., COMPOSE_PROJECT_NAME)
  - Configuration files: `devcontainer.json` or `.devcontainer/devcontainer.json`, `override-config`, and image/Feature metadata
  - Default values
- Resolution Algorithm:
```pseudocode
FUNCTION resolve_configuration(params, configFile, overrideConfig, providedIdLabels, additionalFeatures):
    IF configFile specified AND file name not devcontainer.json/.devcontainer.json:
        ERROR "Filename must be devcontainer.json or .devcontainer.json"

    workspace := workspaceFromPath(params.cliHost.path, params.cliHost.cwd OR workspaceFolder)
    candidateConfigPath :=
        IF configFile: configFile
        ELSE IF workspace: getDevContainerConfigPathIn(workspace.configFolder)
             OR (overrideConfig ? getDefaultDevContainerConfigPath(workspace.configFolder) : undefined)
        ELSE: overrideConfig

    configs := readDevContainerConfigFile(cliHost, workspace, candidateConfigPath, params.mountWorkspaceGitRoot, params.output, params.workspaceMountConsistencyDefault, overrideConfig)
    IF !configs: ERROR with explicit description depending on missing workspace/config

    idLabels := providedIdLabels OR findContainerAndIdLabels(params, workspace.rootFolderPath, configPath)
    configWithRaw := addSubstitution(configs.config, beforeContainerSubstitute(envListToObj(idLabels), ...))

    ENSURE no disallowed Features for config + additionalFeatures
    RUN initializeCommand if present (runs before any container start)

    IF config is Dockerfile/image-based:
        RETURN openDockerfileDevContainer(...)
    ELSE IF config is docker-compose-based:
        RETURN openDockerComposeDevContainer(...)
END FUNCTION
```
- Variable Substitution Rules:
  - Evaluated with: platform, localWorkspaceFolder, containerWorkspaceFolder, configFile path, and host env.
  - Applies to all string fields including mounts, env, paths, and lifecycle command strings.

## 5. Core Execution Logic
```pseudocode
FUNCTION execute_up(parsed_input) -> ExecutionResult:
    // Phase 1: Initialization
    params := createDockerParams(parsed_input)
    output.start("Resolving Remote")

    // Phase 2: Resolve configuration and container target
    result := resolve(params, parsed_input.configFile, parsed_input.overrideConfigFile, parsed_input.providedIdLabels, parsed_input.additionalFeatures)
    output.stop("Resolving Remote")

    // Phase 3: Main execution (performed inside resolve/open* functions)
    // - For Dockerfile/Image:
    //   1) findExistingContainer(idLabels); start if present; else build image (respect BuildKit, cache, extra features)
    //   2) updateRemoteUserUID if required/allowed
    //   3) run docker run with id labels, mounts, env, user, entrypoint, security opts, init
    //   4) inspect container and compute ContainerProperties
    //   5) merge image metadata with devcontainer config
    //   6) setupInContainer: lifecycle hooks, customizations, dotfiles, remote env, secrets, probe user env
    // - For Compose:
    //   1) resolve compose config (collect files, .env, profiles) and project name
    //   2) find container by project/service; remove if requested; create with `compose up -d`
    //   3) read image metadata from container and merge
    //   4) compute ContainerProperties
    //   5) setupInContainer (same as above)

    // Phase 4: Post-execution
    RETURN object including containerId, composeProjectName (if compose), remoteUser, remoteWorkspaceFolder, and optional configuration blobs
END FUNCTION
```

## 6. State Management
- Persistent State:
  - Host-side: `--user-data-folder`, `--container-session-data-folder` used for caching CLI data (e.g., userEnvProbe results), temporary build artifacts, and lockfiles if enabled.
  - Image metadata: Labels carry configuration and Feature provenance; merged into runtime config.
- Caching:
  - Build cache via Docker/BuildKit; `--cache-from` and `--cache-to` supported; optional `--no-cache` for build.
  - Features temp/cache folder used to construct extended images and BuildKit contexts.
- Locks:
  - Experimental lockfile support for Features (write/frozen) controls Feature resolution determinism.
- Idempotency:
  - Running `up` multiple times reuses a running container when possible.
  - `--remove-existing-container` resets/recreates; `--expect-existing-container` fails if absent.

## 7. External System Interactions
- Docker/Container Runtime
```pseudocode
FUNCTION interact_with_docker(op) -> Result:
    CASE op.type OF
        'inspect-container': docker inspect <id>
        'list-containers': docker ps --filter label=<...>
        'remove-container': docker rm -f <id>
        'run-container': docker run [labels, mounts, env, user, entrypoint, security opts, init]
        'build-image':
            IF BuildKit enabled: docker buildx build [..., --cache-from, --cache-to, --platform, --push?]
            ELSE: docker build [...]
        'compose-config': docker compose [-f files...] [--env-file] config
        'compose-up': docker compose [-f files...] up -d [--profile *]
        'compose-project-name': env COMPOSE_PROJECT_NAME or infer from .env or files
        'logs/exec': docker exec / logs
    HANDLE differences when TTY is available vs non-TTY, and Windows Unicode fallback via PTY
END FUNCTION
```
- OCI Registries
  - When Features extend images, the CLI fetches metadata and may pull base images; authentication and registry interactions follow the Features and image metadata logic (see containerCollectionsOCI and httpOCIRegistry). Caching and lockfiles apply per Feature resolution.
- File System
  - Reads devcontainer files, .env (Compose), Dockerfiles, and writes temporary Dockerfiles for UID update or Features.
  - Creates temp folders for features/build contexts, respects host path semantics and WSL path translation when needed.

## 8. Data Flow Diagrams
```
┌──────────────┐      ┌────────────────────┐      ┌──────────────────────┐
│ CLI Options  │ ───▶ │ Parse & Normalize  │ ───▶ │ Create Docker Params │
└─────┬────────┘      └─────────┬──────────┘      └─────────┬────────────┘
      │                         │                            │
      │                         ▼                            ▼
      │                  ┌──────────────┐              ┌───────────────┐
      │                  │ Resolve Conf │─────────────▶│ Dockerfile/   │
      │                  └──────┬───────┘              │ Compose Flow  │
      │                         │                      └──────┬────────┘
      │                         ▼                             │
      │                  ┌──────────────┐                     │
      │                  │ Merge Config │◀──── Image Labels ──┘
      │                  └──────┬───────┘
      │                         ▼
      │                  ┌──────────────┐
      │                  │ setupInCont. │ (lifecycle, dotfiles, env)
      │                  └──────┬───────┘
      │                         ▼
      │                  ┌──────────────┐
      └─────────────────▶│  Up Result   │ (ids, user, folder, opts)
                         └──────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Missing/invalid args, malformed mounts or remote-env, missing config: outcome=error, exit code 1, descriptive message.
  - Invalid devcontainer.json shape: descriptive config error.
- System Errors:
  - Docker unavailable, Compose errors, image build failures (including BuildKit policy issues), Unicode rendering on Windows Compose v1: switch to PTY fallback; surface ContainerError with description and original error payload.
  - GPU requested but unsupported: warn and continue if optional; otherwise note limitation.
- Configuration Errors:
  - Disallowed Features: error includes offending feature id.
  - Lifecycle command failure: prints progress and error; stops subsequent user commands and fails `up`.

## 10. Output Specifications
- Standard Output (stdout):
  - JSON object per run. See Data Structures for schema.
  - Includes fields: outcome, containerId, composeProjectName (if compose), remoteUser, remoteWorkspaceFolder, optional configuration and merged configuration when requested; error fields include message, description, containerId (if available), disallowedFeatureId, didStopContainer, learnMoreUrl.
- Standard Error (stderr):
  - Logs and progress (text or JSON) according to --log-format and --log-level.
- Exit Codes:
  - 0: success
  - 1: failure (any outcome=error)

## 11. Performance Considerations
- Caching Strategy:
  - Utilize Docker build cache, BuildKit cache-from/to, and temp directories for features to avoid redundant work.
- Parallelization:
  - Lifecycle parallel command blocks (object form) execute commands concurrently with output buffering to avoid interleaving noise.
  - Background tasks can outlive main setup; finishBackgroundTasks is awaited before dispose when outcome is success.
- Resource Limits:
  - Respect host requirements (cpus, memory, storage, gpu) merged from metadata; `gpu-availability` constrains capability usage.
- Optimizations:
  - Prefer reusing running container; only rebuild when needed.
  - Avoid extra image tagging when no features need extension.

## 12. Security Considerations
- Secrets Handling:
  - Read via `--secrets-file` (mapped into env for lifecycle when executing) and redacted in logs.
- Privilege/Capabilities:
  - Apply `init`, `privileged`, `capAdd`, `securityOpt` from merged configuration; ensure least privilege by default.
- Input Sanitization:
  - Validate mount and env formats; escape env values for Compose YAML and docker args; prevent injection via quoting.
- Isolation:
  - Containers run with user/UID mapping per configuration and optional UID update to align host/container users.

## 13. Cross-Platform Behavior
- Linux: Native paths, Docker socket at /var/run/docker.sock; UID/GID mapping may update images.
- macOS: Same as Linux; UID update path commonly used; BuildKit policy checks considered.
- Windows: Path handling via URI helpers; Compose v1 Unicode issue triggers PTY fallback when parsing `compose config`.
- WSL2: Translate file URIs to WSL paths for Docker context and Dockerfile; host type influences CLI exec choice.

## 14. Edge Cases and Corner Cases
- Empty or missing configuration files: explicit error.
- Circular `extends` or invalid schema: validation error.
- Network partitions: build/pull may fail; upstream docker errors surfaced.
- Container exits during lifecycle: lifecycle failure error with progress trace.
- Permission denied: when writing temp files, creating volumes, or exec as non-root; surfaces as ContainerError.
- Read-only FS: mounting volumes or writing metadata fails; error.

## 15. Testing Strategy
```pseudocode
TEST SUITE "up":
  TEST "happy path image": config with base image -> up succeeds; containerId present
  TEST "happy path features": config with features -> up succeeds
  TEST "compose image": minimal compose service -> up succeeds; composeProjectName derived
  TEST "compose Dockerfile": service with build -> up succeeds
  TEST "missing config": invalid workspace path -> outcome error, exit 1
  TEST "mount format invalid": bad mount string -> argument validation error
  TEST "remote-env format invalid": bad env string -> argument validation error
  TEST "skip-post-create": lifecycle hooks skipped and dotfiles not installed
  TEST "prebuild": stops after onCreate/updateContent; reruns updateContent if already ran
  TEST "remove-existing": remove running container then recreate
  TEST "expect-existing": fails when container not present
  TEST "include-config": result includes configuration and mergedConfiguration blobs
END
```

## 16. Migration Notes
- Deprecated Behavior: none.
- Breaking Changes: none intended; follow reference behavior.
- Compatibility: Compose v2 profile handling and PTY fallback for Windows Compose v1 are implemented for parity.

