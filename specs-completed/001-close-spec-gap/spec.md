# Feature Specification: Close Spec Gap (Features Plan)

**Feature Branch**: `001-close-spec-gap`  
**Created**: 2025-11-01  
**Status**: Draft  
**Input**: User description: "Close the GAP in the SPEC"

## Clarifications

### Session 2025-11-01

 - Q: When the same feature appears both in the devcontainer config and via `--additional-features`, how should option maps be merged? → A: Shallow override by CLI — option maps are merged shallowly with CLI precedence; for conflicting keys the CLI value replaces the config value; collection values (objects, arrays) are replaced wholesale (no deep merge or concatenation).
 - Q: If planning cannot fetch metadata for any referenced feature (e.g., network/auth/404), how should the planner behave? → A: Fail fast on first fetch failure; emit a clear error; no plan output.
 - Q: When multiple features are independent (no ordering constraints), how should the planner break ties to keep the installation order deterministic? → A: Lexicographic by canonical feature ID (string compare).
 - Q: What is the canonical feature ID used for sorting and identity? → A: Exact provided ID after trimming whitespace; no case, host, or path normalization.
 - Q: Should the `graph` list only direct dependencies per feature, or include the full transitive closure? → A: Direct dependencies only (deduplicated union of `installsAfter` and `dependsOn`); no transitive closure in output.

## User Scenarios & Testing *(mandatory)*

<!--
  IMPORTANT: User stories should be PRIORITIZED as user journeys ordered by importance.
  Each user story/journey must be INDEPENDENTLY TESTABLE - meaning if you implement just ONE of them,
  you should still have a viable MVP (Minimum Viable Product) that delivers value.
  
  Assign priorities (P1, P2, P3, etc.) to each story, where P1 is the most critical.
  Think of each story as a standalone slice of functionality that can be:
  - Developed independently
  - Tested independently
  - Deployed independently
  - Demonstrated to users independently
-->

### User Story 1 - Clear input validation for additional features (Priority: P1)

As a CLI user planning features, when I pass `--additional-features`, I get immediate, clear validation that it must be a JSON object; invalid inputs produce a descriptive error that references the flag and why it failed.

**Why this priority**: Prevents confusion and wasted time; aligns behavior with the published specification and makes the tool trustworthy.

**Independent Test**: Run the plan command with invalid `--additional-features` (non-JSON, JSON array, string); observe a single descriptive error without proceeding to planning.

**Acceptance Scenarios**:

1. **Given** a user runs the command with `--additional-features '[1,2,3]'`, **When** input is parsed, **Then** the tool exits with a message like "--additional-features must be a JSON object" and no plan output is produced.
2. **Given** a user runs with `--additional-features '{"id":42}'` (valid object), **When** planning executes, **Then** the plan includes merged features per merge rules without errors.

---

### User Story 2 - Explicit rejection of local feature paths (Priority: P2)

As a user, if I mistakenly specify a local path as a feature ID, the tool clearly rejects it with guidance to use registry references, rather than failing later with an unclear parse error.

**Why this priority**: Reduces confusion and aligns with the current scope (registry features only) while setting expectations.

**Independent Test**: Provide a local path (e.g., `./features/foo`) as a feature key and verify the tool exits with a clear error message that mentions local paths are not supported by planning.

**Acceptance Scenarios**:

1. **Given** a config that includes a feature key `"./my-local-feature"`, **When** the plan command runs, **Then** it fails fast with a message like "Local feature paths are not supported by 'features plan'—use a registry reference".

---

### User Story 3 - Deterministic order and complete graph output (Priority: P3)

As a developer or CI system, I get a deterministic installation order and a graph that combines `installsAfter` and `dependsOn` constraints, with stable sorting to make diffs and debugging easy.

**Why this priority**: Predictability improves reliability for CI and reviews; it also matches the spec's guidance and existing examples.

**Independent Test**: Provide a small set of features with known relationships; verify that repeated runs return identical order and graph, and that the graph uses dependencies (edges from a feature to what it depends on).

**Acceptance Scenarios**:

1. **Given** features A and B where B installs after A, **When** planning runs, **Then** the order lists A before B and the graph for B includes A.
2. **Given** features with both `installsAfter` and `dependsOn`, **When** planning runs, **Then** the graph lists the deduplicated union of both for each feature with lexicographic ordering.

---

[Add more user stories as needed, each with an assigned priority]

### Edge Cases

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right edge cases.
-->

- Empty features map returns an empty order and an empty graph.
- `--additional-features` provided as non-object JSON (array, string, number) yields a single, clear validation error; planning does not proceed.
- Circular `dependsOn` relationships yield an error that identifies the cycle participants in human-readable form.
- Local feature references (e.g., `./path`) produce an explicit "not supported" error with guidance.
- Unsupported option types in feature options are passed through as opaque values and do not alter plan shape.
 - Option values of any type (objects, arrays, numbers, booleans, strings) are accepted and passed through as-is; planning does not normalize or transform option values.
 - Any registry metadata fetch failure (network error, 401/403 auth failure, 404 not found) yields a single fatal error with categorized reason and no plan output.
 - Graph adjacency lists include only direct dependencies for each feature; transitive dependencies are not included in the output graph.

## Requirements *(mandatory)*

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right functional requirements.
-->

### Functional Requirements

