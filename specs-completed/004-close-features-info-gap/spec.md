# Feature Specification: Features Info GAP Closure

**Feature Branch**: `004-close-features-info-gap`  
**Created**: 2025-11-01  
**Status**: Draft  
**Input**: User description: "Close the GAP in the features-info SPEC"

Note: In this repository, the CLI binary is named `deacon`. All examples below use `deacon features info ...`.

## Clarifications

### Session 2025-11-01

- Q: JSON behavior for verbose mode on partial failure → A: Partial fields + errors; exit 1.
- Q: canonicalId for local feature refs in JSON → A: Include "canonicalId": null when not applicable.
- Q: Sensible defaults for timeouts and pagination → A: 10s/request timeout, 10 pages max, 1000 tags cap.
- Q: Dependencies failure effect in verbose JSON → A: Include `errors.dependencies` and exit 1.

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

### User Story 1 - Inspect manifest and canonical ID (Priority: P1)

Users and CI systems can retrieve the OCI manifest and canonical identifier for a feature reference to verify integrity and provenance.

**Why this priority**: Core verification capability; required by spec and used by CI for validation.

**Independent Test**: Run `deacon features info manifest <ref>` and verify manifest JSON and canonicalId in text and JSON modes independently of other modes.

**Acceptance Scenarios**:

1. **Given** a public feature ref (e.g., `ghcr.io/devcontainers/features/node:1`), **When** running in text mode, **Then** the CLI prints a boxed "Manifest" section with formatted JSON and a boxed "Canonical Identifier" section including registry, path, and digest (e.g., `@sha256:...`).
2. **Given** the same ref, **When** running with `--output-format json`, **Then** output is a single JSON object containing keys `manifest` and `canonicalId` and exit code is 0.
3. **Given** a non-existent ref, **When** running with `--output-format json`, **Then** the CLI prints `{}` and exits with code 1.
4. **Given** a local feature path without an OCI digest, **When** running with `--output-format json`, **Then** output contains `manifest` and `"canonicalId": null`, and exit code is 0.

---

### User Story 2 - Discover published tags (Priority: P1)

Users can list all published tags for a feature to select a desired version.

**Why this priority**: Essential discovery function for consumers; required by spec.

**Independent Test**: Run `deacon features info tags <ref>` and verify output shape and error handling in both formats.

**Acceptance Scenarios**:

1. **Given** a public feature ref pointing to a repository, **When** running in text mode, **Then** the CLI prints a boxed "Published Tags" section listing tags sorted lexicographically or registry order.
2. **Given** the same ref, **When** running with `--output-format json`, **Then** output contains `{ "publishedTags": [ ... ] }` and exit code 0.
3. **Given** a repo with no tags or inaccessible registry, **When** running in JSON mode, **Then** the CLI prints `{}` and exits with code 1.

---

### User Story 3 - Visualize dependency graph (Priority: P2)

Authors can view a dependency tree of a feature as Mermaid syntax to understand ordering and relationships.

**Why this priority**: Aids reasoning about installation order and transitive relationships; aligns with design but not critical for machine consumption.

**Independent Test**: Run `deacon features info dependencies <ref>` in text mode and verify a boxed Mermaid graph is printed with a render hint; confirm no JSON output is produced.

**Acceptance Scenarios**:

1. **Given** a feature with `dependsOn` and/or `installsAfter`, **When** running in text mode, **Then** the CLI prints a boxed section titled "Dependency Tree (Render with https://mermaid.live/)" followed by valid Mermaid `graph TD` syntax.
2. **Given** any ref, **When** running with `--output-format json` and `mode=dependencies`, **Then** the CLI exits 1 with an explanatory error in text mode or `{}` in JSON mode (per spec decision to not emit graph JSON).

---

### User Story 4 - Combined verbose view (Priority: P2)

Users can see manifest + canonicalId, published tags, and dependency graph in one command for convenience.

**Why this priority**: Improves ergonomics by aggregating outputs; not blocking for core functionality.

**Independent Test**: Run `deacon features info verbose <ref>` and validate that text output contains all three boxed sections; in JSON mode only includes `manifest`, `canonicalId`, and `publishedTags`.

**Acceptance Scenarios**:

1. **Given** a valid ref, **When** running in text mode, **Then** the CLI outputs three boxed sections in order: Manifest/Canonical Identifier, Published Tags, Dependency Tree.
2. **Given** the same ref, **When** running with `--output-format json`, **Then** output is a single JSON object union of manifest/canonicalId and publishedTags; no dependency graph is included.
3. **Given** a valid ref where dependency graph generation fails, **When** running with `--output-format json`, **Then** output includes successfully retrieved fields (manifest/canonicalId and/or publishedTags as applicable), plus an `errors` object containing a `dependencies` key with a brief message, and the process exits with code 1.
4. **Given** a valid ref where at least one sub-mode fails (e.g., tags listing times out), **When** running with `--output-format json`, **Then** output includes the successfully retrieved fields and an `errors` object keyed by sub-mode (e.g., `{"errors": {"tags": "<message>"}}`) and the process exits with code 1.

---

