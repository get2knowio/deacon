---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: docker"]
---

# [run-user-commands] Build ContainerProperties and exec/PTy wrappers

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Create a complete `ContainerProperties` for the selected container, including timestamps, user/gid, env, detected shell, home and data folders, plus reusable exec and PTY exec functions (and optional shell server) used by lifecycle and dotfiles.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Create ContainerProperties)

**From GAP.md Section:** 2.4 ContainerProperties Creation (largely missing)

### Expected Behavior
- Inspect container and fill `createdAt`, `startedAt`, `osRelease`, `user`, `gid`, `env`, `shell`, `homeFolder`, `userDataFolder`, `installFolder`, `remoteWorkspaceFolder`.
- Provide `remoteExec`, `remotePtyExec`, and optionally `shellServer`.

### Current Behavior
- Partial exec helpers exist; most fields missing.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/docker.rs` – Extend inspect parsing utilities.
- `crates/core/src/run_user_commands/container.rs` – New construction function for `ContainerProperties`.

#### Specific Tasks
- [ ] Implement `create_container_properties(...)` using docker inspect.
- [ ] Implement PTY allocation respecting terminal dimensions flags.
- [ ] Unit tests against mocked inspect JSON.

### 2. Data Structures
```rust
pub struct ContainerProperties { /* from DATA-STRUCTURES.md */ }
```

### 3. Validation Rules
- [ ] None beyond exact error strings when container missing (handled in selection).

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Populate fields from inspect.
- [ ] PTY sizing honored when provided.

### Integration Tests
- [ ] Simple docker-run container to verify exec and PTY behavior.

## Acceptance Criteria
- [ ] ContainerProperties complete and reusable by other tasks.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§2.4)
