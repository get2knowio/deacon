---
subcommand: upgrade
type: enhancement
priority: low
scope: small
---

# [upgrade] Edge Cases and Cross-Platform Path Handling

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: ___________

## Description
Cover edge cases from the spec and verify cross-platform path normalization behavior for lockfile derivation and config editing. This ensures consistent behavior on Linux/macOS/Windows/WSL2.

## Specification Reference

**From SPEC.md Section:** §13 Cross-Platform Behavior, §14 Edge Cases

**From GAP.md Section:** 8. Cross-Platform Considerations, 9. Edge Cases

### Expected Behavior
- Handles configs with no `features` gracefully (empty lockfile)
- Dry-run with pinning edits config but doesn't write lockfile
- Multiple matching feature keys → only first updated
- Renaming config changes lockfile path accordingly
- Permission errors handled with user-friendly messages

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- Tests only; code paths largely covered by prior tasks

#### Specific Tasks
- [ ] Add tests for all listed edge cases
- [ ] Verify Windows path rules using path normalization helpers (where feasible)

### 2. Data Structures
- N/A

### 3. Validation Rules
- [ ] Ensure error messages match exactly

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Base ID extraction and replacement edge cases

### Integration Tests
- [ ] Empty features config → empty lockfile
- [ ] Permission denied scenarios (best-effort in CI)

### Smoke Tests
- [ ] None

### Examples
- [ ] None

## Acceptance Criteria
- [ ] All edge cases covered by tests
- [ ] Cross-platform path logic validated where possible
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§13–14)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§8–9)
