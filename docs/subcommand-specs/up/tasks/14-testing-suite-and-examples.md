# [up] Add tests, smoke, and examples per spec

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Testing & Validation

## Description
Implement the test coverage specified for `up`, including unit tests for parsers and lifecycle logic, integration tests for happy paths and error cases, and update smoke tests and examples to reflect new JSON output and flags.

## Specification Reference
- From SPEC.md Section: §15. Testing Strategy
- From GAP.md Section: §14 Missing Tests

### Expected Behavior
- Tests covering: happy path image, happy path features, compose image, compose Dockerfile, missing config, invalid mount, invalid remote-env, skip-post-create, prebuild, remove-existing, expect-existing, include-config output.

### Current Behavior
- Only a subset of unit tests exists; many integration and smoke tests missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/tests/` - add integration tests for `up`.
- `crates/deacon/tests/smoke_basic.rs` - update to check JSON stdout and stderr logs.
- `examples/` - add or update examples relevant to the new flags.
- `fixtures/` - add minimal fixtures for configs and compose scenarios.

### 2. Data Structures
- N/A

### 3. Validation Rules
- Ensure tests assert exact error messages and JSON structure.

### 4. Cross-Cutting Concerns
- [x] Theme 1 - JSON Output Contract.
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Implement test cases listed in SPEC §15.

## Acceptance Criteria
- All new tests pass locally; CI green; examples updated.

## References
- `docs/subcommand-specs/up/SPEC.md` (§15)
- `docs/subcommand-specs/up/GAP.md` (§14)
