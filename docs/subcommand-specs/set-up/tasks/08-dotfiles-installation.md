---
subcommand: set-up
type: enhancement
priority: medium
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: medium", "area: dotfiles"]
---

# [set-up] Implement dotfiles installation inside container

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Install dotfiles inside the target container when dotfiles flags are provided. Support explicit `--dotfiles-install-command` or conventional scripts (install.sh, setup, bootstrap, script/*). Ensure idempotency via a marker at the target path.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (3c Dotfiles)

**From GAP.md Section:** 1.8 Dotfiles Integration (Partial)

### Expected Behavior
- Clone repository to `--dotfiles-target-path` (default `~/dotfiles`).
- Run install command (explicit flag takes precedence), else auto-detect scripts.
- Create a marker to avoid reinstall on subsequent runs.
- Respect `--skip-post-create` by skipping dotfiles when set.

### Current Behavior
- Dotfiles module exists but is host-oriented and not wired to set-up or markers.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/dotfiles.rs` – Extend to support in-container execution via `ContainerProperties` exec.
- `crates/core/src/setup/dotfiles.rs` – New glue for set-up path.

#### Specific Tasks
- [ ] Implement clone via `git` inside container.
- [ ] Implement installer detection order: install.sh, setup, bootstrap, script/*.
- [ ] Implement `--dotfiles-install-command` override.
- [ ] Implement marker file creation in target path.

### 2. Data Structures
- Reuse `DotfilesConfiguration` and `ContainerProperties`.

### 3. Validation Rules
- [ ] If repo URL invalid or clone fails, return error; do not silently continue.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 5 - Marker File Idempotency Pattern
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Installer detection matrix.
- [ ] Marker prevents reinstall.

### Integration Tests
- [ ] Simple public repo with install.sh; verify script executed once.

### Smoke Tests
- [ ] Not required until wired.

## Acceptance Criteria
- [ ] Dotfiles install runs once and honors flags.
- [ ] Errors are surfaced with actionable messages.

## Implementation Notes
- Use POSIX sh; avoid bashisms.

### Edge Cases to Handle
- Target path exists but empty; still clone or pull.

## Definition of Done
- [ ] Dotfiles workflow complete for set-up.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§5)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.8)
