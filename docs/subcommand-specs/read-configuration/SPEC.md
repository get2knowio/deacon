# Read-Configuration Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Resolve and display the effective Dev Container configuration without creating or modifying containers. Optionally includes Feature resolution details and a merged view combining the base configuration with metadata derived from Features and/or a running container image.
- User Personas:
  - Developers: Inspect the configuration the CLI will use for `up`, `build`, and lifecycle runs; debug variable substitution.
  - CI/Automation: Verify configuration resolution deterministically without side effects.
  - Tooling/Extensions: Programmatically obtain config, workspace mapping, Feature plan, and merged result for further processing.
- Specification References:
  - Dev Container Specification (configuration properties, lifecycle, environment): https://containers.dev/implementors/spec
  - Features (distribution, options, metadata label semantics): https://containers.dev/implementors/features/
  - Variable substitution rules: https://containers.dev/implementors/spec (environment and path substitutions)
  - Image metadata labels schema (devcontainer metadata): specification and CLI reference implementation
- Related Commands:
  - `build`: Consumes resolved config and Features; produces metadata labels that `read-configuration` can surface in `mergedConfiguration`.
  - `up`: Uses resolved config to create/start containers; `read-configuration` shows the inputs.
  - `run-user-commands` / `set-up`: Use the merged configuration to run lifecycle hooks; `read-configuration` is the non-mutating inspection counterpart.

## 2. Command-Line Interface
- Full Syntax: `devcontainer read-configuration [options]`
- Flags and Options:
  - Paths and discovery:
    - `--workspace-folder <PATH>` Optional. Root used to discover config; also used to infer id-labels when locating a container.
    - `--config <PATH>` Optional. Explicit devcontainer.json path. For this subcommand, the filename is not strictly enforced.
    - `--override-config <PATH>` Optional. Override devcontainer.json path; required when no base config exists.
    - `--mount-workspace-git-root` Optional boolean, default true. Influences workspace discovery/mount in workspace resolution.
  - Container selection:
    - `--container-id <ID>` Optional. Target container ID; enables container-based substitutions and metadata reads.
    - `--id-label <name=value>` Optional, repeatable. Used to locate the container if `--container-id` is not provided. If neither `--container-id` nor `--id-label` is set, one is inferred from `--workspace-folder`.
  - Docker tooling:
    - `--docker-path <PATH>` Optional. Docker CLI binary path (default `docker`).
    - `--docker-compose-path <PATH>` Optional. Docker Compose CLI path (default `docker-compose`, with v2 detected via `docker compose`).
  - Logging/terminal:
    - `--log-level {info|debug|trace}` Optional, default `info`.
    - `--log-format {text|json}` Optional, default `text` (applies to stderr logs only; stdout is JSON payload).
    - `--terminal-columns <N>` Optional. Requires `--terminal-rows`.
    - `--terminal-rows <N>` Optional. Requires `--terminal-columns`.
  - Features and output shaping:
    - `--include-features-configuration` Optional boolean. Include computed Feature configuration in output.
    - `--include-merged-configuration` Optional boolean. Include the merged configuration (base + metadata) in output.
    - `--additional-features <JSON>` Optional. JSON mapping per `features` schema to apply additionally during resolution.
    - `--skip-feature-auto-mapping` Optional hidden boolean (testing); bypass auto mapping of older Feature IDs.
  - Other:
    - `--user-data-folder <PATH>` Accepted but not used by this subcommand (present for parity).
- Flag Taxonomy:
  - Required vs Optional: At least one of `--container-id`, `--id-label`, or `--workspace-folder` is required. All others are optional.
  - Paired requirements: `--terminal-columns` implies `--terminal-rows` and vice versa.
  - Mutually exclusive: None enforced by this subcommand.
  - Deprecated: None.
