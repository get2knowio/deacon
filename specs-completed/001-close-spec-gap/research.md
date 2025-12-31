# Research — Close Spec Gap (Features Plan)

This document consolidates decisions and resolves all NEEDS CLARIFICATION items referenced in the plan.

## Inputs
- Spec: /workspaces/deacon/specs/001-close-spec-gap/spec.md
- Constitution: /workspaces/deacon/.specify/memory/constitution.md

## Decisions

### 1) Additional features input validation
- Decision: `--additional-features` must be a JSON object (map) when provided; any other JSON type is rejected with a clear error that references the flag.
- Rationale: Matches spec FR-001; prevents ambiguous merges; easy for users to correct.
- Alternatives considered: Accept array of tuples or stringified query formats — rejected for complexity and divergence from upstream spec.

### 2) Local feature path handling
- Decision: Reject local paths with an actionable error. A feature key is treated as a local path if it begins with `./`, `../`, `/`, or uses the `file://` scheme.
- Rationale: Planning is scoped to registry features only; failing fast reduces confusion (FR-002).
- Alternatives considered: Best-effort parsing of local paths — rejected per “No Silent Fallbacks”.

### 3) Canonical feature ID and ordering
- Decision: Canonical ID is the exact provided ID after trimming surrounding whitespace. Identity comparisons and sort keys use this canonical form. Lexicographic `Ord` string compare determines tie-breaks among independent features.
- Rationale: Matches FR-003/FR-012 and keeps deterministic outputs.
- Alternatives considered: Case-insensitive compare or host/path normalization — rejected; not in scope per spec.

### 4) Graph semantics
- Decision: For each feature, `graph[id]` lists the deduplicated union of `installsAfter` and `dependsOn` (direct dependencies only). Adjacency arrays are lexicographically sorted.
- Rationale: Aligns with FR-004 and ensures deterministic diffs.
- Alternatives considered: Include transitive closure — rejected; harms readability and deviates from spec.

### 5) Merge behavior for options
- Decision: Shallow merge with CLI precedence. Objects and arrays are replaced wholesale (no deep merge, no concatenation). Scalars are overwritten by CLI.
- Rationale: Matches clarified behavior in spec (FR-006).
- Alternatives considered: Deep merge for objects or array concatenation — rejected; unpredictable and harder to reason about.

### 6) Error reporting and fail-fast policy
- Decision: Abort planning on first fetch failure. Distinguish categories: 401/403 (auth), 404 (not found), and transient network errors; never emit partial plans.
- Rationale: FR-007/FR-008/FR-011; Constitution III (No Silent Fallbacks).
- Alternatives considered: Best-effort partial plan — rejected.

### 7) Output contracts
- Decision: JSON mode outputs only the plan JSON to stdout; all logs and diagnostics go to stderr via `tracing`. Provide a JSON Schema for the plan artifact.
- Rationale: Constitution V (Observability).
- Alternatives considered: Mixed stdout — rejected.

## Best Practices Consulted
- Rust CLI: clap for args; serde_json for robust parsing/serde; thiserror for domain errors; tracing for structured logs.
- Determinism: avoid HashMap iteration ordering for outputs; always sort keys/arrays for stable results.
- Testing: assert_cmd for CLI; unit tests for graph/merge logic; doctests compile; avoid network in tests.

## Open Questions (None)
All plan-level clarifications have been resolved above.
