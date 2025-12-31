---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Testing, Examples, and Smoke Coverage

## Issue Type
- [ ] Missing CLI Flags
- [x] Testing & Validation
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern

## Description
Add comprehensive unit, integration, and smoke tests covering new flags, environment probe and merge behavior, metadata merge, PTY selection, terminal dimensions, and exit code mapping. Update examples and fixtures accordingly.

## Specification Reference
- From SPEC.md Section: §15 Testing Strategy, §14 Edge Cases
- From GAP.md Section: 11. Testing Gaps
- From PARITY_APPROACH.md: Quality Gates and Examples Maintenance

### Expected Behavior
- Test matrix covers: container-id targeting, empty remote env values, probe modes, image metadata merge, substitution, env merge order, signal exit code mapping, terminal dims, docker-path override.

### Current Behavior
- Partial tests exist; many scenarios missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/tests/integration_exec.rs` — Add missing cases per spec §15.
- `crates/deacon/tests/smoke_basic.rs` — Update for at least one new exec flow.
- `examples/cli/` — Add or update example demonstrating new flags and behaviors.
- `fixtures/` — Add configs to exercise remoteEnv, userEnvProbe, and label inference.

### 2. Data Structures
- N/A

### 3. Validation Rules
- Ensure exact error strings and exit codes asserted.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract (logs)
- [ ] Theme 2 - CLI Validation
- [ ] Theme 6 - Error Messages

## Testing Requirements
- As above; include happy-path and error-path tests. Ensure tests do not require Docker when not available by accepting well-defined errors (see `.github/copilot-instructions.md` Smoke Tests Maintenance).

## Acceptance Criteria
- [ ] All new tests added and passing locally and in CI.
- [ ] Examples and fixtures updated.
- [ ] GAP.md testing section updated.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§14–§15)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§11)
- Parity Approach: `docs/PARITY_APPROACH.md`