- Argument Validation Rules:
  - `--id-label` must match `<name>=<value>`; regex `/.+=.+/` (value must be non-empty). Multiple occurrences allowed.
  - If none of `--container-id`, `--id-label`, or `--workspace-folder` are present: error “Missing required argument: One of --container-id, --id-label or --workspace-folder is required.”
  - `--additional-features` must parse as JSON object mapping string->(string|boolean|object); on parse error: command error.
  - For this subcommand, `--config` filename is not strictly validated; it is treated as a path and read as-is.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args: CommandLineArgs) -> ParsedInput:
    input.user_data_folder = args.get('--user-data-folder')
    input.docker_path = args.get('--docker-path') OR 'docker'
    input.docker_compose_path = args.get('--docker-compose-path') OR 'docker-compose'
    input.workspace_folder = resolve_path(CWD, args.get('--workspace-folder')) if provided
    input.mount_workspace_git_root = args.get('--mount-workspace-git-root', default=true)
    input.container_id = args.get('--container-id')
    input.id_label = to_array(args.get_all('--id-label'))  // repeatable
    VALIDATE every input.id_label matches /.+=.+/ ELSE error
    REQUIRE any of [input.container_id, input.id_label non-empty, input.workspace_folder] ELSE error
    input.config_file = resolve_uri(CWD, args.get('--config')) if provided
    input.override_config_file = resolve_uri(CWD, args.get('--override-config')) if provided
    input.log_level = args.get('--log-level') OR 'info'
    input.log_format = args.get('--log-format') OR 'text'
    input.terminal_columns = args.get('--terminal-columns')
    input.terminal_rows = args.get('--terminal-rows')
    IF exactly one of terminal_columns or terminal_rows is set THEN error
    input.include_features_configuration = args.get('--include-features-configuration', default=false)
    input.include_merged_configuration = args.get('--include-merged-configuration', default=false)
    input.additional_features = parse_json(args.get('--additional-features') OR '{}')
    input.skip_feature_auto_mapping = args.get('--skip-feature-auto-mapping', default=false)
    RETURN input
