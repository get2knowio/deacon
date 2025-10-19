---
subcommand: build
type: enhancement
priority: medium
scope: medium
---

# [build] State management: cache, persisted folder, and lockfiles

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Implement or align persisted folder usage for feature artifacts, generated Dockerfiles, optional lockfiles, and Compose override location. Ensure idempotency and stable paths to support caching and reproducibility.

## Specification Reference

**From SPEC.md Section:** §6 State Management

**From GAP.md Section:** 6.1 Feature Installation Workflow (artifacts), 6 State management mentions

### Expected Behavior
- Generated assets are written under a workspace-specific persisted folder (e.g., `.devcontainer/` subpaths) and reused across runs when valid.
- Optional lockfile is written/validated when experimental flags enabled.
- Compose override path includes timestamp/uuid per spec hint.

### Current Behavior
- Basic build cache exists; feature artifacts and lockfiles not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/` – utilities to compute persisted folder paths and write/read artifacts.
- `crates/deacon/src/commands/build.rs` – use these paths when generating files in features and compose tasks.

#### Specific Tasks
- [ ] Define stable folder layout for artifacts.
- [ ] Implement lockfile write/validate helpers.
- [ ] Document and enforce cleanup or reuse policy.

### 2. Data Structures
N/A.

### 3. Validation Rules
- Honor `--experimental-frozen-lockfile` by failing if planned changes differ.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Persisted path calculators and lockfile I/O tests.

### Integration Tests
- [ ] Validate idempotent behavior across two runs with same inputs.

## Acceptance Criteria
- [ ] Persisted folder structure defined and used.
- [ ] Lockfile behavior implemented.
- [ ] CI checks pass.

## Definition of Done
- [ ] Artifacts written/read under consistent paths; lockfiles handled.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§6)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md`
