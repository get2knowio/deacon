# Features Plan Subcommand Design Specification

## 1. Subcommand Overview
- Purpose: Compute the deterministic installation order and dependency graph for features declared in the effective devcontainer configuration (optionally merged with additional CLI features). Outputs a machine‑readable plan.
- User Personas:
  - Developers/maintainers: Preview feature order and dependency relationships.
  - CI/tools: Generate installation plans for auditing or dry‑runs.
- Specification References:
  - Features dependencies and `installsAfter` semantics: containers.dev implementors spec (Features)
  - Resolution and merge semantics for configuration: containers.dev implementors spec.
- Related Commands:
  - `build`, `up`: Consume similar computation during actual builds/installs.
  - `features info dependencies`: Related visualization, not JSON plan.

## 2. Command-Line Interface
- Full Syntax (Rust CLI):
  - `deacon features plan [--json] [--additional-features <JSON>]`
- Flags and Options:
  - `--json` (boolean, default true): Emit JSON output.
  - `--additional-features <JSON>`: Merge additional features (map of id → value/options) into config before planning.
- Argument Validation Rules:
  - `--additional-features` must be a JSON object (map). Parse errors are fatal.
  - Feature IDs must be registry references; local paths (starting with `./`, `../`, `/`, or `file://`) are rejected with a clear error message.

## 3. Input Processing Pipeline

```pseudocode
FUNCTION parse_command_arguments(args) -> ParsedInput:
    input.json = args['--json'] (default true)
    input.additional = parse_json_map(args['--additional-features']) OR {}
    RETURN input
END FUNCTION
```

## 4. Configuration Resolution
- Sources (precedence):
  1) CLI additional features (`--additional-features`)
  2) `devcontainer.json` (explicit `--config` else discovered in workspace)
  3) Default empty map
- Merge Algorithm:
  - Start with config.features (object map).
  - For each key in CLI map: if key does not exist, add; if exists and `prefer_cli_features` not in scope, last write wins per TS implementation pattern; this plan command uses additive merge without overwrite by default.
- Variable Substitution:
  - Not required for planning; feature IDs are treated as opaque strings. Option values are passed through.

## 5. Core Execution Logic

```pseudocode
FUNCTION execute_subcommand(parsed) -> ExecutionResult:
    // Phase 1: Initialization
    workspace = determine_workspace()
    config = load_or_default_config(workspace)
    features_map = merge_config_features(config.features, parsed.additional)

    // Phase 2: Pre-execution validation
    IF features_map is empty THEN
        emit_json({ order: [], graph: {} })
        RETURN Success
    END IF

    // Phase 3: Main execution
    fetcher = oci_default_fetcher()
    resolved = []
    FOR (id, val) IN features_map:
        ref = parse_registry_reference(id)
        downloaded = fetcher.fetch_feature(ref)
        options = normalize_options(val)
        resolved.push({ id: downloaded.metadata.id, source: ref.reference, options, metadata: downloaded.metadata })
    END FOR

    resolver = dependency_resolver(override_order=config.overrideFeatureInstallOrder)
    plan = resolver.resolve(resolved)
    order = plan.feature_ids()  // Deterministic topological sort with lexicographic tie-breakers
    graph = build_graph(resolved)  // Direct dependencies only: union of installsAfter and dependsOn, deduped and sorted lexicographically

    // Phase 4: Post-execution
    emit_json_or_text(order, graph, parsed.json)
    RETURN Success
END FUNCTION
```

**Determinism Notes:**
- The `order` array is produced by a deterministic topological sort with lexicographic tie-breakers for independent features.
- The `graph` object contains direct dependencies only (union of `installsAfter` and `dependsOn` arrays), deduplicated and sorted lexicographically by feature ID.

## 6. State Management
- None; read‑only fetch of metadata from registries.

## 7. External System Interactions

### OCI Registries
- Download feature metadata to compute dependencies (`dependsOn`, `installsAfter`).
- Caching: Fetcher may cache blobs locally (implementation detail of OCI client).

### File System
- Reads config from workspace; no writes unless emitting to stdout.

## 8. Data Flow Diagrams

```
┌──────────────┐
│ Config+CLI   │
└──────┬───────┘
       │ merge
       ▼
┌────────────────┐
│ Feature IDs     │
└──────┬──────────┘
       │ fetch
       ▼
┌────────────────┐
│ Metadata (OCI) │
└──────┬──────────┘
       │ resolve
       ▼
┌────────────────┐
│ Plan (order)   │
└──────┬──────────┘
       │ derive
       ▼
┌────────────────┐
│ Graph (JSON)   │
└────────────────┘
```

## 9. Error Handling Strategy
- User Errors: JSON parse failures for `--additional-features`.
- System Errors: OCI fetch failures; surface feature ID and error.
- Configuration Errors: Circular dependencies detected by resolver ⇒ error with details.

## 10. Output Specifications
- JSON Mode (default):
  - `{ "order": ["<featureId>"...], "graph": { "<id>": ["dep1", ...] } }`
- Schema: See `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/contracts/plan.schema.json` for JSON schema validation.
- Text Mode:
  - Human‑readable header, order list, and pretty‑printed graph JSON.
- Exit Codes: `0` success; `1` on failure (e.g., fetch/resolve errors).

## 11. Performance Considerations
- Metadata fetches serialized; could parallelize with concurrency limit.
- Resolver linearithmic on node count; small graphs in practice.

## 12. Security Considerations
- Remote fetch of metadata; rely on TLS and registry auth; no secrets printed.

## 13. Cross-Platform Behavior
- OS‑agnostic; network I/O and stdout only.

## 14. Edge Cases and Corner Cases
- Features without dependencies.
- Mixed local/remote features (future enhancement): plan currently expects registry refs.

## 15. Testing Strategy

```pseudocode
TEST empty features: outputs empty order and graph
TEST simple chain: A -> B installsAfter; expect order [A, B]
TEST dependsOn cycles: expect error
TEST additional-features merge: ensure CLI additions included in order/graph
```

## 16. Migration Notes
- Not present in TS CLI; aligns with TS dependency graph computation used in `features info dependencies` and internal planning.

### Selected Design Decisions

#### Design Decision: Graph content combines installsAfter and dependsOn
Implementation Behavior:
- Emits unified adjacency list for transparency.

Specification Guidance:
- Both fields impact installation order; combining reveals full constraint set.

Rationale:
- Useful for debugging merge results and ordering.

Alternatives Considered:
- Separate graphs per relation; more verbose with limited additional value.

Trade-offs:
- Slightly denser output; clearer single artifact.