END FUNCTION
```

## 4. Configuration Resolution

- Configuration Sources (highest to lowest precedence for value computation):
  - Command-line flags (`--config`, `--override-config`, Feature and output flags)
  - Environment variables on the host (used during substitution)
  - Configuration files (`devcontainer.json` or `.devcontainer/devcontainer.json`), optionally overridden by `--override-config`
  - Default values (e.g., default workspace folder/mounts)
- Discovery and Reading:
  - Build `cliHost` with platform/env and logging.
  - Compute `workspace` using `workspaceFromPath` when `--workspace-folder` is provided.
  - Determine `configPath`:
    - If `--config` provided: use it directly.
    - Else if `workspace` present: try `getDevContainerConfigPathIn(workspace.configFolderPath)`; if not found and `--override-config` provided, use `getDefaultDevContainerConfigPath(workspace.configFolderPath)` to anchor substitution and workspace resolution.
    - Else if no workspace: use `--override-config` (if provided) as the config to read.
  - Read config via `readDevContainerConfigFile(cliHost, workspace, configPath, mountWorkspaceGitRoot, output, consistency?, overrideConfigFile?)`:
    - Parses JSON with comments (JSONC) and normalizes old properties via `updateFromOldProperties`.
    - Computes `workspaceConfig` including `workspaceFolder` and `workspaceMount` with substitution and defaults; if `config.workspaceFolder` is a string, it overrides `workspaceConfig.workspaceFolder`.
    - Applies pre-container substitution using host env and paths: `${env:VAR}`, `${localEnv:VAR}`, `${localWorkspaceFolder}`, `${containerWorkspaceFolder}`, `${localWorkspaceFolderBasename}`.
    - Returns a wrapper `{ raw, config, substitute }` and `workspaceConfig`.
  - If a config path or workspace was specified but the file cannot be read, throw: “Dev container config (<path>) not found.”
  - If only container selection flags are provided (no config or workspace), proceed with an empty base config `{}` and a substitution function seeded with host env/paths.
- Substitution Rules:
  - Before-container substitution (`beforeContainerSubstitute`): Expands `${devcontainerId}` using id-labels; id label order does not affect the value; adding/removing labels changes the ID.
  - Container substitution (`containerSubstitute`): When a container is found, expands `${containerEnv:VAR}`, `${containerWorkspaceFolder}`, and similar variables using the container’s environment and config file path.
  - Defaults for env placeholders: `${localEnv:NAME:default}` and `${containerEnv:NAME:default}` provide fallback values when unset; `${localEnv:NAME}` without default resolves to empty string if missing.
- Feature Resolution:
  - `--include-features-configuration` forces computing `featuresConfiguration` even without merged output.
  - `--include-merged-configuration` implicitly requires features when no container is present (metadata comes from base config + features plan).
  - Additional features from `--additional-features` are merged into the plan. `--skip-feature-auto-mapping` disables auto-mapping legacy IDs to OCI references (testing toggle).
- Merge Algorithm (when `--include-merged-configuration`):
  - If a container is found: obtain metadata from the container (`getImageMetadataFromContainer`). Apply `containerSubstitute` to the resulting metadata entries.
  - If no container: compute `imageBuildInfo` from the config and features, then derive metadata via `getDevcontainerMetadata`.
  - Combine base config and metadata via `mergeConfiguration(config, imageMetadata)`:
    - Remote environment: last-wins per key (metadata entries later in the chain override earlier; base config can override metadata according to implementation).
    - Mounts: deduplicated by target; mix string and object forms; retain stable ordering with deduplication.
    - Lifecycle hooks: arrays/commands merged respecting metadata-defined origins; maintain idempotency semantics in consumers.
    - Host requirements (e.g., GPU) merged structurally.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    cliHost = getCLIHost(CWD or parsed.workspace_folder)
    output = createLog(log_level=parsed.log_level, log_format=parsed.log_format, stderr)
    dockerCLI = parsed.docker_path OR 'docker'
    dockerComposeCLI = dockerComposeCLIConfig(cliHost, parsed.docker_compose_path OR 'docker-compose')
    params = DockerCLIParameters{ cliHost, dockerCLI, dockerComposeCLI, env=cliHost.env, output, platformInfo }

    // Phase 2: Pre-execution validation & discovery
    workspace = parsed.workspace_folder ? workspaceFromPath(cliHost.path, parsed.workspace_folder) : undefined
    configPath = resolve_config_path(workspace, parsed.config_file, parsed.override_config_file)
    configs = configPath ? readDevContainerConfigFile(cliHost, workspace, configPath, parsed.mount_workspace_git_root, output, undefined, parsed.override_config_file) : undefined
    IF (parsed.config_file OR parsed.workspace_folder OR parsed.override_config_file) AND configs is undefined THEN
        ERROR("Dev container config (<path>) not found.")
    configuration = configs?.config OR { raw: {}, config: {}, substitute: (value) => substitute({ platform: cliHost.platform, env: cliHost.env }, value) }

    // Find container and id labels
    providedIdLabels = parsed.id_label[]
    { container, idLabels } = findContainerAndIdLabels(params, parsed.container_id, providedIdLabels, parsed.workspace_folder, configPath?.fsPath)
    IF container exists THEN
        configuration = addSubstitution(configuration, cfg => beforeContainerSubstitute(envListToObj(idLabels), cfg))
        configuration = addSubstitution(configuration, cfg => containerSubstitute(cliHost.platform, configuration.config.configFilePath, envListToObj(container.Config.Env), cfg))
    END IF

    // Phase 3: Main execution (optional features + merged)
    featuresConfiguration = undefined
    IF parsed.include_features_configuration OR (parsed.include_merged_configuration AND NOT container) THEN
        featuresConfiguration = readFeaturesConfig(params, pkg, configuration.config, extensionPath, parsed.skip_feature_auto_mapping, parsed.additional_features)
    END IF
    mergedConfig = undefined
    IF parsed.include_merged_configuration THEN
        IF container THEN
            imageMetadata = getImageMetadataFromContainer(container, configuration, featuresConfiguration, idLabels, output).config
            imageMetadata = imageMetadata.map(cfg => containerSubstitute(cliHost.platform, configuration.config.configFilePath, envListToObj(container.Config.Env), cfg))
        ELSE
            imageBuildInfo = getImageBuildInfo(params, configuration)
            imageMetadata = getDevcontainerMetadata(imageBuildInfo.metadata, configuration, featuresConfiguration).config
        END IF
        mergedConfig = mergeConfiguration(configuration.config, imageMetadata)
    END IF

    // Phase 4: Post-execution (output)
    stdout_json({ configuration: configuration.config, workspace: configs?.workspaceConfig, featuresConfiguration, mergedConfiguration: mergedConfig })
    RETURN success
CATCH error AS e:
    log_error(stderr, e)
    RETURN error
FINALLY:
    dispose_resources()
END FUNCTION
```

