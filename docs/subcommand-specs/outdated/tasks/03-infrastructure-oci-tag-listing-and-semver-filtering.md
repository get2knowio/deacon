# [Infrastructure] OCI Tag Listing and Semver Filtering

Labels:
- subcommand: outdated
- type: infrastructure
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Extend the OCI client to list tags for a given Feature reference, filter to semver-valid tags, and provide ascending and descending orderings. This supports computing `latest` and `wanted` versions in the `outdated` command.

## Specification Reference

- From SPEC.md Section: §7. External System Interactions (OCI Registries)
- From GAP.md Section: 4.1 Tag Listing & Filtering; 11 Dependencies (add `semver` crate)

### Expected Behavior
- GET `/v2/<namespace>/<id>/tags/list` to retrieve tags for a registry path.
- Filter to valid semver tags per Theme 4 rules; sort ascending, provide reversed (descending).
- Provide a function to return tags as `Vec<semver::Version>` and as `Vec<String>`.

### Current Behavior
- Partial OCI support exists but lacks tag listing and semver filtering for this flow.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/core/src/oci.rs`
  - Add async `list_tags(path: &str) -> anyhow::Result<Vec<String>>` using existing HTTP client.
  - Add `filter_semver_tags(tags: &[String]) -> Vec<semver::Version>`.
  - Add `sort_semver_asc(versions: &mut [semver::Version])` helper.
- `crates/core/Cargo.toml`
  - Add dependency `semver = "^1"` (workspace if appropriate).
- Tests under `crates/core/tests/`
  - Unit tests for filtering and sorting.
  - Mock HTTP for tag endpoint with realistic payloads.

#### Specific Tasks
- [ ] Implement tag listing with authentication if required by existing client.
- [ ] Ensure network failures surface as errors (graceful handling is in caller task).
- [ ] Implement semver validation using `semver` crate, excluding non-semver tags.

### 2. Data Structures
- Use `semver::Version` for internal sorting; convert to strings at boundaries.

### 3. Validation Rules
- [ ] Exclude pre-release tags only if spec requires (default: include unless otherwise stated). If exclusion is required, mirror upstream behavior.
- [ ] Ensure stable ordering for equal versions (not expected but keep deterministic behavior).

### 4. Cross-Cutting Concerns
- Theme 4 - Semantic Versioning Operations: consistent parsing, sorting, filtering.
- Theme 6 - Error Messages: provide context in tag listing failures.

## Testing Requirements

### Unit Tests
- [ ] `filter_semver_tags` excludes non-semver values.
- [ ] Sorting ascending then reversing yields expected `latest`.

### Integration Tests
- [ ] Mock registry: tags list returns sample data; verify behavior.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] OCI tag listing available and tested.
- [ ] Semver filtering/sorting utilities implemented and tested.
- [ ] CI passes.

## Implementation Notes
- Reuse existing HTTP client trait and auth logic per core guidelines.
- Consider retry/backoff already present in OCI client.

### Edge Cases to Handle
- Empty tags array.
- Non-semver tags only → return empty semver list.
- Network errors (timeouts, 5xx) → return error.

### References
- GAP: §4.1, §11
- SPEC: §7