# Upgrade Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Generate or refresh the devcontainer lockfile based on the currently resolved Feature set. Optionally pin a specific Feature version in `devcontainer.json` prior to generating the lockfile. Supports a dry-run that prints the lockfile JSON instead of writing to disk.
- User Personas:
  - Developers: Update lockfile after changing Feature refs or when upstream versions change; optionally pin a specific Feature to a major/minor/patch target.
  - CI/Automation: Regenerate lockfile deterministically as part of build verification; use dry-run to capture artifacts without touching the workspace.
  - Dependency bots (Dependabot): Pin targeted Feature versions via hidden flags, then update the lockfile to match.
- Specification References:
  - Dev Container configuration and Features: https://containers.dev/implementors/spec
  - Features distribution and semantics (identifiers, versions, tags, digests): https://containers.dev/implementors/features/
  - Lockfile semantics (recording resolved versions and integrity): co-located with Features documentation in the CLI reference implementation
- Related Commands:
  - `outdated`: Reports current/wanted/latest Feature versions; often run before `upgrade` to decide whether to pin or refresh.
  - `build`: Consumes the lockfile; when experimental frozen lockfile is enabled, build enforces exact matches.
  - `read-configuration`: Resolves the effective config that `upgrade` reads to locate declared Features.

## 2. Command-Line Interface
- Full Syntax: `devcontainer upgrade --workspace-folder <PATH> [--config <PATH>] [--docker-path <PATH>] [--docker-compose-path <PATH>] [--log-level <error|info|debug|trace>] [--dry-run] [--feature <ID>] [--target-version <X[.Y[.Z]]>]`
- Flags and Options:
  - Paths and discovery:
    - `--workspace-folder <PATH>` Required. Root for configuration discovery (looks for `.devcontainer/devcontainer.json` or `.devcontainer.json`).
    - `--config <PATH>` Optional. Explicit devcontainer config path. When provided, it supersedes auto-discovery and is resolved relative to CWD.
  - Docker tooling:
    - `--docker-path <PATH>` Optional. Docker CLI path. Default `docker`.
    - `--docker-compose-path <PATH>` Optional. Docker Compose CLI path. Default `docker-compose`.
  - Logging:
    - `--log-level {error|info|debug|trace}` Optional. Default `info`. Affects stderr logging only.
  - Behavior:
    - `--dry-run` Optional. Print the generated lockfile JSON to stdout and do not write the lockfile to disk. Note: Any config edits from `--feature/--target-version` still apply to disk.
  - Hidden (Dependabot-oriented):
    - `--feature, -f <ID>` Hidden. Pin the version requirement of a specific Feature (and its dependencies) in `devcontainer.json` before generating the lockfile.
    - `--target-version, -v <X[.Y[.Z]]>` Hidden. The major (`x`), minor (`x.y`), or patch (`x.y.z`) version to pin for `--feature`.
- Flag Taxonomy:
  - Required: `--workspace-folder`
  - Optional: `--config`, `--docker-path`, `--docker-compose-path`, `--log-level`, `--dry-run`
  - Mutually constrained: `--feature` must be used with `--target-version` and vice versa.
  - Deprecated: None.
- Argument Validation Rules:
  - Pairing: If exactly one of `--feature` or `--target-version` is provided → error “The '--target-version' and '--feature' flag must be used together.”
  - Format: `--target-version` must match regex `^\d+(\.\d+(\.\d+)?)?$` → accepts `X`, `X.Y`, or `X.Y.Z`.
  - Paths: `--workspace-folder` is resolved to an absolute path; `--config` is resolved to an absolute file URI if provided.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args: CommandLineArgs) -> ParsedInput:
    DECLARE input: ParsedInput
    input.workspace_folder = REQUIRE(resolve_path(CWD, args['--workspace-folder']))
    input.config_file = resolve_uri(CWD, args['--config'])  // optional
    input.docker_path = args['--docker-path'] OR 'docker'
    input.docker_compose_path = args['--docker-compose-path'] OR 'docker-compose'
    input.log_level = args['--log-level'] OR 'info'
    input.dry_run = args['--dry-run'] IS TRUE
    input.feature = args['--feature']  // optional, hidden
    input.target_version = args['--target-version']  // optional, hidden

    IF (input.feature IS SET) XOR (input.target_version IS SET) THEN
        RAISE InputError("The '--target-version' and '--feature' flag must be used together.")
    END IF

    IF input.target_version IS SET AND NOT MATCHES(input.target_version, /^\d+(\.\d+(\.\d+)?)?$/) THEN
        RAISE InputError("Invalid version '<value>'.  Must be in the form of 'x', 'x.y', or 'x.y.z'")
    END IF

    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Configuration Sources (precedence):
  1) Command-line flags (`--config`, logging)
  2) Environment variables used by variable substitution during config load
  3) Auto-discovered devcontainer config under `--workspace-folder`
  4) Defaults
