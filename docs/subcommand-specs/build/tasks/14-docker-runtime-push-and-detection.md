---
subcommand: build
type: enhancement
priority: medium
scope: small
---

# [build] Docker runtime: push behavior and buildx detection

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Ensure that `--push` results in pushing via `docker buildx build --push` and that buildx availability is checked with user-friendly errors if missing. This complements the BuildKit output control issue by focusing on environment checks and push semantics.

## Specification Reference

**From SPEC.md Section:** §7 External System Interactions (Docker)

**From GAP.md Section:** 5.1 Push to Registry

### Expected Behavior
- If `--push` is set and BuildKit is enabled, invocation uses `buildx build --push`.
- If buildx is not available or BuildKit disabled, emit a clear spec-aligned error.

### Current Behavior
- `--push` not implemented; no buildx sanity checks.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – add buildx presence check (e.g., `docker buildx version`) and map failures to input errors when `--push` is requested.

#### Specific Tasks
- [ ] Implement buildx detection and friendly error messaging.

### 2. Data Structures
N/A.

### 3. Validation Rules
- Coordinate with validation task to ensure early errors where possible; this task adds runtime checks.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Mock buildx check and assert behavior.

### Integration Tests
- [ ] Simulate environment without buildx and confirm error.

## Acceptance Criteria
- [ ] Push behavior wired and guarded.
- [ ] CI checks pass.

## Definition of Done
- [ ] Buildx detection and push semantics implemented.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§7)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§5.1)