- **FR-001 (Input Validation)**: The command MUST validate that `--additional-features` is a JSON object; otherwise, fail fast with a clear error that references the flag and expected format.
- **FR-002 (Scope Guardrails)**: The command MUST reject local feature paths with a clear, actionable message instructing users to use registry references for planning.
- **FR-003 (Deterministic Output)**: The command MUST produce deterministic results (order and graph) for the same inputs, including stable, lexicographic ordering for graph adjacency lists.
  - Among independent features (no ordering constraints), ties MUST be broken by lexicographic sort of the canonical feature ID (string compare).
- **FR-004 (Graph Semantics)**: The command MUST output a graph where each node maps to an array of its direct dependencies, defined as the deduplicated union of `installsAfter` and `dependsOn`. The output graph MUST NOT include transitive dependencies; adjacency lists use stable lexicographic ordering.
- **FR-005 (Error Reporting for Cycles)**: The command MUST surface dependency cycle errors that identify cycle participants and do not emit partial plans.
- **FR-006 (Merge Behavior)**: The command MUST merge CLI-provided features additively. If a feature appears in both the devcontainer config and `--additional-features`, apply a shallow merge of option maps with CLI precedence: for conflicting keys, replace the config value with the CLI value; collection values (objects and arrays) are replaced wholesale (no recursive/deep merge and no array concatenation).
- **FR-007 (User-Facing Messaging)**: Error messages for common user errors (invalid JSON, local paths, fetch failures) MUST reference the offending input and provide next-step guidance.
- **FR-008 (Documentation Note)**: The command MUST document (via help text or user-facing docs) that variable substitution is not performed during planning; feature IDs are opaque and option values pass through.
- **FR-009 (Auth Expectation)**: The command MUST state that registry access uses standard authentication where needed; failures should indicate authentication problems distinctly from not found.
- **FR-010 (Option Types)**: For planning purposes, option values that are objects, arrays, numbers, booleans, or strings SHALL be accepted and passed through without mutation; no normalization is performed.
- **FR-011 (Registry Fetch Failures)**: If metadata fetch for any referenced feature fails (e.g., network, authentication, not found), the planner MUST fail fast with a clear error and MUST NOT emit a partial plan. Errors SHOULD distinguish authentication failures (401/403) from not found (404) and transient network errors.
 - **FR-012 (Canonical Feature ID Definition)**: The planner MUST define the canonical feature ID as the exact provided ID after trimming surrounding whitespace, with no additional normalization (no case folding, no registry host/path normalization, and no default tag insertion). All deterministic ordering and identity checks MUST use this canonical form.

## Assumptions

- Planning supports registry feature references only; local feature paths are out of scope and will be rejected with a clear message.
- Option value types are not normalized; they are opaque for planning and carried through unchanged.
- Performance optimizations (e.g., parallel metadata retrieval) are out of scope for this feature; correctness and clarity take precedence.

### Key Entities *(include if feature involves data)*

- **Plan**: An artifact with two parts: `order` (array of feature IDs) and `graph` (map of feature ID to dependency array). It is read-only and consumed by users/CI for auditing and previews.
- **Feature Reference**: An identifier for a feature, expected to be a registry reference for planning. Local paths are out of scope for this command.
 - **Canonical Feature ID**: The exact provided feature ID after trimming surrounding whitespace. No further normalization is applied (no case folding; no host/path normalization; tags/digests remain as provided). This canonical form is used for equality checks and lexicographic tie-breaking in ordering.

## Success Criteria *(mandatory)*

<!--
  ACTION REQUIRED: Define measurable success criteria.
  These must be technology-agnostic and measurable.
-->

### Measurable Outcomes

- **SC-001**: 100% of invalid `--additional-features` inputs (non-object JSON) result in a single, descriptive error without emitting any plan output.
- **SC-002**: 100% of local feature path inputs are rejected with a single, descriptive error indicating the need for registry references.
- **SC-003**: For identical inputs, repeated runs produce identical `order` and `graph` outputs across at least 10 consecutive runs.
- **SC-004**: Dependency cycle cases reliably produce an error that lists cycle participants; no partial plan is emitted.
- **SC-005**: The graph combines `installsAfter` and `dependsOn` with deduplicated entries, sorted lexicographically, verified via tests covering simple chain, fan-in, and union cases.
- **SC-006**: Help/docs clearly state that variable substitution is not performed during planning and that registry auth may be required.
- **SC-007**: When the same feature is present in both config and CLI, overlapping option keys are resolved by shallow merge with CLI precedence; objects and arrays are replaced wholesale (no deep merge/concat), verified by tests with conflicting scalar/object/array keys.
- **SC-008**: Injected registry failures (401/403 auth, 404 not found, simulated network errors) cause a single clear error with categorized reason and no plan output; partial plans are never emitted.
- **SC-009**: For inputs with independent features (no constraints), the installation order is lexicographically sorted by canonical feature ID, verified with mixed-case and multi-namespace IDs.
 - **SC-010**: Canonical ID behavior: inputs with surrounding whitespace compare/sort identically after trimming; case differences do not collapse (case-sensitive compare), and no implicit host/tag normalization occurs.
 - **SC-011**: For a chain A → B → C, `graph["C"]` lists only `B` (not `A`); and for fan-in/fan-out cases, adjacency lists include only direct dependencies (no transitive closure), with lexicographic ordering.
