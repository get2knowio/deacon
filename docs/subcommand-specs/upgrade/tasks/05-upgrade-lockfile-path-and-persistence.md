---
subcommand: upgrade
type: enhancement
priority: high
scope: medium
---

# [upgrade] Lockfile Path Derivation and Persistence

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Implement lockfile path derivation and writing using `deacon_core::lockfile`. Follow the naming rule based on config basename, perform atomic writes, and honor `force_init = true` semantics after a pre-truncate step to ensure initialization.

## Specification Reference

**From SPEC.md Section:** §6. State Management, §10. Output Specifications

**From GAP.md Section:** 2.5 Lockfile Generation, 2.6 Lockfile Path Derivation, 4.2 Filesystem Operations

### Expected Behavior
- `get_lockfile_path(config_path)` derives `.devcontainer-lock.json` or `devcontainer-lock.json` in the config directory.
- When not `--dry-run`, truncate/create then call `write_lockfile(path, lockfile, force_init = true)`.
- On success, no stdout content; logs confirmation.

### Current Behavior
- Core lockfile I/O exists; upgrade integration missing.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - After generating lockfile, branch by `dry_run` vs persist
  - Use `deacon_core::lockfile::{get_lockfile_path, write_lockfile}`
  - Pre-truncate file with empty write prior to `write_lockfile(..., true)` per spec diagram

#### Specific Tasks
- [ ] Implement path derivation
- [ ] Atomic write via core helper
- [ ] Log completion and path

### 2. Data Structures
- Reuse `Lockfile` from core

### 3. Validation Rules
- [ ] Exit code 0 on success, 1 on any error

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error message standardization

## Testing Requirements

### Unit Tests
- [ ] Path naming rule tests (dotfile vs non-dotfile)

### Integration Tests
- [ ] File persistence on filesystem with expected JSON formatting (2-space indent)

### Smoke Tests
- [ ] None

### Examples
- [ ] None

## Acceptance Criteria
- [ ] Non-dry-run persists lockfile at derived path
- [ ] No stdout payload when persisting; logs only to stderr
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§6, §10)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§2.5–2.6, §4.2)