## 6. State Management
- Persistent State: None. No files are created or modified by this subcommand.
- Cache Management: None within this command; Feature resolution may leverage caches in underlying helpers but does not persist state here.
- Lock Files: None.
- Idempotency: Safe to run repeatedly; output depends solely on current workspace/config, container state, and flags.

## 7. External System Interactions

### Docker/Container Runtime
```pseudocode
FUNCTION interact_with_docker(operation: Operation) -> Result:
    // Container detection and metadata
    docker inspect <container>                  // via findContainerAndIdLabels
    // When mergedConfiguration requested without running container, build info may be derived without network
END FUNCTION
```

### OCI Registries
- Not directly contacted by this subcommand. If metadata computation requires Features information, it uses local resolution logic; registry access is handled by other commands when building.

### File System
- Reads `devcontainer.json` (or `.devcontainer/devcontainer.json`) and optional override file; supports JSON with comments.
- Cross-platform path handling via `cliHost.path` and URIs.
- Symlinks are resolved by the underlying file host where applicable; no special handling required here.

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
│ Discover + Read Config   │
│ (JSONC, normalize, sub)  │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐
│ Find Container + Labels  │
│ (optional)               │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐
│ Features Resolution      │
│ (optional)               │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐
│ Merge Configuration      │
│ (optional)               │
└────────┬─────────────────┘
         │
         ▼