- Discovery and resolution:
  - Auto-discovery probes (in order): `<workspace>/.devcontainer/devcontainer.json`, `<workspace>/.devcontainer.json`.
  - When `--config` is provided, use it directly (no discovery); the path is resolved relative to CWD and wrapped as a file URI.
  - Load the configuration via the standard document reader (applies variable substitution per spec: `${env:VAR}`, `${localEnv:VAR}`, etc.).
  - On failure to find or parse the config, error: “Dev container config (...) not found.” or “must contain a JSON object literal.”
- Merge Algorithm:
  - No user-specified merges beyond a single effective config. The loaded config is a single JSON object with substitutions applied.
- Variable Substitution:
  - Apply host-side substitutions to allow proper evaluation of the `features` block. The upgrade flow does not require container-side values.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    cli_host = get_cli_host(parsed.workspace_folder, use_native_modules = true)
    logger = create_logger(level = map_log_level(parsed.log_level), format = 'text', sink = stderr)
    docker_compose_cli = docker_compose_cli_config(exec = cli_host.exec, env = cli_host.env, log = logger, docker_path = parsed.docker_path, compose_path = parsed.docker_compose_path)
    docker_params = { cliHost: cli_host, dockerCLI: parsed.docker_path, dockerComposeCLI: docker_compose_cli, env: cli_host.env, output: logger, platformInfo: { os: map_os(cli_host.platform), arch: map_arch(cli_host.arch) } }

    workspace = workspace_from_path(cli_host.path, parsed.workspace_folder)
    config_path = parsed.config_file OR discover_config_path(cli_host, workspace.configFolderPath)
    config = REQUIRE(read_devcontainer_config(cli_host, workspace, config_path))

    // Phase 2: Optional config edit (pin a Feature)
    IF parsed.feature IS SET AND parsed.target_version IS SET THEN
        logger.info("Updating '<feature>' to '<target_version>' in devcontainer.json")
        CALL update_feature_version_in_config(config, config_path.fsPath, parsed.feature, parsed.target_version)
        // Re-read config for consistency after edit
        config = REQUIRE(read_devcontainer_config(cli_host, workspace, config_path))
    END IF

    // Phase 3: Resolve features and construct lockfile
    pkg = get_package_config()
    features_cfg = REQUIRE(read_features_config(docker_params, pkg, config, extension_path, skip_feature_auto_mapping = false, additional_features = {}))
    lockfile = generate_lockfile(features_cfg)

    // Phase 4: Output and persistence
    IF parsed.dry_run THEN
        PRINT_JSON_TO_STDOUT(lockfile)
        RETURN Success
    END IF

    lockfile_path = get_lockfile_path(config)
    WRITE_FILE(lockfile_path, '')  // truncate or create marker
    write_lockfile(params = { output: logger, env: cli_host.env, cwd: cli_host.cwd, platform: cli_host.platform, skipFeatureAutoMapping: false }, config, lockfile, force_init = true)

    RETURN Success
END FUNCTION
```

## 6. State Management
- Persistent State:
  - Lockfile: Written to the same directory as the config using naming rule: if the config filename starts with a dot → `.devcontainer-lock.json`, else `devcontainer-lock.json`.
  - Config file edit: When `--feature` and `--target-version` are used, the command modifies `devcontainer.json` in place by replacing the matching Feature key with a version-pinned form.
- Cache Management:
  - Feature resolution reads/writes temp/cache directories when fetching feature metadata and computing digests. The upgrade subcommand itself does not manage additional caches.
- Lock Files:
  - `write_lockfile` writes only if content changed unless `experimentalFrozenLockfile` is set (not used here). Upgrade forces initialization by pre-truncating and passing `force_init = true`.
- Idempotency:
  - Re-running without changes yields the same lockfile. With `--feature/--target-version`, reruns are no-ops after the first edit. `--dry-run` does not persist the lockfile but still applies config edits.

## 7. External System Interactions

#### Docker/Container Runtime
- Not directly used during upgrade; docker CLI paths are initialized for parity and for downstream feature resolution tooling.

#### OCI Registries
- Authentication: Uses existing credential helpers (`docker login` et al.) through the feature resolution pipeline.
- Manifest/tag fetching: Feature resolution resolves tag ranges to concrete versions and fetches blobs/manifests to compute content-addressable digests used in the lockfile.
- Platform selection: Not applicable to lockfile entries; features are resolved independently of image platform.

#### File System
- Reads: `devcontainer.json` (or `.devcontainer.json`).
- Writes: Lockfile adjacent to config, and optionally `devcontainer.json` if pinning a feature via hidden flags.
- Permissions: Requires write permission to the config directory for lockfile updates; requires write permission to `devcontainer.json` when pinning.
- Symlinks: Paths are normalized by the config loader; standard file IO semantics apply.

## 8. Data Flow Diagrams

```
┌─────────────────┐
│ User Input      │
└────────┬────────┘
         │ args/env
         ▼
