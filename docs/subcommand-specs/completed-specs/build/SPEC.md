# Build Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Build a Dev Container image from a devcontainer configuration by resolving configuration, applying Features, embedding image metadata labels, and optionally retagging or exporting via BuildKit.
- User Personas:
  - Application developers: Prebuild images to speed up `up` workflows and CI runs.
  - CI/Automation engineers: Produce multi-arch images, push to registries, export OCI archives, and prime caches.
  - Tooling integrators: Generate images with Features/metadata for consumption by other tools.
- Specification References:
  - Dev Container configuration and properties: containers.dev implementors spec (features, lifecycle, mounts, env, user, containerEnv, labels)
  - Dev Container Features distribution and semantics: implementors/features
  - Image metadata label schema: implementors/spec (devcontainer metadata label)
  - Docker Compose considerations: implementors/compose
- Related Commands:
  - `up`: May build when needed and then run the container. `build` mirrors its build pipeline without running containers.
  - `read-configuration`: Parses configuration that `build` consumes.
  - `run-user-commands`/`set-up`: Use a running container; not directly related to `build` but align in metadata semantics.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer build [path] [options]`
  - Preferred form: `devcontainer build --workspace-folder <path> [options]`
- Flags and Options:
  - Paths and tooling:
    - `--user-data-folder <path>`: Persisted host state/cache folder.
    - `--docker-path <string>`: Docker CLI path (default `docker`).
    - `--docker-compose-path <string>`: Docker Compose CLI path (default `docker-compose`, with v2 autodetection via `docker compose`).
    - `--workspace-folder <path>`: Project root used to locate devcontainer config.
    - `--config <path>`: Explicit devcontainer.json path. Must be named `devcontainer.json` or `.devcontainer.json`.
  - Logging:
    - `--log-level <info|debug|trace>` (default `info`)
    - `--log-format <text|json>` (default `text`)
  - Build behavior:
    - `--no-cache` (boolean): Build without cache.
    - `--image-name <name[:tag]>` (repeatable): Final image name(s)/tags to apply. Default derived if omitted. **[IMPLEMENTED]**
    - `--cache-from <ref>` (repeatable): Additional cache sources (BuildKit only; ignored if `--no-cache`).
    - `--cache-to <spec>`: Buildx cache export destination (BuildKit only; not supported with Compose).
    - `--buildkit <auto|never>` (default `auto`): Build with BuildKit/buildx or legacy build.
    - `--platform <os/arch[/variant]>`: Target platforms (BuildKit only; not supported with Compose).
    - `--push` (boolean): Push built image to registry (BuildKit only; not supported with Compose). **[IMPLEMENTED]**
    - `--output <spec>`: Override build output (e.g., `type=oci,dest=out.tar`) (BuildKit only; mutually exclusive with `--push`). **[IMPLEMENTED]**
    - `--label <name=value>` (repeatable): Add image metadata labels to builds. **[IMPLEMENTED]**
  - Features and metadata:
    - `--additional-features <json>`: JSON object per `features` schema to merge with config.
    - `--skip-feature-auto-mapping` (boolean, hidden): Testing toggle; bypasses auto feature mapping.
    - `--skip-persisting-customizations-from-features` (boolean, hidden): Do not persist `customizations` from Features into image metadata label.
    - `--experimental-lockfile` (boolean, hidden): Write feature lockfile.
    - `--experimental-frozen-lockfile` (boolean, hidden): Fail if lockfile changes would occur.
    - `--omit-syntax-directive` (boolean, hidden): Omit Dockerfile `# syntax=` directive workaround.
- Flag Taxonomy:
  - Required: One of `--workspace-folder` or positional `[path]` (positional is accepted by CLI but build resolution uses `--workspace-folder` in practice).
  - Optional: All others.
  - Mutually exclusive / constrained:
    - `--output` cannot be combined with `--push`.
    - `--platform`, `--push`, `--output`, `--cache-to` require BuildKit; error if BuildKit disabled.
    - Compose configurations do not support `--platform`, `--push`, `--output`, `--cache-to`.
  - Deprecated: none.
- Argument Validation Rules:
  - `--config` filename must be `devcontainer.json` or `.devcontainer.json`; otherwise: error “Filename must be devcontainer.json or .devcontainer.json (...)”.
  - `--label` and `--image-name` may be specified multiple times; order preserved.
  - `--additional-features` must be valid JSON mapping string->(string|boolean|object); on parse error: build error.
  - `--platform` value must match `os/arch` or `os/arch/variant`; otherwise: build error.
  - When `--output` present with `--push`: error “--push true cannot be used with --output.”
  - In Compose mode, usage of `--platform`/`--push`/`--output`/`--cache-to`: error “... not supported.”