[Add more user stories as needed, each with an assigned priority]

### Edge Cases

- Private registries require authentication: on auth failure, return `{}` in JSON mode (exit 1) or clear text error in text mode.
- Very large tag lists: handle pagination via `Link` headers; enforce defaults of 10 pages max and 1000 tags total; ensure no infinite loops.
- Invalid feature reference format: exit 1; `{}` for JSON mode.
- Local feature paths without OCI manifest: manifest mode should read local metadata when applicable; in JSON include `"canonicalId": null`.
- Network failures/timeouts: exit 1; JSON mode outputs `{}`.
- Verbose JSON partial failure: include partial fields plus an `errors` object; exit 1.
  - If dependency graph generation fails, set `errors.dependencies` and exit 1.

## Requirements *(mandatory)*

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right functional requirements.
-->

### Functional Requirements

- **FR-001 (CLI flags)**: Provide `--output-format <text|json>` (default: text) and `--log-level <info|debug|trace>` flags; remove/replace legacy `--json` boolean.
- **FR-002 (Manifest mode)**: Fetch the OCI manifest for registry refs; for local refs read available metadata from `devcontainer-feature.json` in the feature directory. On success, output boxed Manifest (text) and JSON `{manifest}` (json mode). Errors: `{}` and exit 1 (json mode).
  - JSON key stability: Always include `canonicalId` key; for local refs (no digest), set `canonicalId` to `null`.
- **FR-003 (Canonical ID)**: Calculate canonical identifier from the registry response digest: `{registry}/{namespace}/{name}@{sha256:...}`; print as a separate boxed section (text) and as `canonicalId` in JSON mode. For local refs or when no digest is available, include `"canonicalId": null` in JSON; in text mode indicate `N/A`.
- **FR-004 (Tags mode)**: Query the registry tags list endpoint `/v2/<name>/tags/list` with pagination via `Link` headers; per-request timeout 10s; stop after at most 10 pages or 1000 tags (whichever comes first). On success, output boxed "Published Tags" (text) and `{ "publishedTags": [...] }` (json). Empty/missing: `{}` with exit 1 (json mode).
- **FR-005 (Dependencies mode)**: Build a dependency graph from `dependsOn` and `installsAfter` fields present in feature metadata and render Mermaid (`graph TD`). Text-only. If invoked in JSON mode, return error (`{}` with exit 1).
- **FR-006 (Verbose mode)**: Delegate to Manifest, Tags, and Dependencies modes; in text, print three boxed sections; in JSON, output union of `{manifest, canonicalId, publishedTags}` only (no graph data).
  - JSON partial failure policy: If any sub-mode (manifest, tags, dependencies) fails, still return available successful fields and include an `errors` object keyed by the failing sub-mode(s); exit code MUST be 1.
  - JSON key stability: Always include `canonicalId`; when not applicable (local refs), set to `null`.
- **FR-007 (Boxed text formatting)**: Use Unicode box drawing for section titles sized to content width; content follows the header line.
- **FR-008 (Error handling & exit codes)**: For any error in JSON mode, print `{}` and exit 1; in text mode, print human-readable error and exit 1. Success exit code is 0.
  - Exception: In verbose JSON mode, follow FR-006 partial failure policy (return partial fields with `errors` and exit 1) instead of `{}`.
- **FR-009 (Auth & security)**: Support bearer-token auth flows per OCI Distribution Spec; never print secrets in output; respect Docker config env.
- **FR-010 (Deterministic output)**: Sort tag lists consistently (registry order if provided; otherwise ascending lexicographic across the full union). Tie-breakers: when registry returns paged results with unspecified order, perform a stable ascending lexicographic sort on the aggregated set. Ensure JSON keys and formatting are stable across runs.

### Key Entities *(include if feature involves data)*

- **Feature Reference**: A string representing a feature location (local path or `registry/namespace/name[:tag]`).
- **OCI Manifest**: The manifest object returned by registry for a feature reference; digest used for canonicalId.
- **Published Tags**: A list of version strings published for a feature repository.
- **Dependency Graph**: A directed graph derived from `dependsOn` and `installsAfter` relationships, rendered as Mermaid.

## Success Criteria *(mandatory)*

<!--
  ACTION REQUIRED: Define measurable success criteria.
  These must be technology-agnostic and measurable.
-->

### Measurable Outcomes

- **SC-001**: In JSON mode, error cases for invalid ref, missing manifest, or no tags produce `{}` and exit code 1 in 100% of tested scenarios.
- **SC-002**: Manifest mode returns both `manifest` and `canonicalId` with a valid sha256 digest for public refs; verified against at least 2 known features.
- **SC-003**: Tags mode returns 3+ tags for a known public feature and completes within 3 seconds on a typical network connection.
- **SC-004**: Dependencies mode emits valid Mermaid syntax that renders without errors on mermaid.live for a sample feature with at least 2 relationships.
- **SC-005**: Verbose mode (text) displays all three boxed sections; JSON mode includes only `manifest`, `canonicalId`, and `publishedTags`.
- **SC-006**: Boxed text headers render correctly in a standard 80-column terminal without wrapping artifacts.
