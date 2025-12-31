---
subcommand: set-up
type: enhancement
priority: high
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: tests", "area: docs"]
---

# [set-up] Add tests, examples, and documentation

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation
- [x] Other: Documentation

## Description
Implement the integration test suite described in SPEC §15, add smoke tests, and provide examples and README updates. This consolidates verification and user guidance for the new subcommand.

## Specification Reference

**From SPEC.md Section:** §15 Testing Strategy

**From GAP.md Section:** 7. Testing Strategy (0/7 tests implemented), 8. Documentation Gaps

### Expected Behavior
- Integration tests cover lifecycle behavior, metadata-driven hooks, containerEnv substitution, skip flags, and dotfiles idempotency.
- Smoke tests updated to include set-up help and a minimal run.
- Examples added under `examples/set-up/` with a README and simple config.

### Current Behavior
- No tests or examples for set-up exist.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/tests/integration_set_up.rs` – New integration tests implementing all 7 cases from SPEC §15.
- `crates/deacon/tests/smoke_basic.rs` – Add minimal set-up help/run assertions resilient to missing Docker.
- `examples/set-up/basic/` – Add example configuration and README.
- `EXAMPLES.md` – Add entry for set-up example.
- `README.md` – Add set-up usage section.

#### Specific Tasks
- [ ] Write tests:
  - `config postAttachCommand`
  - `metadata postCreateCommand`
  - `include-config`
  - `remote-env substitution`
  - `invalid remote-env`
  - `skip-post-create`
  - `dotfiles install`
- [ ] Add fixtures as needed under `fixtures/`.

### 2. Data Structures
- Use result JSON shapes for assertions.

### 3. Validation Rules
- [ ] Tests must assert exact error messages per Theme 6.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 5 - Marker Idempotency (tests verify)
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Covered in prior issues for units; this issue focuses on integrations.

### Integration Tests
- [ ] All 7 tests implemented per SPEC §15.

### Smoke Tests
- [ ] Update smoke tests to include set-up.

### Examples
- [ ] Examples folder and README updated.

## Acceptance Criteria
- [ ] All tests pass locally and in CI.
- [ ] Examples documented and runnable.

## Implementation Notes
- Where Docker is unavailable, assert well-defined error instead of hard failing.

## Definition of Done
- [ ] Tests, examples, and docs in place and passing.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§15)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§7, §8)
