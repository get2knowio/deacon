# Features Info Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Query information about a Feature reference from an OCI registry or local source: manifest, canonical id, published tags, and dependency graph (text output).
- User Personas:
  - Authors/consumers: Inspect published features for available tags and verify metadata.
  - CI/automation: Validate manifests and canonical references.
- Specification References:
  - Features over OCI: containers.dev implementors spec (registry layout, manifests)
  - Feature metadata semantics (dependsOn, installsAfter, options).
- Related Commands:
  - `features publish`: Validate results after publish.
  - `features test`: Use dependency graph to understand order.

## 2. Command-Line Interface
- Full Syntax:
  - `devcontainer features info <mode> <feature> [--log-level <lvl>] [--output-format <text|json>]`
- Flags and Options (from TS CLI):
  - `mode`: one of `manifest`, `tags`, `dependencies`, `verbose`.
  - `feature`: Feature identifier (local path or `registry/namespace/name[/tag]`).
  - `--log-level <info|debug|trace>`: Logging level (default `info`).
  - `--output-format <text|json>`: Output format (default `text`).
- Argument Validation Rules:
  - Invalid/unknown feature ref ⇒ exit 1; empty JSON `{}` when `--output-format json`.
  - If no manifest found (auth required, not logged in) ⇒ exit 1 with message.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    input.mode = require(args.positional('mode'))
    input.feature = require(args.positional('feature'))
    input.log_level = map_log_level(args['--log-level'])
    input.format = args['--output-format'] OR 'text'
    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Sources: Parses the provided feature ref; if local path, read metadata; if registry ref, resolve.
- Variable Substitution: None by CLI; registry access handled by helpers.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(input) -> ExecutionResult:
    logger = create_logger(level=input.log_level)
    ref = parse_feature_ref(input.feature)
    IF NOT ref THEN
        RETURN json_or_text_error('Failed to parse Feature identifier')
    END IF

    IF input.mode IN ['manifest','verbose'] THEN
        manifest = fetch_oci_manifest(ref)
        IF NOT manifest THEN json_or_text_error('No manifest found! ...')
        IF input.format == 'text' THEN
            print_box('Manifest'); print(JSON.stringify(manifest, 2))
            print_box('Canonical Identifier'); print(canonical_id(manifest, ref))
        ELSE
            json.manifest = manifest; json.canonicalId = canonical_id(manifest, ref)
        END IF
    END IF

    IF input.mode IN ['tags','verbose'] THEN
        tags = fetch_published_tags(ref)
        IF tags EMPTY THEN json_or_text_error('No published versions found ...')
        IF input.format == 'text' THEN
            print_box('Published Tags'); print(list(tags))
        ELSE json.publishedTags = tags END IF
    END IF

    IF input.mode IN ['dependencies','verbose'] AND input.format == 'text' THEN
        graph = build_dependency_graph_from_ref(ref)
        print_box('Dependency Tree (Render with https://mermaid.live/)')
        print(mermaid_from_graph(graph))
    END IF

    IF input.format == 'json' THEN print(JSON.stringify(json, 4))
    RETURN Success
END FUNCTION
```

## 6. State Management
- None; read‑only queries.

## 7. External System Interactions

### OCI Registries
- `fetchOCIManifestIfExists(ref)`: Returns manifest and canonical identifier; may require auth.
- `getPublishedTags(ref)`: Returns list of tags; empty ⇒ error.

### File System
- Local feature paths can be parsed to extract metadata for `manifest` output.

## 8. Data Flow Diagrams

```
┌──────────────┐
│ Feature Ref  │
└──────┬───────┘
       │
       ▼
┌──────────────────┐
│ Resolve & Fetch  │
└───┬──────────────┘
    │
    ├──▶ Manifest + Canonical Id
    │
    ├──▶ Published Tags
    │
    └──▶ Dependency Graph (text)
```

## 9. Error Handling Strategy
- Parsing errors: text message or `{}` in JSON mode.
- No manifest/tags: exit 1 with message; `{}` in JSON mode.
- Dependency graph failures: exit 1 with message.

## 10. Output Specifications
- JSON Mode:
  - `manifest`: `OCIManifest` object and `canonicalId` string.
  - `tags`: `{ publishedTags: string[] }`.
  - `verbose`: union of the above (graph remains text in TS implementation).
- Text Mode: Boxed sections with headings; nicely formatted manifest JSON and tag lists; dependency graph as Mermaid.
- Exit Codes: `0` success; `1` on any error.

## 11. Performance Considerations
- Network‑bound; minimal CPU.

## 12. Security Considerations
- Auth required for private refs; respect `DOCKER_CONFIG` or other env conventions; redact sensitive info.

## 13. Cross-Platform Behavior
- OS‑agnostic; network I/O only.

## 14. Edge Cases and Corner Cases
- Private registries; rate limits; nonexistent tags.

## 15. Testing Strategy

```pseudocode
TEST manifest (public): returns manifest + canonicalId
TEST tags (public): returns non-empty tag list
TEST tags (empty): returns error (text) or {} (json) and exit 1
TEST dependencies: emits Mermaid graph in text mode
```

## 16. Migration Notes
- NOTE: TS CLI changed `publishedVersions` to `publishedTags` in outputs.

### Selected Design Decisions

#### Design Decision: JSON output only for data, not graphs
Implementation Behavior:
- JSON mode returns manifest/tags; dependency graph is not emitted as JSON in TS.

Specification Guidance:
- Spec does not mandate CLI output shapes; aligns with developer ergonomics.

Rationale:
- Simpler consumption; graph is a visualization aid best kept as Mermaid.

Alternatives Considered:
- Emit graph JSON; increases complexity with limited benefit.

Trade-offs:
- Less machine‑readable detail for dependencies; acceptable.

