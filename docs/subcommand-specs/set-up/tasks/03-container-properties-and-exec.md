---
subcommand: set-up
type: enhancement
priority: high
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: docker"]
---

# [set-up] Implement container properties discovery and exec abstractions

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Build the container properties discovery pipeline and exec abstractions required by set-up: inspect container, derive user/gid/env/shell/home, and expose `remoteExec`, `remotePtyExec`, and `remoteExecAsRoot`. Provide a simple shell server mechanism (or stub interface) to minimize repeated docker execs.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (Phase 2) and §7 External System Interactions

**From GAP.md Section:** 1.2 Core Execution Logic (100% Missing), 1.5 External System Interactions (Partial)

### Expected Behavior
- Inspect target container (`docker inspect`) and populate `ContainerProperties` fields required by set-up.
- Provide user and root exec functions compatible with lifecycle execution and patching steps.
- Handle PTY allocation when requested; choose non-PTY otherwise.
- If container not found, surface: `Dev container not found.`

### Current Behavior
- Basic docker exec utilities exist, but no `ContainerProperties` concept nor root exec wrapper tied to set-up.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/docker.rs` – Add helpers for container inspection → `ContainerProperties` mapping and exec wrappers.
- `crates/core/src/setup/container.rs` – New module implementing `create_container_properties()`.
- `crates/core/src/lib.rs` – Export new setup modules.

#### Specific Tasks
- [ ] Implement `create_container_properties(container_id, params) -> Result<ContainerProperties>`.
- [ ] Add `remote_exec`, `remote_pty_exec`, and `remote_exec_as_root` closures capturing container id and docker path.
- [ ] Implement error path: if inspect fails, return error with message `Dev container not found.` (Theme 6 exactness).
- [ ] Unit tests using fakes/mocks to validate mapping from inspect JSON to props.

### 2. Data Structures
- Uses `ContainerProperties` from task 02.

### 3. Validation Rules
- [ ] None beyond error messaging; ensure exact message per spec §9.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages (use exact string).
- [x] Theme 5 - Marker File Idempotency Pattern (exec wrappers will be used by marker operations later).

## Testing Requirements

### Unit Tests
- [ ] Inspect mapping test: mock inspect output with user, env, home, shell.
- [ ] Error path test: missing container → exact error message.

### Integration Tests
- [ ] Optional smoke: if Docker available, simple exec that echoes env.

### Smoke Tests
- [ ] Not required for this issue alone.

### Examples
- [ ] Add rustdoc on `create_container_properties` with example.

## Acceptance Criteria
- [ ] `create_container_properties` returns complete `ContainerProperties` ready for set-up phases.
- [ ] Exec wrappers honor PTY selection.
- [ ] CI checks pass.

## Implementation Notes
- Reuse existing docker client abstractions in core; avoid duplicating low-level exec logic.

### Edge Cases to Handle
- Containers with missing HOME or PATH; still populate sensible defaults (spec §14 guidance).

## Definition of Done
- [ ] Properties discovery works and is tested.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§5, §7, §14)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.2, §1.5)
- Data Structures: `docs/subcommand-specs/set-up/DATA-STRUCTURES.md`
