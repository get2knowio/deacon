# Features Package Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Produce distributable archives for Dev Container Features either for a single feature or an entire collection. Generates `*.tgz` artifacts and a `devcontainer-collection.json` metadata file in the output folder.
- User Personas:
  - Feature authors: Package features locally prior to publishing.
  - CI engineers: Build artifacts for later upload to OCI.
  - Consumers/tools: Validate collection metadata for discovery.
- Specification References:
  - Features distribution and collection metadata: containers.dev implementors spec (Features Distribution)
  - `devcontainer-feature.json` schema and rules.
- Related Commands:
  - `features publish`: Push packaged artifacts and collection metadata to OCI.
  - `features test`: Run validation prior to packaging.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer features package [target] [--output-folder <dir>] [--force-clean-output-folder] [--log-level <lvl>]`
- Flags and Options:
  - `--output-folder, -o <dir>`: Output directory (default `./output`). Created if absent.
  - `--force-clean-output-folder, -f` (boolean): Delete previous output directory content before packaging.
  - `--log-level <info|debug|trace>`: Logging level (default `info`).
  - Positional `target` (default `.`):
    - Path to `src/` folder containing multiple features, or
    - Path to a single feature directory containing `devcontainer-feature.json`.
- Flag Taxonomy:
  - Required: none.
  - Optional: all flags above.
  - Mutually exclusive: n/a.
- Argument Validation Rules:
  - Single feature mode: directory must contain `devcontainer-feature.json`.
  - Collection mode: directory must contain `src/` and each subfolder must contain a valid feature.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    input.target = args.positional('target') OR '.'
    input.output_dir = args['--output-folder'] OR './output'
    input.force_clean = args['--force-clean-output-folder']
    input.log_level = map_log_level(args['--log-level'])
    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Sources: On‑disk feature(s) under `target`.
- Detection:
  - If `target/devcontainer-feature.json` exists: single feature mode.
  - Else if `target/src` exists: collection mode; each `src/<featureId>` is packaged.
- No merge semantics; each feature is packaged independently. Collection metadata aggregates packaged features.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(input) -> ExecutionResult:
    logger = create_logger(level=input.log_level)
    cli_host = detect_cli_host(CWD)

    if input.force_clean THEN rm_rf(input.output_dir)
    ensure_dir(input.output_dir)

    IF is_single_feature(input.target) THEN
        write_info('Packaging single feature...')
        metas = package_single_feature(input.target, input.output_dir)
    ELSE
        write_info('Packaging feature collection...')
        metas = package_feature_collection(join(input.target, 'src'), input.output_dir)
    END IF

    IF metas IS UNDEFINED OR EMPTY THEN ERROR('Failed to package features')

    collection = { sourceInformation: { source: 'devcontainer-cli' }, features: metas }
    write_file(join(input.output_dir, 'devcontainer-collection.json'), JSON.stringify(collection, 2))

    RETURN Success
END FUNCTION
```

## 6. State Management
- Persistent State: Output artifacts under `--output-folder`.
- Cache Management: None.
- Lock Files: None.
- Idempotency: Re‑packaging overwrites output; `-f` ensures clean output directory.

## 7. External System Interactions
- File System:
  - Reads source feature directories; writes tarballs and `devcontainer-collection.json`.
  - Validates presence of `devcontainer-feature.json` and related files.

## 8. Data Flow Diagrams

```
┌───────────────┐    ┌────────────────────┐
│ Source Folder │───▶│ Detect Mode        │
└──────┬────────┘    └──────┬─────────────┘
       │                    │
       ▼                    ▼
  Single Feature        Collection (src/*)
       │                    │
       ▼                    ▼
┌───────────────┐      ┌──────────────────┐
│ Create .tgz   │      │ Package each .tgz│
└──────┬────────┘      └────────┬─────────┘
       │                         │
       ▼                         ▼
      Merge into devcontainer-collection.json
```

## 9. Error Handling Strategy
- User Errors:
  - Missing `devcontainer-feature.json` in single mode: exit non‑zero with message.
  - Empty collection or invalid feature subfolders: exit non‑zero.
- System Errors:
  - File system write failures: surface OS error.
- Configuration Errors:
  - Invalid feature metadata: surface packaging error with path context.

## 10. Output Specifications
- Text Mode: Logs steps (“Packaging single feature…”, “Packaging feature collection…”). Writes output artifacts.
- JSON Mode: Not emitted by TS `package` command (would be CLI logs only). If needed, emit summary object `{ count, outputDir }`.
- Exit Codes: `0` success; `1` failure.

## 11. Performance Considerations
- Packaging is I/O bound; collection mode loops over features sequentially. Could parallelize with bounded concurrency.

## 12. Security Considerations
- Trusts local filesystem content; no code execution.

## 13. Cross-Platform Behavior
- Path handling normalized via CLI host; output folder creation consistent across OSes.

## 14. Edge Cases and Corner Cases
- Non‑ASCII paths; deep nested files.
- Read‑only output directory (fail with message).

## 15. Testing Strategy

```pseudocode
TEST "single feature": expect .tgz and collection metadata written
TEST "collection": expect one .tgz per feature and collection metadata
TEST "force clean": prepopulate output; run with -f; ensure only new artifacts remain
TEST "invalid feature": corrupt devcontainer-feature.json; expect error
```

## 16. Migration Notes
- None.

### Selected Design Decisions

#### Design Decision: Write collection metadata always
Implementation Behavior:
- Always emits `devcontainer-collection.json` describing packaged content.

Specification Guidance:
- Spec defines collection metadata file for discovery.

Rationale:
- Enables `publish` to push metadata and supports registry queries.

Alternatives Considered:
- Conditional write; increases complexity and special‑cases single feature packaging.

Trade-offs:
- Slight extra I/O; improved consistency.