┌─────────────────┐
│ Parse & Validate│
└────────┬────────┘
         │
         ▼
┌────────────────────────┐
│ Resolve Configuration  │
└────────┬───────────────┘
         │ features
         ▼
┌────────────────────────┐
│ Optional Pin in Config │
└────────┬───────────────┘
         │ re-read config
         ▼
┌────────────────────────┐
│ Resolve Features Set   │
└────────┬───────────────┘
         │ featureSets
         ▼
┌────────────────────────┐
│ Generate Lockfile JSON │
└────────┬───────────────┘
         │ stdout or fs
         ▼
┌────────────────────────┐
│ Persist or Print       │
└────────────────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Missing/invalid `--target-version` pairing → CLI validation error; command aborts before execution.
  - Invalid `--target-version` format → CLI validation error with explicit message.
  - Config not found/unreadable → error “Dev container config (...) not found.”; exit code 1.
  - Pinning target not present in config → no error; logs “No Features found in '<path>'.” and leaves config unchanged.
- System Errors:
  - Registry/network failures during feature resolution → surface as errors and abort (since lockfile cannot be generated); exit code 1.
  - Filesystem permission errors writing lockfile/config → error to stderr; exit code 1.
- Configuration Errors:
  - Malformed JSON config → error: “... must contain a JSON object literal.”; exit code 1.
  - Feature resolution failure (e.g., invalid identifier) → summarized as “Failed to update lockfile”; exit code 1.

## 10. Output Specifications

### Standard Output (stdout)
- Dry-run mode: Pretty-printed JSON of the lockfile with 2-space indentation.
- Non-dry-run: No stdout payload (only stderr logs).

### Standard Error (stderr)
- Logging honors `--log-level` with levels: `error`, `info`, `debug`, `trace`.
- Progress/diagnostic messages are written as text; there is no JSON log format for this subcommand.

### Exit Codes
- `0`: Success.
- `1`: Failure (any thrown error during processing).

## 11. Performance Considerations
- Caching Strategy: Benefit from underlying feature resolution caches (downloaded artifacts, digests). Upgrade itself performs no heavy work beyond feature resolution and JSON generation.
- Parallelization: Feature resolution may perform parallel registry requests internally; upgrade does not orchestrate additional concurrency.
- Resource Limits: Minimal memory/CPU footprint; large configs with many features may increase network IO.
- Optimization Opportunities: Skip re-reading config when no pinning is requested; avoid truncation write when the new lockfile equals the existing one (currently, pre-truncation is followed by an equality check in `write_lockfile`).

## 12. Security Considerations
- Secrets Handling: Registry credentials are handled by underlying tooling; avoid logging sensitive tokens. Logs include registry hosts and feature IDs only.
- Privilege Escalation: None required by the subcommand.
- Input Sanitization: Validate `--target-version` strictly; treat `--feature` as an identifier string and match only by exact key (see design decision on replacement).
- Container Isolation: Not applicable; no containers are started.

## 13. Cross-Platform Behavior

| Aspect | Linux | macOS | Windows | WSL2 |
|--------|-------|-------|---------|------|
| Path handling | POSIX | POSIX | Win path normalization | Host path normalization |
| Docker socket | n/a | n/a | n/a | n/a |
| User ID mapping | n/a | n/a | n/a | n/a |

