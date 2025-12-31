# [outdated] Version Resolution and Digest Metadata Helpers

Labels:
- subcommand: outdated
- type: enhancement
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Implement helpers to compute `wanted`, `current`, and `latest` versions given a Feature ref, optional lockfile entry, and published tags, including support for digest-based identifiers by fetching manifest/metadata when needed.

## Specification Reference

- From SPEC.md Section: §5. Core Execution Logic (version computation); §7. External System Interactions
- From GAP.md Section: 2.3 Version Resolution; 5.2 Version Resolution

### Expected Behavior
- `wanted` resolution:
  - No tag/digest → treat as `latest` and select highest semver tag.
  - Specific tag/range → pick highest matching semver tag.
  - Digest → if lockfile version missing, attempt metadata fetch to determine version; else leave undefined.
- `current = lockfile.version || wanted`.
- `latest = highest published semver`.
- Compute majors for wanted/latest.

### Current Behavior
- Not implemented; no helpers exist.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/core/src/version.rs` (NEW)
  - `pub fn semver_major_str(v: &str) -> Option<String>`
  - `pub fn highest_matching<'a>(versions_desc: &'a [semver::Version], req: &str) -> Option<&'a semver::Version>`
  - `pub fn parse_feature_ref(id: &str) -> Option<OCIRef>` (or reuse existing if present)
- `crates/core/src/oci.rs`
  - Add `maybe_fetch_manifest_and_metadata(ref: &OCIRef) -> anyhow::Result<Option<String>>` (version string).
- Tests in `crates/core/tests/`
  - Unit tests for `semver_major_str` and `highest_matching`.
  - Mock-based tests for digest metadata returning version.

#### Specific Tasks
- [ ] Implement npm-like semver range support if spec requires; otherwise accept exact tags and `latest`.
- [ ] Ensure functions are pure and testable; network I/O isolated in OCI.

### 2. Data Structures
- Use `OCIRef` as in DATA-STRUCTURES; if already defined elsewhere, reference that type.

### 3. Validation Rules
- [ ] Return `None` for unparsable semver strings; do not panic.

### 4. Cross-Cutting Concerns
- Theme 4 - Semantic Versioning: unified version/range semantics across subcommands.
- Theme 6 - Error Messages: add context to metadata fetch failures.

## Testing Requirements

### Unit Tests
- [ ] `semver_major_str` yields correct majors.
- [ ] `highest_matching` respects ordering and selection.

### Integration Tests
- [ ] Digest metadata path returns version when available; returns `None` when absent.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] Version helpers available and tested.
- [ ] Digest metadata function implemented with error handling returning `Ok(None)` for not-found.
- [ ] CI passes.

## Implementation Notes
- Keep digest metadata download minimal and place temp files under OS temp dir; ensure cleanup by using temp dirs.

### Edge Cases to Handle
- No tags available.
- Only non-semver tags available.
- Digest metadata lacks version field.

### References
- GAP: §2.3, §5.2
- SPEC: §5, §7