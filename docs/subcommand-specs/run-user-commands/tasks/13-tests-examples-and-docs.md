---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: tests", "area: docs"]
---

# [run-user-commands] Add tests, examples, and documentation

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation
- [x] Other: Documentation

## Description
Implement the integration test suite from SPEC §15, extend smoke tests, and add examples and README/EXAMPLES updates to document usage and behaviors (early exits, dotfiles, secrets, markers, substitution).

## Specification Reference

**From SPEC.md Section:** §15 Testing Strategy

**From GAP.md Section:** 9. Testing Gaps; 8. Security Gaps (tests for masking)

### Expected Behavior
- Integration tests cover happy path with markers, subfolder configs, invalid workspace errors, skip-non-blocking with waitFor, prebuild, skip-post-attach, and secrets masking.
- Examples demonstrate common flows with clear README.

### Current Behavior
- Limited or no tests/examples for full behavior.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/tests/integration_run_user_commands.rs` – New integration tests per SPEC §15.
- `crates/deacon/tests/smoke_basic.rs` – Add JSON stdout verification.
- `examples/run-user-commands/` – Add example projects (dotfiles, secrets, waitFor) with README.
- `EXAMPLES.md` – Add index entries.
- `README.md` – Add run-user-commands usage section.

#### Specific Tasks
- [ ] Implement each test case enumerated in SPEC §15.
- [ ] Add necessary fixtures under `fixtures/`.

### 2. Data Structures
- Use output result JSON for assertions.

### 3. Validation Rules
- [ ] Assert exact error messages where specified (Theme 6).

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 5 - Marker Pattern (verified)
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Already covered in prior tasks.

### Integration Tests
- [ ] All tests from SPEC §15.

### Smoke Tests
- [ ] Update smoke tests for JSON output.

### Examples
- [ ] Examples and documentation updated.

## Acceptance Criteria
- [ ] All tests green; docs updated; CI passes.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§15)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§9)