### Implementation Status

The following validation rules and flags have been fully implemented in the Rust CLI:

- **Multi-tag support**: `--image-name` can be specified multiple times; all tags are applied to the built image and returned in the success payload as an array.
- **Custom labels**: `--label` can be specified multiple times; labels are injected into the image along with devcontainer metadata.
- **Push to registry**: `--push` triggers BuildKit push mode; requires BuildKit availability.
- **Export artifacts**: `--output` enables BuildKit output customization (e.g., OCI archive export); mutually exclusive with `--push`.
- **BuildKit gating**: BuildKit-only flags (`--push`, `--output`, `--platform`, `--cache-to`) fail fast with clear error messages when BuildKit is unavailable.
- **Compose mode restrictions**: Compose configurations reject unsupported flags (`--platform`, `--push`, `--output`, `--cache-to`) with validation errors.
- **Success payload schema**: JSON output matches the contract: `{ "outcome": "success", "imageName": string | string[], "pushed"?: boolean, "exportPath"?: string }`.
- **Error payload schema**: JSON errors follow the contract: `{ "outcome": "error", "message": string, "description"?: string }`.
## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args: CommandLineArgs) -> ParsedInput:
    DECLARE input: ParsedInput
    input.workspace_folder = args.get('--workspace-folder') OR args.positional_path OR CWD
    input.config_file = args.get('--config')  // optional

    // Logging
    input.log_level = args.get('--log-level','info')
    input.log_format = args.get('--log-format','text')

    // Build flags
    input.no_cache = args.has('--no-cache')
    input.image_names = args.get_all('--image-name')  // may be []
    input.cache_from = args.get_all('--cache-from')
    input.cache_to = args.get('--cache-to')  // optional
    input.buildkit_mode = args.get('--buildkit','auto')
    input.platform = args.get('--platform')  // optional
    input.push = args.get_boolean('--push', false)
    input.output = args.get('--output')  // optional
    input.labels = args.get_all('--label')

    // Features/metadata
    input.additional_features_json = args.get('--additional-features')
    input.skip_feature_auto_mapping = args.get_boolean('--skip-feature-auto-mapping', false)
    input.skip_persist_customizations = args.get_boolean('--skip-persisting-customizations-from-features', false)
    input.experimental_lockfile = args.get_boolean('--experimental-lockfile', false)
    input.experimental_frozen_lockfile = args.get_boolean('--experimental-frozen-lockfile', false)
    input.omit_syntax_directive = args.get_boolean('--omit-syntax-directive', false)

    // Validate
    IF input.output AND input.push == true THEN
        RAISE InputError("--push true cannot be used with --output.")
    END IF

    IF input.config_file AND NOT filename_is_devcontainer_json(input.config_file) THEN
        RAISE InputError("Filename must be devcontainer.json or .devcontainer.json (...) ")
    END IF

    IF input.additional_features_json THEN
        TRY
            input.additional_features = JSON_PARSE_OBJECT(input.additional_features_json)
        CATCH e
            RAISE InputError("Invalid JSON for --additional-features")
        END TRY
    ELSE
        input.additional_features = {}
    END IF

    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Configuration Sources (highest precedence first):
  - Command-line flags (e.g., `--additional-features`, BuildKit switches)
  - Environment variables used by variable substitution in `devcontainer.json`
  - Configuration file: discovered under `--workspace-folder` as `.devcontainer/devcontainer.json` or `.devcontainer.json`, or explicit `--config`
  - Defaults (e.g., log level `info`, BuildKit `auto`)
- Merge Algorithm:
  - Compute the workspace by normalizing `workspace_folder`.
  - Locate the effective config file path (explicit `--config` else default search under workspace).
  - Read the configuration and create a substituted view suitable for pre-container evaluation. For build, pre-container substitution is sufficient.
  - Compose mode: read docker-compose fragments via discovered compose files and environment file; compute project name.
  - Additional features: merge `input.additional_features` atop `config.features` (last write wins for identical keys).
