---
subcommand: upgrade
type: enhancement
priority: low
scope: small
---

# [upgrade] State and Idempotency Semantics

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Implement and verify idempotent behavior for `upgrade`: repeated runs without changes should produce identical lockfiles and no extra output; pinning runs are no-ops after the first edit. Ensure the pre-truncate + write strategy aligns with spec.

## Specification Reference

**From SPEC.md Section:** §6 State Management — Idempotency

**From GAP.md Section:** 6 State Management — Idempotency

### Expected Behavior
- Re-running `upgrade` without changes yields the same lockfile; no additional content changes
- With `--feature/--target-version`, subsequent runs detect no further edits

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Confirm behavior around force_init and pre-truncate matches spec

#### Specific Tasks
- [ ] Add comments and small guards to avoid unnecessary writes/logs

### 2. Data Structures
- N/A

### 3. Validation Rules
- [ ] N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - Deterministic output

## Testing Requirements

### Unit/Integration
- [ ] Two consecutive runs yield identical file
- [ ] Pinning then re-run: no additional edit

### Smoke Tests
- [ ] None

### Examples
- [ ] Mention in examples README

## Acceptance Criteria
- [ ] Idempotency verified by tests
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§6)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§6)
