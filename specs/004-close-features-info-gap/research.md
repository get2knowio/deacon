# Phase 0 Research — Features Info GAP Closure

This document consolidates decisions resolving implementation details and best practices for `features info`.

## Decisions

### 1) Output contracts and partial failure policy
- Decision: JSON mode prints only data objects; for `verbose`, include `manifest`, `canonicalId`, `publishedTags` and, on sub-mode failures, include `errors` keyed by failing sub-mode(s). Exit code MUST be 1 on any error in JSON mode, including partial `verbose` failures.
- Rationale: Aligns with spec and constitution Principle V (stdout contract) and FR-006 in the feature spec.
- Alternatives considered: Emit dependency graph JSON; rejected for complexity and limited value (keep Mermaid text only).

### 2) Boxed text formatting
- Decision: Use Unicode box drawing for section headers sized to content width; content printed below header (Manifest JSON pretty-printed; tags as lines; dependencies as Mermaid).
- Rationale: Improves readability and matches TS CLI ergonomics; covered by FR-007.
- Alternatives considered: Plain headers without boxes; rejected due to ergonomics and spec alignment.

### 3) Registry interactions: manifests and tags
- Decision: Implement registry calls per OCI Distribution Spec v2.
  - Manifests: GET `\-/v2/<name>/manifests/<tag|digest>`; parse digest to compute `canonicalId`.
  - Tags: GET `/v2/<name>/tags/list` with pagination via `Link` headers; stop after 10 pages or 1000 tags, per-request timeout 10s.
  - Sort tags by server order if provided; otherwise lexicographically ascending.
- Rationale: Matches FR-002/FR-004 and repository OCI guidance.
- Alternatives considered: Client-side page size tweaks or unlimited pagination; rejected per constraints.

### 4) Authentication
- Decision: Support bearer-token flows and Docker config helpers via the core registry client; no secrets in stdout/stderr; redact in logs.
- Rationale: Principle V and FR-009.
- Alternatives considered: Anonymous-only; rejected as incompatible with private registries.

### 5) Error handling and exit codes
- Decision: `{}` + exit 1 for JSON errors (except `verbose` partials which still exit 1 but include partial fields and an `errors` object). Text errors are human-readable with exit 1. Always include `canonicalId` in JSON (null for local refs).
- Rationale: FR-008 and FR-006; Principle III (fail fast).
- Alternatives considered: Exit 0 with partial data; rejected for automation predictability.

### 6) Dependency graph emission
- Decision: Mermaid `graph TD` text only; no JSON schema for graph. In JSON mode when `mode=dependencies`, return `{}` and exit 1.
- Rationale: Spec and feature decision; ergonomics and simplicity.
- Alternatives considered: Emit graph JSON; rejected.

### 7) Testing strategy
- Decision: Unit tests for parsing/formatting; CLI smoke tests for each mode and JSON policies. Avoid network in unit tests; mock registry responses in core tests. Allow opt-in integration tests for live registries gated in CI.
- Rationale: Constitution (deterministic tests) and repo guidance.
- Alternatives considered: Live network tests by default; rejected.

## Open Questions (resolved)
- None. The feature spec and subcommand spec provide sufficient guidance.
