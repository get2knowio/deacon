---
subcommand: build
type: enhancement
priority: high
scope: small
---

# [build] Align error output, messages, and exit codes to spec

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Standardize error handling to emit the spec-compliant error JSON on stdout with exit code 1 and exact message texts where specified. Consolidate dispersed error paths in the build command to use a single helper for producing error outputs.

## Specification Reference

**From SPEC.md Section:** §9 Error Handling Strategy; §10 Output Specifications

**From GAP.md Section:** 4.2 Error Message Format

### Expected Behavior
- All user/system/config errors surface as `{ outcome: "error", message, description? }` with exit code 1.
- Messages match the spec exactly where prescribed (e.g., config filename, push/output mutual exclusion, compose restrictions).

### Current Behavior
- Inconsistent error messages and shapes.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – add an error-to-json helper to map `anyhow`/domain errors to `BuildErrorResult` and set exit code. Refactor call sites to use it.

#### Specific Tasks
- [ ] Centralize error mapping and writing to stdout.
- [ ] Ensure stderr retains detailed logs.

### 2. Data Structures
Use `BuildErrorResult` from DATA-STRUCTURES.md.

### 3. Validation Rules
- N/A.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Error mapping unit tests for each prescribed message.

### Integration Tests
- [ ] Confirm exit code 1 and correct stdout JSON for common error scenarios.

## Acceptance Criteria
- [ ] All errors are shaped correctly and exit codes standardized.
- [ ] CI checks pass.

## Definition of Done
- [ ] Error paths refactored; messages match spec.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§9, §10)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§4.2)
