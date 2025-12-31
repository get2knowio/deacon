---
subcommand: upgrade
type: enhancement
priority: medium
scope: small
---

# [upgrade] Error Messages and Exit Codes Compliance

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Ensure all user and system errors for `upgrade` match the specification’s exact messages and exit codes. Standardize error contexts using `anyhow::Context` and ensure exit code 1 on failures.

## Specification Reference

**From SPEC.md Section:** §9. Error Handling Strategy, §10. Output Specifications (Exit Codes)

**From GAP.md Section:** 5.3 Error Messages, 5.4 Exit Codes

### Expected Behavior
- Config not found: "Dev container config (...) not found."
- Malformed JSON: "... must contain a JSON object literal."
- Lockfile write errors: "Failed to update lockfile" (with underlying cause)
- CLI validation errors (from Task 01) preserved exactly
- Exit code 0 success; 1 on any error

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Map errors to exact messages
  - Wrap I/O/registry errors with helpful context
  - Ensure main entry returns error to be mapped to exit 1

#### Specific Tasks
- [ ] Add error mapping helpers
- [ ] Use `.with_context(...)` on filesystem and resolver calls
- [ ] Unit test error message strings

### 2. Data Structures
- N/A

### 3. Validation Rules
- [ ] Exact string matching for key error cases

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Message Standardization

## Testing Requirements

### Unit Tests
- [ ] Verify exact strings for key errors using fixtures/mocks

### Integration Tests
- [ ] Nonexistent config path produces expected message

### Smoke Tests
- [ ] None

### Examples
- [ ] None

## Acceptance Criteria
- [ ] All error strings align with SPEC.md
- [ ] Exit codes correct
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§9–10)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§5.3–5.4)
