---
subcommand: upgrade
type: enhancement
priority: high
scope: large
---

# [upgrade] Tests: Unit, Integration, Smoke

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation
- [ ] Error Handling
- [ ] Other: ___________

## Description
Add a comprehensive test suite for the `upgrade` subcommand covering CLI validation, config discovery, lockfile generation, dry-run, feature pinning behavior, and error handling. Include smoke tests updates.

## Specification Reference

**From SPEC.md Section:** §15. Testing Strategy

**From GAP.md Section:** 6. Testing — Missing Implementation

### Expected Behavior
- Unit tests verify utilities (regex, base ID extraction, path derivation, JSON ordering)
- Integration tests validate end-to-end flows (basic upgrade, dry-run, pinning, error cases)
- Smoke tests updated with `upgrade` presence and basic invocation

### Current Behavior
- No tests exist for `upgrade`. Core lockfile has its own tests.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify/Add
- `crates/deacon/src/commands/upgrade.rs` (add unit-testable helpers under `#[cfg(test)]`)
- `crates/deacon/tests/integration_upgrade.rs` (new)
- `crates/deacon/tests/smoke_basic.rs` (update)
- Fixtures under `fixtures/` for configs and expected lockfiles

#### Specific Tasks
- [ ] Unit: argument pairing and version regex validator
- [ ] Unit: `get_lockfile_path` already tested in core; add upgrade wrapper tests if any
- [ ] Unit: base ID extraction and text replacement helper
- [ ] Integration: basic lockfile write
- [ ] Integration: dry-run prints JSON and no file created
- [ ] Integration: pin feature then regenerate
- [ ] Integration: config not found error
- [ ] Integration: invalid target-version format error
- [ ] Integration: missing pairing error
- [ ] Integration: empty features config handled
- [ ] Smoke: `upgrade --help` presence and nonzero on bad inputs

### 2. Data Structures
- Reuse from prior tasks and core

### 3. Validation Rules
- [ ] Exact error messages asserted

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON output contract for dry-run
- [ ] Theme 2 - CLI validation
- [ ] Theme 6 - Error messages

## Testing Requirements

### Unit Tests
- See above checklist

### Integration Tests
- See above checklist

### Smoke Tests
- See above checklist

### Examples
- [ ] None in this task

## Acceptance Criteria
- [ ] All tests implemented and passing locally
- [ ] CI build/test/fmt/clippy green

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§15)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§6)