- Variable Substitution Rules:
  - Apply pre-container substitution to config using host environment variables (`${env:VAR}`, `${localEnv:VAR}`) and built-in rules from the spec.
  - No container-based substitution is used in pure `build` flows (no running container).

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    params = create_docker_params(
        docker_path=..., docker_compose_path=...,
        workspace_folder=parsed.workspace_folder,
        build_no_cache=parsed.no_cache,
        use_buildkit=parsed.buildkit_mode,
        buildx_platform=parsed.platform,
        buildx_push=parsed.push,
        buildx_output=parsed.output,
        buildx_cache_to=parsed.cache_to,
        additional_cache_froms=parsed.cache_from,
        additional_labels=parsed.labels,
        skip_feature_auto_mapping=parsed.skip_feature_auto_mapping,
        skip_persisting_customizations_from_features=parsed.skip_persist_customizations,
        experimental_lockfile=parsed.experimental_lockfile,
        experimental_frozen_lockfile=parsed.experimental_frozen_lockfile,
        omit_syntax_directive=parsed.omit_syntax_directive,
        log_level=parsed.log_level, log_format=parsed.log_format)

    // Phase 2: Pre-execution validation
    config = read_effective_devcontainer_config(params, parsed.config_file)
    ENSURE config exists ELSE ERROR("Dev container config not found.")
    ensure_no_disallowed_features(config, parsed.additional_features)

    image_names = parsed.image_names  // may be []
    compose_mode = config.has('dockerComposeFile')
    dockerfile_mode = is_dockerfile_config(config)

    // Phase 3: Main execution
    IF dockerfile_mode THEN
        result = build_named_image_and_extend(
            params, config, parsed.additional_features, can_add_labels=false, arg_image_names=image_names)
        final_names = image_names OR result.updated_image_name
    ELSE IF compose_mode THEN
        ASSERT NOT parsed.platform AND NOT parsed.push AND NOT parsed.output AND NOT parsed.cache_to
        compose_files, env_file, compose_args = resolve_compose_files_and_args(config)
        version_prefix = read_version_prefix(compose_files)
        compose_res = build_and_extend_compose(
            config, project_name(params, compose_files), params, compose_files, env_file,
            compose_args, [config.service], params.build_no_cache, persisted_folder(params),
            'docker-compose.devcontainer.build', version_prefix, parsed.additional_features,
            can_add_labels=false, additional_cache_froms=parsed.cache_from)
        original_name = derive_original_service_image(compose_res, config.service)
        IF image_names NOT EMPTY THEN
            tag_all(original_name, image_names)
            final_names = image_names
        ELSE
            final_names = original_name
        END IF
    ELSE // image reference mode (config.image)
        final = extend_image(
            params, config, base_image=config.image,
            additional_image_names=image_names, additional_features=parsed.additional_features,
            can_add_labels=false)
        final_names = image_names OR final.updated_image_name
    END IF

    // Phase 4: Post-execution
    RETURN Success(outcome='success', imageName=final_names)
END FUNCTION
```

## 6. State Management
- Persistent State:
  - Feature temp folder(s) under a cache/persisted folder: scripts (`devcontainer-features-install.sh`), env files, generated Dockerfiles.
  - Optional lockfile for Features when `--experimental-lockfile` set; validated by `--experimental-frozen-lockfile`.
  - In Compose mode, a generated override file is written under `<persisted>/docker-compose/docker-compose.devcontainer.build-<timestamp>-<uuid>.yml` and used for build/tagging.
- Cache Management:
  - BuildKit `--cache-from` and `--cache-to` supported in Dockerfile/image modes; in Compose mode `--cache-to` is rejected.
  - `--no-cache` disables using cache and bypasses `--cache-from`.
  - Feature resolution caches content by source to avoid refetching across runs.
- Lock Files:
  - Experimental Features lockfile is written/validated when corresponding flags are set.
- Idempotency:
  - Re-running with identical inputs yields identical image labels and tags. Temporary images (non-BuildKit feature-content helper) may be rebuilt.

## 7. External System Interactions

#### Docker/Container Runtime
```pseudocode
FUNCTION docker_build_with_features(params, feature_build_info, image_tags, labels) -> void:
    DECLARE args: string[]
    IF params.buildkit_version EXISTS THEN
        APPEND args: ['buildx','build']
        IF params.buildx_platform THEN APPEND args: ['--platform', params.buildx_platform]
        IF params.buildx_push THEN APPEND args: ['--push']
        ELSE IF params.buildx_output THEN APPEND args: ['--output', params.buildx_output]
        ELSE APPEND args: ['--load']
        IF params.buildx_cache_to THEN APPEND args: ['--cache-to', params.buildx_cache_to]
        IF NOT params.build_no_cache THEN FOR c IN params.additional_cache_froms DO APPEND args: ['--cache-from', c]
        FOR (name, path) IN feature_build_info.buildKitContexts DO APPEND args: ['--build-context', name + '=' + path]
        FOR opt IN feature_build_info.securityOpts DO APPEND args: ['--security-opt', opt]
    ELSE
        APPEND args: ['build']
    END IF
    IF params.build_no_cache THEN APPEND args: ['--no-cache']
    FOR (k,v) IN feature_build_info.buildArgs DO APPEND args: ['--build-arg', k + '=' + v]
    APPEND args: ['--target', feature_build_info.overrideTarget]
    APPEND args: ['-f', feature_build_info.dockerfilePath]
    FOR t IN image_tags DO APPEND args: ['-t', t]
    FOR l IN labels DO APPEND args: ['--label', l]
    APPEND args: [empty_context_dir]
    RUN docker[buildx?] WITH args (PTY if TTY, else non-PTY)