## 14. Edge Cases and Corner Cases
- No `features` in config → lockfile generation can still succeed for empty set; pinning logs a message and makes no change.
- `--dry-run` with `--feature/--target-version` → config file is modified on disk; lockfile is printed only (not written). This is intentional to support automation that separately commits config edits.
- Multiple matching feature keys (with/without tags/digests) → only the first matching key (by base ID sans tag/digest) is updated.
- Feature key replacement is text-based → may replace any occurrence of the exact old key string; keys must be unique to avoid unintended changes. See design decision and mitigation suggestions.
- Lockfile path derivation depends on config filename leading dot; moving the config file changes the lockfile location accordingly.

## 15. Testing Strategy

```pseudocode
TEST SUITE for upgrade:
    TEST "writes upgraded lockfile":
        GIVEN workspace with outdated .devcontainer-lock.json
        WHEN upgrade --workspace-folder <path>
        THEN lockfile equals expected upgraded.devcontainer-lock.json

    TEST "dry-run prints JSON":
        GIVEN workspace with features
        WHEN upgrade --dry-run --workspace-folder <path>
        THEN stdout parses as JSON with features map and expected entries

    TEST "pin feature then dry-run":
        GIVEN config with feature 'ghcr.io/org/feat'
        WHEN upgrade --dry-run --feature ghcr.io/org/feat --target-version 2
        THEN devcontainer.json updated to '.../feat:2'
        AND stdout lockfile contains key '.../feat:2' with resolved patch version within major 2

    TEST "invalid target-version":
        WHEN upgrade --target-version 1.x --feature id
        THEN CLI validation error before execution

    TEST "missing pairing":
        WHEN upgrade --feature id (no target)
        THEN CLI validation error before execution

    TEST "config not found":
        WHEN upgrade --workspace-folder <nonexistent>
        THEN exit code 1 with clear error
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: None.
- Breaking Changes: Introduction of lockfile update as a distinct subcommand; no breaking changes to other commands.
- Compatibility Shims: Hidden flags to pin a specific Feature version enable automation workflows (e.g., Dependabot) to carry forward existing behavior while transitioning to lockfile-driven resolution.

---

Appendix — Design Decisions

#### Design Decision: Hidden pinning flags update config on disk even in dry-run
Implementation Behavior: When `--feature` and `--target-version` are provided, the command edits `devcontainer.json` before generating the lockfile. Dry-run only affects lockfile persistence, not the config edit.
Specification Guidance: The spec is silent on CLI-side helpers for editing configuration. Editing the config file aligns with the stated purpose of pinning requirements prior to lockfile generation.
Rationale: Supports automation tools that create PRs pinning versions and regenerating lockfiles without committing the lockfile in the same step.
Alternatives Considered: Defer edits to non-dry-run only; rejected for automation ergonomics.
Trade-offs: Users may be surprised that dry-run writes to disk; documented explicitly.

#### Design Decision: Feature key matching ignores tag/digest
Implementation Behavior: Matching is based on the base identifier up to the last `:` or `@` (e.g., `ghcr.io/x/y/z` matches `ghcr.io/x/y/z:1` or `ghcr.io/x/y/z@sha256:...`).
Specification Guidance: Feature identifiers support tags and digests; matching by base identifier is consistent with the intent to pin the same logical Feature.
Rationale: Users expect pinning to affect the declared Feature regardless of current tag/digest form.
Alternatives Considered: Require exact key match; rejected as brittle.
Trade-offs: Ambiguity if multiple keys share the same base ID; only the first is updated.

#### Design Decision: Text-based key replacement
Implementation Behavior: The edit replaces all occurrences of the exact old key string with the updated string in the file text.
Specification Guidance: The spec does not define how tooling edits configuration files.
Rationale: Simplicity and robustness without parsing/rewriting JSON AST.
Alternatives Considered: Parse JSON and update the `features` object key precisely.
Trade-offs: Potential for unintended replacements if the key appears elsewhere in the file (e.g., in comments or string values). In practice, devcontainer files are JSON without comments and Feature IDs are used as keys, minimizing risk. A future refinement could operate on the parsed object and reserialize.

#### Design Decision: Lockfile path naming rule
Implementation Behavior: If the config basename starts with a dot, the lockfile is named `.devcontainer-lock.json`; otherwise `devcontainer-lock.json`.
Specification Guidance: Not formally specified; mirrors upstream CLI behavior for compatibility with common project layouts.
Rationale: Consistency across repos; preserves hidden-file convention when the config itself is a dotfile.
Alternatives Considered: Always use a single name; rejected due to upstream divergence.
Trade-offs: Moving/renaming the config changes lockfile location.

