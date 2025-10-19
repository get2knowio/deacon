# [outdated] Testing: Unit, Integration, Smoke, and Examples

Labels:
- subcommand: outdated
- type: testing
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Description
Add comprehensive tests for the `outdated` command per SPEC §15 and GAP §7, including unit tests for helpers, integration tests with mocked registries/lockfiles, smoke tests, and example updates.

## Specification Reference

- From SPEC.md Section: §15. Testing Strategy
- From GAP.md Section: 7. Testing Requirements; 9. Documentation Gaps

### Expected Behavior
- Test happy path JSON and text outputs, registry failure graceful behavior, and no-features scenario.
- Provide fixtures and examples for discoverability.

### Current Behavior
- No tests or examples exist.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/deacon/tests/` (new integration tests):
  - `integration_outdated_json.rs`
  - `integration_outdated_text.rs`
  - `integration_outdated_registry_failure.rs`
- `crates/deacon/tests/smoke_basic.rs`
  - Add smoke cases for `outdated --output-format json|text` with minimal config.
- `fixtures/` additions:
  - `fixtures/features/minimal/` configs and lockfiles as needed.
- `examples/` additions:
  - `examples/feature-management/outdated/` with README and small config.
- Optional: mocks under `crates/core/tests/` for OCI tag listing and metadata.

#### Specific Tasks
- [ ] Unit tests for rendering and ordering.
- [ ] Integration tests for end-to-end behavior with mocked HTTP.
- [ ] Smoke test updates to ensure command presence and baseline behavior.
- [ ] Example README snippets for users.

### 2. Data Structures
- Use `OutdatedResult` shape from DATA-STRUCTURES for JSON assertions.

### 3. Validation Rules
- [ ] Verify exit codes: 0 on success/partial, 1 on config missing.
- [ ] Verify exact error messages where specified (Theme 6).

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: assertions on JSON shape and field presence.
- Theme 6 - Error Messages: assert exact strings where applicable.

## Testing Requirements

### Unit Tests
- [ ] Rendering replaces missing values with `-` and strips version suffix.
- [ ] Ordering preserved.

### Integration Tests
- [ ] JSON/text outputs align with spec for mixed feature types.
- [ ] Registry failure yields undefineds but exit 0.
- [ ] No features → empty map/header-only.

### Smoke Tests
- [ ] Include `outdated` in smoke; ensure it runs without Docker.

### Examples
- [ ] Add/update example and index in `examples/README.md`.

## Acceptance Criteria
- [ ] Tests and examples added; all pass locally and in CI.
- [ ] Documentation updated accordingly.

## Implementation Notes
- Favor deterministic fixtures; no network calls in unit/integration tests.

### Edge Cases to Handle
- Mixed semver and non-semver tags.
- Digest refs without metadata.

### References
- SPEC: §15
- GAP: §7, §9