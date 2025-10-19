---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: markers"]
---

# [run-user-commands] Implement marker idempotency and prebuild behavior

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add user data folder marker files for lifecycle hooks, using timestamps to decide whether to skip or run a hook. Honor `--prebuild` by re-running `updateContentCommand` even if its marker exists and return the `"prebuild"` result.

## Specification Reference

**From SPEC.md Section:** §6 State Management (Marker Files), §5 Core Execution Logic (prebuild)

**From GAP.md Section:** 3.2 Idempotency and Markers; 3.6 WaitFor and Early Exit (prebuild)

### Expected Behavior
- Create/check markers: `.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker`, `.postStartCommandMarker` under `${HOME}/.devcontainer` or `--container-data-folder`.
- Marker content uses container timestamps; atomic creation; permission failures skip hook gracefully.
- `--prebuild` forces updateContent rerun and returns `"prebuild"`.

### Current Behavior
- No markers; prebuild flag exists but not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/run_user_commands/markers.rs` – New marker utilities.
- `crates/core/src/container_lifecycle.rs` – Integrate marker checks into hook execution.

#### Specific Tasks
- [ ] Implement marker read/write helpers with atomic behavior.
- [ ] Integrate markers into lifecycle run order; skip when up-to-date.
- [ ] Implement prebuild rerun of updateContent.

### 2. Data Structures
- Marker file paths computed from `ContainerProperties.userDataFolder` and hook names.

### 3. Validation Rules
- [ ] None; errors convert to warnings and skip (per spec §14 edge cases).

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 5 - Marker File Idempotency Pattern

## Testing Requirements

### Unit Tests
- [ ] Markers written after hook runs; subsequent runs skip.
- [ ] Prebuild forces updateContent rerun and returns result.

### Integration Tests
- [ ] Validate behavior across repeated invocations in same container.

## Acceptance Criteria
- [ ] Marker pattern implemented; prebuild logic returns correct result.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§5, §6)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§3.2, §3.6)
