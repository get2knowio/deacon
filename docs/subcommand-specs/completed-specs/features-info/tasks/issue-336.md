# [features info] Implement OCI tag listing with pagination and auth

https://github.com/get2knowio/deacon/issues/336

## Issue Type
- [x] Core Logic Implementation
- [ ] Missing CLI Flags
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Add support for listing tags for a repository (feature) using the OCI client, handling Link-based pagination and authentication. Ensure JSON mode exposes `publishedTags` as an array. Handle 401/403 gracefully and propagate errors per JSON/text rules.

## Specification Reference
- From SPEC.md Section: §7. Tags Listing
- From GAP.md Section: 4.2 Tags Pagination & Auth

### Expected Behavior
- Tags mode fetches all tags, transparently following pagination using Link headers.
- JSON output contains `publishedTags: ["v1","v1.2.0", ... ]`.
- In text mode present tags boxed or as a simple list.

### Current Behavior
- Basic single-page tag fetching or none.

## Implementation Requirements

### Files to Modify
- `crates/core/src/oci.rs` (add list_tags pagination support)
- `crates/deacon/src/commands/features.rs` (expose tags in output)

### Specific Tasks
- [ ] Implement `list_tags(repo) -> Vec<String>` handling Link pagination
- [ ] Accept and use auth tokens/cookies as required by OCI registries
- [ ] Unit tests mocking Link header pagination

## Testing Requirements
- Unit Tests:
  - [ ] Pagination sequence handling
  - [ ] Auth-required registry returns 401 -> error path
- Integration Tests:
  - [ ] `features info --output-format json` includes `publishedTags`

## Acceptance Criteria
- [ ] Tags listing correct and paginates
- [ ] JSON output shape validated in tests
- [ ] CI checks pass

## Dependencies
Blocked By: #324, #335
