# Outdated Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Analyze Features declared in a devcontainer configuration and report their current, wanted, and latest available versions. Helps developers understand upgrade opportunities before running `upgrade` or `build`.
- User Personas:
  - Application developers: Quickly see which Features are outdated in their project.
  - CI/Automation engineers: Generate machine-readable reports to gate merges or trigger upgrades.
  - Tooling integrators: Surface upgrade hints in editors or dashboards.
- Specification References:
  - Dev Container configuration and Features: containers.dev implementors spec (features, configuration resolution, identifiers)
  - Features distribution and semantics: implementors/features-distribution and implementors/features
  - Lockfile semantics: implementors/features (lockfile proposals and behavior co-located with Features)
- Related Commands:
  - `upgrade`: Consumes the same inputs to update lockfile and optionally the config; `outdated` is read-only.
  - `read-configuration`: Produces the effective configuration that `outdated` reads to locate declared Features.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer outdated --workspace-folder <path> [--config <path>] [--output-format <text|json>] [--log-level <info|debug|trace>] [--log-format <text|json>] [--terminal-columns <n> --terminal-rows <n>]`
- Flags and Options:
  - Paths and discovery:
    - `--workspace-folder <path>` (required): Project root to discover `.devcontainer/devcontainer.json` or `.devcontainer.json`.
    - `--config <path>`: Explicit path to a devcontainer config file; if omitted, auto-discovery is used relative to `--workspace-folder`.
  - Output:
    - `--output-format <text|json>`: Default `text`. Controls stdout format.
  - Logging:
    - `--log-level <info|debug|trace>`: Default `info`.
    - `--log-format <text|json>`: Default `text`.
    - `--terminal-columns <n>` and `--terminal-rows <n>`: Provide terminal size hints; each implies the other.
- Flag Taxonomy:
  - Required: `--workspace-folder`.
  - Optional: `--config`, `--output-format`, logging options.
  - Mutually exclusive: none.
  - Implications: `--terminal-columns` implies `--terminal-rows`, and vice versa.
- Argument Validation Rules:
  - `--workspace-folder` must exist and be readable; otherwise error “Dev container config (...) not found.”
  - When `--config` is provided, it is resolved relative to CWD; if the file cannot be read, the command fails with exit code 1.
  - `--output-format` must be `text` or `json`.
  - `--terminal-columns`/`--terminal-rows` must be positive integers and provided together.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args: CommandLineArgs) -> ParsedInput:
    DECLARE input: ParsedInput
    input.workspace_folder = REQUIRE(args.get('--workspace-folder'))
    input.config_file = args.get('--config')  // optional
    input.output_format = args.get('--output-format', 'text')
    input.log_level = args.get('--log-level', 'info')
    input.log_format = args.get('--log-format', 'text')
    input.terminal_columns = args.get_number('--terminal-columns')
    input.terminal_rows = args.get_number('--terminal-rows')

    IF (input.terminal_columns XOR input.terminal_rows) THEN
        RAISE InputError('Both --terminal-columns and --terminal-rows must be provided')
    END IF

    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Configuration Sources (highest precedence first):
  - Command-line flags (`--config`, logging, output mode)
  - Environment variables used by variable substitution in `devcontainer.json`
  - Discovered configuration file under `--workspace-folder`
  - Defaults (log levels, output format)
- Merge Algorithm:
  - Normalize `workspace_folder` to an absolute path.
  - Resolve `config_file` to an absolute URI when provided; else discover default config location under workspace.
  - Load the devcontainer config with pre-container variable substitution.
  - No merges beyond reading the single effective config file are needed for `outdated`.
- Variable Substitution:
  - Apply host environment substitutions per spec (`${env:VAR}`, `${localEnv:VAR}`, etc.) sufficient to evaluate the `features` block.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed: ParsedInput) -> ExecutionResult:
    // Phase 1: Initialization
    cli_host = get_cli_host(parsed.workspace_folder, use_text_tty = parsed.log_format == 'text')
    logger = create_logger(level = parsed.log_level, format = parsed.log_format, stderr)
    workspace = workspace_from_path(cli_host.path, parsed.workspace_folder)
    config_path = parsed.config_file OR discover_config_path(cli_host, workspace)
    IF NOT exists(config_path) THEN
        RETURN ErrorResult(message = 'Dev container config (...) not found.')
    END IF
    config = read_devcontainer_config(cli_host, workspace, config_path)

    // Phase 2: Pre-execution validation
    lockfile = read_lockfile_adjacent_to(config)  // may be undefined

    // Phase 3: Main execution — produce version info per Feature
    features = user_features_to_array(config)  // ordered list
    IF features is undefined THEN
        result = { features: {} }
        RETURN success_output(parsed.output_format, result)
    END IF

    DECLARE resolved_map: map<string, FeatureVersionInfo> = {}

    PARALLEL_FOR each feature IN features DO
        feature_id = feature.userFeatureId
        ref = parse_feature_ref(feature_id)  // getRef; returns tag='latest' when no tag/digest
        IF ref is undefined THEN
            CONTINUE  // skip unversionable or invalid identifiers
        END IF

        versions = list_published_semver_tags_sorted(ref) OR []  // ascending semver, then reversed to descending
        versions_desc = REVERSE(versions)

        lock_ver = lockfile.features[feature_id]?.version

        // Compute wanted from tag/digest semantics
        DECLARE wanted: string? = lock_ver
        IF ref.tag IS NOT undefined THEN
            IF ref.tag == 'latest' THEN
                wanted = versions_desc[0]
            ELSE
                wanted = HIGHEST(versions_desc WHERE semver_satisfies(version, ref.tag))
            END IF
        ELSE IF ref.digest IS NOT undefined AND wanted IS undefined THEN
            meta = maybe_fetch_manifest_and_metadata(ref)
            wanted = meta?.version  // version from dev.containers.metadata or feature JSON
        END IF

        DECLARE current: string? = lock_ver OR wanted
        DECLARE latest: string? = versions_desc[0]

        resolved_map[feature_id] = {
            current: current,
            wanted: wanted,
            wantedMajor: wanted ? semver_major(wanted).to_string() : undefined,
            latest: latest,
            latestMajor: latest ? semver_major(latest).to_string() : undefined,
        }
    END PARALLEL_FOR

    // Reorder to match config declaration order
    ordered = {}
    FOR f IN features DO
        IF resolved_map.contains(f.userFeatureId) THEN
            ordered[f.userFeatureId] = resolved_map[f.userFeatureId]
        END IF
    END FOR

    result = { features: ordered }

    // Phase 4: Post-execution — format output
    RETURN success_output(parsed.output_format, result)
END FUNCTION
```

