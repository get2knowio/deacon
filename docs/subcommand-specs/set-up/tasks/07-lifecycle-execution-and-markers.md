---
subcommand: set-up
type: enhancement
priority: medium
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: medium", "area: lifecycle"]
---

# [set-up] Lifecycle execution with waitFor semantics and markers

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Wire lifecycle hooks (onCreate → updateContent → postCreate → postStart → postAttach) to execute inside the existing container using `ContainerProperties` exec functions. Honor `waitFor` and `--skip-non-blocking-commands`; integrate user data folder markers to prevent re-execution across runs.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (3c Lifecycle)

**From GAP.md Section:** 1.7 Lifecycle Hook Execution (Partial), 1.4 State Management (Markers)

### Expected Behavior
- When `--skip-post-create` is set, skip all lifecycle hooks and dotfiles (system patching still occurs per spec).
- Otherwise run hooks in order, respecting `waitFor` and `--skip-non-blocking-commands` to stop after the configured phase.
- Use user markers under `~/.devcontainer` (or `--container-data-folder`) to ensure idempotency per hook.
- Stop on first failure and return error with combined stdout/stderr.

### Current Behavior
- Core lifecycle helpers exist but not integrated with set-up properties/markers.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/container_lifecycle.rs` – Extend to accept `ContainerProperties` and marker controls.
- `crates/core/src/setup/lifecycle.rs` – New orchestration calling lifecycle functions in order with markers.

#### Specific Tasks
- [ ] Implement marker naming: `.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker`, `.postStartCommandMarker` in user data folder.
- [ ] Implement `waitFor` cut-off and `--skip-non-blocking-commands` logic.
- [ ] Error propagation with message and combined output.

### 2. Data Structures
- Use `CommonMergedDevContainerConfig` for command arrays and `LifecycleHooksInstallMap` for origins if needed for logs.

### 3. Validation Rules
- [ ] None beyond spec semantics.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 5 - Marker File Idempotency Pattern.
- [x] Theme 6 - Error Messages.

## Testing Requirements

### Unit Tests
- [ ] Marker prevents re-run behavior.
- [ ] `waitFor` boundary stops execution as expected.

### Integration Tests
- [ ] Run simple lifecycle that writes marker files and verify ordering and early stop.

### Smoke Tests
- [ ] Not required until command end-to-end is wired.

## Acceptance Criteria
- [ ] Lifecycle runs in correct order with markers and respects skip flags.
- [ ] Failures stop subsequent steps and return error.

## Implementation Notes
- Buffer output per step and attach to error on failure.

### Edge Cases to Handle
- No hooks present → no-ops; still success.

## Definition of Done
- [ ] Lifecycle orchestration integrated for set-up.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§5)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.7, §1.4)
