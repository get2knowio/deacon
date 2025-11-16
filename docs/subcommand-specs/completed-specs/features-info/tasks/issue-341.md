# [features info] Tests: unit, integration, and smoke coverage

https://github.com/get2knowio/deacon/issues/341

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Add comprehensive tests to validate behavior of `features info` across all modes and formats, including JSON-mode error behavior. Keep tests hermetic, using mock OCI where necessary.

## Specification Reference
- From SPEC.md Section: §15. Testing Strategy
- From GAP.md Section: 5. Test Coverage Gaps

## Implementation Requirements

### Files & Structure
- `crates/deacon/tests/integration_features_info.rs`
- Unit tests alongside new utilities (boxed formatting, error helper)

### Test Matrix
- [ ] Manifest (public/private) and error paths
- [ ] Tags (non-empty, empty, pagination) and errors
- [ ] Dependencies text-only output
- [ ] Verbose composition (text and JSON)
- [ ] JSON-mode `{}` and exit code 1 on errors

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include minimal `features info` invocations

### Examples & Fixtures
- [ ] Add fixtures under `fixtures/features/` if needed (mock metadata)

## Acceptance Criteria
- [ ] All tests pass locally and in CI
- [ ] No clippy warnings; rustfmt clean

## Dependencies
Blocked By: #335, #336, #337, #339, #340 (error helper)