## 6. State Management
- Persistent State: None written. Reads `devcontainer.json` and optional lockfile alongside it. May write temporary files when resolving metadata for digest-based identifiers.
- Cache Management: Remote tag lists and blobs are fetched on-demand. On digest metadata lookup, a temporary tarball may be downloaded and extracted to a temp directory. Standard OCI registry auth/caching applies.
- Lock Files: Never modified. Outdated is read-only and does not initialize or update the lockfile.
- Idempotency: Deterministic for a given registry state and config. Results change only when upstream registries publish new versions or the lockfile/config changes. Output row order is deterministic (matches config declaration order).

## 7. External System Interactions

#### OCI Registries
- Authentication: Uses configured credential helpers (e.g., `docker login <registry>`). Requests are made to the registry API.
- Manifest/tag fetching:
  - List tags: `GET https://<registry>/v2/<namespace>/<id>/tags/list` → JSON `{ tags: string[] }`.
  - Filter tags to those that are valid semantic versions; sort ascending by `semver.compare` then reverse for descending.
  - For digest-based refs, fetch manifest and, if necessary, blob content to obtain `dev.containers.metadata` or feature metadata to recover the semantic `version`.
- Platform selection: Not applicable; features are not multi-arch images in this flow.

#### Docker/Container Runtime
- Not used by this subcommand.

#### File System
- Reads: `devcontainer.json` (or `.devcontainer.json`), adjacent lockfile (`.devcontainer-lock.json`/`devcontainer-lock.json`).
- Writes: Temporary files in the OS temp directory only when resolving digest metadata.
- Cross-platform: Normalize paths for Windows/macOS/Linux. Symlinks are treated by standard FS operations.

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
│ Resolve Config + Lock  │
└────────┬───────────────┘
         │ features
         ▼
┌────────────────────────┐
│ Map to OCI References  │
└────────┬───────────────┘
         │ refs
         ▼
┌────────────────────────┐
│ Fetch Tags/Metadata    │
└────────┬───────────────┘
         │ versions
         ▼
┌────────────────────────┐
│ Compute current/wanted │
└────────┬───────────────┘
         │ result
         ▼
