# [features publish] Tests: semantic tagging, idempotency, auth, collection metadata

https://github.com/get2knowio/deacon/issues/330

<!-- Labels: subcommand:features-publish, type:test, priority:high, scope:medium -->

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Add unit, integration, and smoke tests covering key spec behaviors: semantic tagging, idempotent re-publish, authentication flows, and collection metadata publishing.

## Specification Reference
**From SPEC.md Section:** §15. Testing Strategy

**From GAP.md Section:** 8. Testing Gaps – Missing tests for semantic tags, idempotency, invalid version, auth, and collection metadata

### Expected Behavior
- Tests verify multi-tag publish, skip behavior on re-run, invalid version fails early, auth paths work, and collection metadata is uploaded.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/tests/` — Add integration tests (use mock registry where possible)
- `crates/core/tests/` — Add unit tests for OCI helpers (list tags, HEAD manifest)
- Update smoke tests in `crates/deacon/tests/smoke_basic.rs` if CLI output/flags changed

#### Specific Tasks
- [ ] Unit tests: semver tag computation
- [ ] Integration tests: first publish vs re-publish
- [ ] Integration tests: `DEVCONTAINERS_OCI_AUTH` path
- [ ] Integration tests: collection metadata upload
- [ ] JSON mode tests: output schema

### 2. Data Structures
- Use fixtures under `fixtures/features/` or create minimal ones

### 3. Validation Rules
- [ ] Ensure tests assert exact error messages (Theme 6)

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract
- [ ] Theme 4 - Semantic Versioning
- [ ] Theme 6 - Error Messages

## Acceptance Criteria
- [ ] All new tests pass locally and in CI
- [ ] Coverage meaningfully improved for this subcommand

## Dependencies

**Blocked By:** #322, #323, #324, #325, #327, #328, #329
