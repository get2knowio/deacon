# Data Model — Close Spec Gap (Features Plan)

This captures the entities, fields, and validation rules for the planning feature.

## Entities

### FeatureOptions
- Type: Map<string, any> (serde_json::Value)
- Notes: Values are opaque for planning and passed through unchanged.

### FeatureMetadata (registry-fetched)
- Fields:
  - id: string (canonical feature ID — trimmed)
  - installsAfter: string[] (optional; default: [])
  - dependsOn: string[] (optional; default: [])
- Validation:
  - Arrays contain strings; duplicates permitted in source but will be deduplicated in graph assembly.

### Plan
- Fields:
  - order: string[]
    - Description: Deterministic installation order derived from topological sort; among independent nodes use lexicographic order by canonical ID.
  - graph: Record<string, string[]>
    - Description: For each feature ID, the array lists direct dependencies only — the deduplicated union of `installsAfter` and `dependsOn`, sorted lexicographically.
- Validation:
  - All `graph[id]` entries must reference existing IDs present in inputs.
  - `order` must contain each feature ID exactly once (no duplicates).
  - Cycles produce an error (no partial plan emitted).

## Input Validation Rules
- `--additional-features` must be a JSON object; reject arrays, strings, numbers, booleans, or null with a descriptive error.
- Reject local feature paths: values beginning with `./`, `../`, `/`, or `file://`.
- Canonical feature ID: trim surrounding whitespace; no further normalization.

## Merge Semantics
- Source of features: devcontainer config features + CLI `--additional-features`.
- Shallow merge on colliding IDs with CLI precedence:
  - Scalars: CLI overwrites config.
  - Objects/Arrays: CLI value replaces config value wholesale (no deep merge; no concatenation).

## Processing Stages (high level)
1. Parse inputs; validate `--additional-features` type; reject local paths.
2. Canonicalize feature IDs by trimming; deduplicate.
3. Fetch metadata for each feature (fail fast on errors).
4. Build `graph` from union of `installsAfter` and `dependsOn` (dedup + sort lexicographically).
5. Topologically sort to produce `order` with lexicographic tie-breakers.
6. Emit `{ order, graph }` JSON to stdout.