┌────────────────────────┐
│ Render (text | json)   │
└────────────────────────┘
```

## 9. Error Handling Strategy

- User Errors:
  - Config not found or unreadable → exit code 1; error written to stderr with a clear description.
  - Terminal dimension flags provided singly → CLI parsing error before execution (implied flags requirement).
- System Errors:
  - Registry/network failures when listing tags or fetching manifests → log at error/trace, but do not fail the command; affected features produce `wanted/latest` as undefined. Overall exit remains 0.
- Configuration Errors:
  - Invalid/legacy feature identifiers (local paths, unknown schemes) are skipped, not errors. Outdated focuses on versionable OCI identifiers.

## 10. Output Specifications

#### Standard Output (stdout)
- JSON Mode:
  - Shape: see DATA-STRUCTURES `OutdatedResult`.
  - Indentation: two spaces when a TTY is attached; otherwise compact.
- Text Mode:
  - Columns: `Feature | Current | Wanted | Latest` using simple spacing (akin to `text-table`).
  - Feature column shows the identifier without any version suffix `:x.y.z` or `@sha256:...`.
  - Undefined values are displayed as `-`.

#### Standard Error (stderr)
- Logger output at the configured level and format. No progress bars; only diagnostics.

#### Exit Codes
- `0`: Success (report produced, possibly with undefined fields for some features).
- `1`: Failure (unexpected exception, configuration not found, or fatal IO error).

## 11. Performance Considerations
- Caching Strategy: None explicit; tag lists are fetched per invocation. Digest metadata fetch may download a small tarball to a temp directory.
- Parallelization: Tag queries run concurrently across features.
- Resource Limits: Minimal memory footprint. Network requests constrained by the number of declared features.
- Optimization Opportunities: Persist tag lists within a short-lived cache folder; skip metadata download when lockfile provides version for digest refs.

## 12. Security Considerations
- Secrets Handling: Registry credentials are handled by standard auth helpers; avoid logging tokens. Logs may include registry hostnames but not credentials.
- Privilege Escalation: None.
- Input Sanitization: Validate feature refs; only perform network requests against valid registry hostnames; ensure digest and tag patterns match constraints.
- Container Isolation: Not applicable; no containers are started.

## 13. Cross-Platform Behavior

| Aspect | Linux | macOS | Windows | WSL2 |
|--------|-------|-------|---------|------|
| Path handling | POSIX | POSIX | Win path normalization | Host path normalization |
| Docker socket | n/a | n/a | n/a | n/a |
| User ID mapping | n/a | n/a | n/a | n/a |

## 14. Edge Cases and Corner Cases
- No `features` in config → output `{ features: {} }` or an empty table (header only) and exit 0.
- Features with non-OCI identifiers (local `./feature`, direct `https://...`, legacy without registry) → omitted from output.
- Registry returns no tags or only non-semver tags → `latest/wanted` may be undefined.
- Digest-based identifiers when metadata missing → `wanted` remains undefined unless lockfile supplies version.
- Network partitions → command completes with partial data; undefined fields reflect gaps.
- Deterministic ordering → always follow config declaration order regardless of object key ordering semantics.

## 15. Testing Strategy

```pseudocode
TEST SUITE for outdated:
    TEST "json output happy path":
        GIVEN config with features (tagged, digest, untagged)
        AND lockfile with some versions
        WHEN outdated --output-format json
        THEN response.features includes keys for each versionable feature
        AND values contain current/wanted/latest with expected semver relations

    TEST "text output table":
        GIVEN same config
        WHEN outdated --output-format text
        THEN stdout includes header and one row per versionable feature
        AND filtered (local/legacy) features are absent

    TEST "registry failure":
        GIVEN simulated registry error
        WHEN outdated
        THEN exit code 0 and affected fields are undefined

    TEST "no features":
        GIVEN config without features
        WHEN outdated
        THEN JSON features={} and table shows only header
END TEST SUITE
```

## 16. Migration Notes
- Deprecated Behavior: None.
- Breaking Changes: N/A. The command is read-only.
- Compatibility Shims:
  - Deterministic output ordering ensures stable diffs across runs (tracked upstream; previously relied on object key order).

---

Appendix — Design Decisions

#### Design Decision: Treat missing tag as `latest`
Implementation Behavior: `getRef` assigns `tag='latest'` when no tag/digest is present on the feature identifier; `wanted` resolves to the highest published semver tag.
Specification Guidance: Feature references allow tags and digests; the spec does not mandate behavior for missing tags. Assuming `latest` aligns with common OCI registry conventions.
Rationale: Matches upstream CLI behavior and user expectations for unpinned features.
Alternatives Considered: Leave `wanted` undefined; rejected for poor UX.
Trade-offs: Implicitness may mask that the config is unpinned; reporting latest alongside wanted mitigates this.

#### Design Decision: Skip unversionable identifiers
Implementation Behavior: Local paths, direct tarballs, and legacy identifiers are not included in the output set.
Specification Guidance: Only OCI-based identifiers can be enumerated for versions. Local or legacy forms have no discoverable semver tag space.
Rationale: Avoids false precision and unnecessary errors for items that cannot be version-checked.
Alternatives Considered: Emit placeholders for all features; rejected to reduce noise.
Trade-offs: Output does not reflect every declared feature; documented clearly.

#### Design Decision: Current version derives from lockfile
Implementation Behavior: `current = lockfile.version || wanted`.
Specification Guidance: Lockfile records the exact resolved version used during build; it is authoritative for current.
Rationale: Reflects the last built/locked version even when the tag specifies a range or `latest`.
Alternatives Considered: Always use wanted; rejected because it ignores recorded state.
Trade-offs: Without a lockfile, current equals wanted, which may change as registries update.

#### Design Decision: Graceful degradation on registry errors
Implementation Behavior: Network or registry failures yield undefined fields; command succeeds overall.
Specification Guidance: Spec does not require strict failure when registries are unreachable for read-only introspection.
Rationale: Keeps the developer flow unblocked; failures are visible in logs and in missing fields.
Alternatives Considered: Hard-fail; rejected to improve resilience.
Trade-offs: Consumers must handle undefineds.