END FUNCTION
```

#### OCI Registries
- Authentication and push behavior is delegated to Docker/buildx; if `--push` is set, buildx pushes per daemon credentials/credential helpers.
- Multi-arch builds are determined by `--platform` when BuildKit is enabled.

#### File System
- Reads: `devcontainer.json`, Dockerfile(s), docker-compose.yml, `.env` (Compose), feature sources and cache.
- Writes: temp feature workspace, generated Dockerfiles and env files, optional lockfile, optional compose override, optional cache artifacts via `--cache-to`.

## 8. Data Flow Diagrams

```
┌────────────────────────────┐
│ CLI Args + Env (Host)      │
└───────────────┬────────────┘
                │
                ▼
┌────────────────────────────┐
│ Parse & Validate           │
└───────────────┬────────────┘
                │
                ▼
┌────────────────────────────┐
│ Resolve Config             │
│ (dockerfile | image | comp)│
└───────────────┬────────────┘
                │
        ┌───────┴────────┐
        │                │
        ▼                ▼
  Dockerfile/Image   Compose Resolve
  Build + Extend     (build override)
        │                │
        └───────┬────────┘
                ▼
┌────────────────────────────┐
│ Tag/Export, Return JSON    │
└────────────────────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Invalid `--config` filename → exit code 1, message: “Filename must be devcontainer.json or .devcontainer.json (...)”. Remediation: pass a proper path.
  - `--output` with `--push` → exit code 1, message: “--push true cannot be used with --output.” Remediation: choose one.
  - Compose with unsupported flags (`--platform`, `--push`, `--output`, `--cache-to`) → exit code 1, message: “... not supported.”
  - Invalid `--platform` value → exit code 1, message: “not supported/invalid platform”.
  - Invalid `--additional-features` JSON → exit code 1, message: “Invalid JSON for --additional-features”.
- System Errors:
  - Docker not available or failing build/inspect → exit code 1; show underlying stderr; no retry built-in.
  - Network failures fetching Features/images → exit code 1; underlying message emitted; user may retry.
- Configuration Errors:
  - Config not found → exit code 1, description “Dev container config (...) not found.”
  - Disallowed Feature present → exit code 1, description explaining disallowance with optional URL.

## 10. Output Specifications
- Standard Output (stdout):
  - JSON Mode (default):
    - Success: `{ "outcome": "success", "imageName": string | string[], "pushed"?: boolean, "exportPath"?: string }`
      - `imageName`: Single tag (string) or array of tags when multiple `--image-name` flags provided
      - `pushed`: Optional boolean; `true` when `--push` was used and succeeded
      - `exportPath`: Optional string; path to exported artifact when `--output` was used
    - Error: `{ "outcome": "error", "message": string, "description"?: string }`
      - `message`: Short error message (e.g., "BuildKit is required for this operation")
      - `description`: Optional detailed context for debugging
  - Text Mode: Not used for primary result; logs go to stderr.
- Standard Error (stderr):
  - Logs at chosen level; build progress streamed (pty vs non-pty).
- Exit Codes:
  - 0 on success; 1 on any handled error.

## 11. Performance Considerations
- Caching Strategy: Prefer BuildKit with `--cache-from` and `--cache-to`. Avoid cache when `--no-cache`.
- Parallelization: Tagging can be parallelized; actual build delegated to Docker/buildx which may parallelize stages/architectures.
- Resource Limits: Build respects daemon limits; multi-arch builds may be heavy; minimize contexts for feature builds (empty context + `--build-context`).
- Optimizations:
  - Use build contexts for feature content when BuildKit >= 0.8.
  - Avoid rebuild when only retagging (`--image-name` on an existing image).