┌──────────────────────────┐
│ Emit JSON Payload        │
└──────────────────────────┘
```

## 9. Error Handling Strategy
- User Errors:
  - Missing required selector: Exit 1; message: “Missing required argument: One of --container-id, --id-label or --workspace-folder is required.” Remediation: provide one.
  - Invalid `--id-label` format: Exit 1; message: “Unmatched argument format: id-label must match <name>=<value>.” Remediation: correct format.
  - Config not found when requested: Exit 1; message includes resolved path. Remediation: adjust `--config`/`--workspace-folder`/`--override-config`.
  - Malformed JSON in `--additional-features` or config file: Exit 1; message indicates parse/validation failure.
- System Errors:
  - Docker unavailable or inspect failure: Exit 1; error written to stderr with stack when available.
  - Filesystem read errors: Exit 1; error to stderr.
- Configuration Errors:
  - Non-object config root: Exit 1; message: “Dev container config (...) must contain a JSON object literal.”
  - Compose config selected without workspace (in merged computations where required): the underlying helpers may throw; propagate as error.
- Reporting:
  - Errors are logged to stderr (respecting log format/level), and process exits with code 1. No JSON is printed to stdout in the error case.

## 10. Output Specifications

### Standard Output (stdout)
- JSON Mode (always JSON, single line):

```json
{
  "configuration": { /* DevContainerConfig (substituted) */ },
  "workspace": { /* WorkspaceConfig */ },
  "featuresConfiguration": { /* FeaturesConfig */ },
  "mergedConfiguration": { /* MergedDevContainerConfig */ }
}
```
- Fields `featuresConfiguration` and `mergedConfiguration` are omitted unless requested/needed.
- Text Mode: Not applicable; stdout is always JSON. `--log-format` affects stderr logs only.
- Quiet Mode: Not applicable.

### Standard Error (stderr)
- Logs formatted per `--log-format` and filtered per `--log-level`.
- May include progress and diagnostic details from discovery and metadata steps.

### Exit Codes
- 0: Success; JSON payload written to stdout.
- 1: Error; message logged to stderr; no stdout payload.

## 11. Performance Considerations
- Caching Strategy: None within this command; relies on fast reads and Docker inspect.
- Parallelization: Not required; operations are primarily serial (read config, inspect, resolve features, merge).
- Resource Limits: Minimal memory footprint; payload is bounded by config size.
- Optimization Opportunities: Avoid redundant registry/network calls; this command should not perform network I/O.

## 12. Security Considerations
- Secrets Handling: None. This command does not ingest secrets. Host environment variables may be referenced in substitution but are not logged in plaintext by default.
- Privilege Escalation: None. Docker is used for metadata inspection only.
- Input Sanitization: Validate `--id-label` and JSON inputs; treat file paths as data; no command injection surfaces.
- Container Isolation: No commands executed inside the container; only inspection APIs used.

## 13. Cross-Platform Behavior

| Aspect              | Linux | macOS | Windows | WSL2 |
|---------------------|-------|-------|---------|------|
| Path handling       | POSIX | POSIX | Win32   | POSIX within distro |
| Docker socket       | /var/run/docker.sock | Docker Desktop | Docker Desktop | Docker Desktop or system docker |
| User ID mapping     | N/A   | N/A   | N/A     | N/A |
| Config discovery    | Works | Works | Works   | Works |

Notes:
- Paths are resolved via `cliHost.path` and URIs to ensure portability.
- Workspace and config URIs are normalized before substitution.

## 14. Edge Cases and Corner Cases
- Only container flags provided (no config/workspace): returns `{ configuration: {}, featuresConfiguration? (if requested with merged/no container), mergedConfiguration? }` with substitutions applied where applicable.
- `--id-label` order differences do not affect `${devcontainerId}`; adding/removing labels changes the computed value.
- `--override-config` provided without a workspace: allowed; used as the sole config source.
- Invalid/missing `devcontainer.json` when `--config` or `--workspace-folder` is given: error.
- Read-only filesystems: read-only is sufficient; command does not write.
- Permission denied reading config: error with stderr details.

## 15. Testing Strategy

```pseudocode
TEST SUITE for read-configuration:

  TEST "requires container-id, id-label, or workspace-folder":
    GIVEN no selector flags
    WHEN read-configuration runs
    THEN exit 1 with missing-argument error

  TEST "id-label validation":
    GIVEN --id-label invalid format
    WHEN read-configuration runs
    THEN exit 1 with format error

  TEST "reads configuration from workspace":
    GIVEN workspace with .devcontainer/devcontainer.json
    WHEN read-configuration --workspace-folder
    THEN stdout JSON includes configuration and workspace; features/merged absent

  TEST "include features configuration only":
    GIVEN workspace config with features
    WHEN read-configuration --workspace-folder --include-features-configuration
    THEN stdout JSON includes featuresConfiguration with resolved featureSets

  TEST "include merged config (no container)":
    GIVEN workspace config with features
    WHEN read-configuration --workspace-folder --include-merged-configuration
    THEN stdout JSON includes mergedConfiguration merged from base + feature metadata

  TEST "include merged config (container)":
    GIVEN running container created from config
    WHEN read-configuration --container-id <id> --include-merged-configuration
    THEN stdout JSON includes mergedConfiguration using container image metadata

  TEST "additional-features merge":
    GIVEN base config
    WHEN read-configuration --workspace-folder --include-features-configuration --additional-features '{"xyz": {"option": true}}'
    THEN featuresConfiguration includes xyz with provided options

  TEST "override-config without base":
    GIVEN override devcontainer.json path
    WHEN read-configuration --override-config <path>
    THEN configuration reflects override file

  TEST "empty/invalid config error":
    GIVEN empty file at config path OR non-object root
    WHEN read-configuration runs
    THEN exit 1 with specific error
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: `containerEnv` has been replaced by `remoteEnv` throughout the configuration and specifically in this command’s output normalization. Legacy inputs are upgraded by `updateFromOldProperties` during read.
- Breaking Changes: None specific to this command beyond the above normalization.
- Compatibility Shims: Legacy properties are auto-translated to current schema in the parsed configuration.

