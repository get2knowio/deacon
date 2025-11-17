---
subcommand: build
type: enhancement
priority: medium
scope: medium
---

# [build] Add tests, fixtures, and examples per spec

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Implement the required unit, integration, and smoke tests for the build subcommand, and update/add examples and fixtures to demonstrate new behaviors such as labels, mutual exclusions, BuildKit cache/platform, Compose restrictions, and nested config paths.

## Specification Reference

**From SPEC.md Section:** §15 Testing Strategy

**From GAP.md Section:** 7.1 Required Test Cases from Spec

### Expected Behavior
- Tests cover label propagation, mutual exclusion errors, BuildKit cache/platform flow, Compose restrictions, and config-in-subfolder case.

### Current Behavior
- Partial coverage; many tests missing.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/tests/` – add new integration tests per listed cases.
- `crates/deacon/tests/smoke_basic.rs` – update for JSON output shape and add assertions for flags where relevant.
- `examples/build/*` – update READMEs and add small configs demonstrating new flags.
- `fixtures/config/*` – add fixtures for compose and nested config scenarios if missing.

#### Specific Tasks
- [ ] Test "labels applied" with two labels.
- [ ] Test "mutually exclusive push/output" error.
- [ ] Test "buildkit cache and platform" scenario.
- [ ] Test "compose build not supporting platform/push/output/cache-to".
- [ ] Test "config in subfolder" success path.

### 2. Data Structures
N/A.

### 3. Validation Rules
- Ensure exact error messages match spec.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation
- [x] Theme 6 - Error Messages

## Testing Requirements
As above; ensure deterministic behavior and Docker-unavailable tolerance.

## Acceptance Criteria
- [ ] All tests added and passing locally.
- [ ] Examples updated and documented.
- [ ] CI checks pass.

## Definition of Done
- [ ] Spec-mandated tests implemented; smoke covers new JSON output.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§15)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§7.1)
