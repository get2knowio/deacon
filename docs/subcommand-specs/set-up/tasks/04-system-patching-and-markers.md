---
subcommand: set-up
type: enhancement
priority: high
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: linux", "area: markers"]
---

# [set-up] Patch /etc files with idempotent markers

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement the root-only, once-per-container patching of `/etc/environment` and `/etc/profile` guarded by system marker files under `/var/devcontainer`. This ensures shells launched after set-up inherit the expected PATH and environment entries without repeated modifications.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (3a System patching), §6 State Management

**From GAP.md Section:** 1.2 Core Execution Logic (System Patching), 1.4 State Management (Markers)

### Expected Behavior
- As root, append env pairs to `/etc/environment` (idempotent) and patch `/etc/profile` to preserve PATH.
- Create system markers: `.patchEtcEnvironmentMarker`, `.patchEtcProfileMarker` under `--container-system-data-folder` (default `/var/devcontainer`).
- If root operations fail, log warning and continue; do not abort set-up unless critical.

### Current Behavior
- No patching; no marker framework.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/setup/system_patching.rs` – New module implementing patchers.
- `crates/core/src/setup/markers.rs` – New module: marker read/write helpers (shared by lifecycle later).
- `crates/core/src/lib.rs` – Export modules.

#### Specific Tasks
- [ ] Implement `patch_etc_environment(params, props)` using `remoteExecAsRoot` and marker check.
- [ ] Implement `patch_etc_profile(params, props)` similarly.
- [ ] Implement helpers: `marker_exists(path)`, `write_marker(path)` using root exec.
- [ ] Log warnings on failure and continue (Theme 6 error messages for user-facing errors elsewhere).

### 2. Data Structures
- Uses `ContainerProperties` and `ResolverParameters` folder paths from previous tasks.

### 3. Validation Rules
- [ ] Ensure paths respect overrides: `--container-system-data-folder`.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 5 - Marker File Idempotency Pattern
- [x] Theme 6 - Error Messages (warnings wording standardized).

## Testing Requirements

### Unit Tests
- [ ] Marker exists short-circuits patching.
- [ ] After patching, marker is written.
- [ ] Failure to write marker logs warning but returns Ok.

### Integration Tests
- [ ] If Docker available, run against an Alpine/Ubuntu container to verify idempotent behavior.

### Smoke Tests
- [ ] Not required until wired into `execute_set_up`.

### Examples
- [ ] Add inline rustdoc examples of marker naming.

## Acceptance Criteria
- [ ] Patching runs once per container and respects folder overrides.
- [ ] Markers prevent re-execution.
- [ ] CI passes.

## Implementation Notes
- Use heredoc-safe shells; avoid assuming bash availability.

### Edge Cases to Handle
- Read-only `/etc` leads to warning and skip; continue set-up.

## Definition of Done
- [ ] Patchers and marker helpers added and tested.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§5, §6)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.2, §1.4)