## 12. Security Considerations
- Secrets Handling: CLI may mask provided secrets in logs; build relies on Docker credentials helpers; do not log secrets.
- Privilege Escalation: None within CLI; docker/buildx may require permissions.
- Input Sanitization: Validate filenames, labels, and JSON; quote/escape arguments passed to docker commands.
- Container Isolation: Build steps operate on images; no runtime container is started in Dockerfile/image modes; Compose build path must avoid starting services during `build` (see Notes).

## 13. Cross-Platform Behavior
- Path handling: Normalize to platform-specific separators; resolve absolute paths for `--config` and workspace.
- Docker socket: Use platform-native docker CLI; in WSL, CLI host abstraction resolves correct paths.
- User ID mapping: Not directly relevant to `build`; reflected in image metadata (labels) via Features logic.
- WSL2: Compose/config path resolution accounts for WSL host path semantics.

## 14. Edge Cases and Corner Cases
- Config in subfolder referenced by `--config` while workspace points higher-level.
- Compose file with empty array and `.env` in CWD → auto-detected env file.
- Local disallowed Features → hard stop with message.
- `--omit-syntax-directive` to avoid BuildKit syntax header in specific environments.
- BuildKit disabled but `--platform`/`--push` set → explicit error.

## 15. Testing Strategy

```pseudocode
TEST SUITE for build:
    TEST "labels applied":
        GIVEN example config
        WHEN run with --label name=label-test --label type=multiple-labels
        THEN image has both labels

    TEST "local disallowed feature":
        GIVEN config using disallowed/local feature
        WHEN build
        THEN outcome=error and message explains remediation

    TEST "mutually exclusive push/output":
        WHEN build with --push true and --output type=oci,dest=out.tar
        THEN error about mutual exclusion

    TEST "buildkit cache and platform":
        WHEN build with --platform linux/amd64 and cache-from remote
        THEN build succeeds (BuildKit enabled) and tags present

    TEST "compose build not supporting platform/push/output/cache-to":
        WHEN compose-based config with those flags
        THEN error "not supported"

    TEST "config in subfolder":
        WHEN --config points into nested .devcontainer
        THEN success and image contains expected ENV from Dockerfile
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: none.
- Breaking Changes: `--no-cache` uses standard docker semantics; BuildKit-only flags gated; Compose restrictions enforced.
- Compatibility Shims: Automatic `--load` used when `--push`/`--output` omitted under buildx to maintain classic behavior.

---

Appendix A — Design Decisions

#### Design Decision: Gate `--platform`/`--push`/`--output` behind BuildKit
Implementation Behavior: If BuildKit unavailable and such flags provided, error; under BuildKit, `--push` and `--output` are mutually exclusive, with `--load` implied otherwise.
Specification Guidance: Multi-arch/push/export are implementation details delegated to the runtime; spec does not mandate build tooling; aligns with Docker buildx capabilities.
Rationale: Prevents misleading partial behavior; matches Docker CLI expectations.
Alternatives Considered: Silently ignore flags without BuildKit; rejected to avoid confusion.
Trade-offs: Hard error reduces flexibility on non-BuildKit daemons but improves predictability.

#### Design Decision: Compose restrictions
Implementation Behavior: In Compose configs, `--platform`, `--push`, `--output`, `--cache-to` are rejected.
Specification Guidance: Compose is orchestrator-focused; spec does not standardize pushing/exporting from build; defers to `docker compose` capabilities.
Rationale: Compose flows don’t expose these options uniformly; avoids ambiguous outcomes.
Alternatives Considered: Attempt to pass-through to compose build; rejected due to version variance and complexity.
Trade-offs: Reduced feature surface in Compose mode.

#### Design Decision: Inject devcontainer metadata as image label
Implementation Behavior: Always label the resulting image with merged devcontainer metadata (and optionally feature customizations) for later discovery.
Specification Guidance: Spec defines image metadata label schema for dev containers.
Rationale: Enables downstream tooling (`up`, `set-up`) to reconstruct configuration and lifecycle origin.
Alternatives Considered: Persist outside of image; rejected for portability.
Trade-offs: Slightly larger image; clear provenance.

Appendix B — Open Questions / Spec Gaps
- Compose-based `build` runs should not start containers; ensure behavior strictly builds and tags (TS implementation writes override and may call compose up in other contexts; confirm boundary conditions for `build`).
- Lockfile semantics (experimental) are not standardized; align when spec formalizes.

