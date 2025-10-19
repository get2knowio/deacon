---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: cli"]
---

# [run-user-commands] Implement CLI flags and validation

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add the complete flag surface for run-user-commands per SPEC §2 and enforce validation. This unblocks downstream work (container selection, substitution, markers, dotfiles, secrets) and ensures error messages match the spec.

## Specification Reference

**From SPEC.md Section:** §2 Command-Line Interface, §3 Input Processing Pipeline

**From GAP.md Section:** 1.1 Missing CLI Flags, 1.2 Argument Validation Issues

### Expected Behavior
- Support container selection (`--container-id`, `--id-label` repeatable) and `--workspace-folder` fallback.
- Add missing flags: docker paths, container data folders, mount-workspace-git-root, terminal dimensions, env/dotfiles/secrets flags, hidden skip-feature-auto-mapping.
- Validate: `--id-label` matches `/.+=.+/`; `--remote-env` matches `/.+=.*/`; at least one of container-id/id-label/workspace-folder provided; terminal dims paired.

### Current Behavior
- Many flags are missing or partial; label/env validations absent; container selection requirement not enforced.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` – Extend run-user-commands flags and validators.
- `crates/deacon/src/commands/run_user_commands.rs` – Map parsed flags to internal args struct; surface exact validation errors.

#### Specific Tasks
- [ ] Add flags: `--container-id`, `--id-label <NAME=VALUE>` (repeatable), `--docker-path`, `--docker-compose-path`, `--container-data-folder`, `--container-system-data-folder`, `--container-session-data-folder`, `--mount-workspace-git-root`, `--terminal-columns`, `--terminal-rows`, `--default-user-env-probe`, `--remote-env <NAME=VALUE>` (repeatable), `--secrets-file`, `--dotfiles-repository`, `--dotfiles-install-command`, `--dotfiles-target-path`, `--skip-feature-auto-mapping` (hidden).
- [ ] Enforce validations: label regex `/.+=.+/`, remote-env regex `/.+=.*/`, terminal dims pairing, container selection requirement.
- [ ] Errors and messages: use exact phrases from SPEC §9 where specified (e.g., config not found, container not found).

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
pub struct RunUserCommandsArgs { /* fields from DATA-STRUCTURES.md */ }
```

### 3. Validation Rules
- [ ] Validate format: `--id-label` `/.+=.+/`; `--remote-env` `/.+=.*/`.
- [ ] Required selection: at least one of `--container-id`, `--id-label`, `--workspace-folder`.
- [ ] Paired requirement: `--terminal-columns` requires `--terminal-rows` and vice versa.
- [ ] Error message: "Dev container config (<path>) not found." when discovery requested but missing.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 2 - CLI Validation
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Parsing of repeatable `--id-label` and `--remote-env`.
- [ ] Regex validation failures produce expected messages.
- [ ] Terminal pairing validation.
- [ ] Selection requirement error when none of the three flags provided.

### Integration Tests
- [ ] CLI help snapshot to ensure flags appear correctly.

### Smoke Tests
- [ ] Add `deacon run-user-commands --help` assertion in `smoke_basic.rs`.

### Examples
- [ ] Update `examples/README.md` with a short run-user-commands example.

## Acceptance Criteria
- [ ] Flags present and correctly parsed.
- [ ] Validations enforced with exact error messages.
- [ ] CI checks pass (build, tests, fmt, clippy).

## Implementation Notes
- Keep flag behavior aligned with up/set-up for consistency.

### Edge Cases to Handle
- Duplicate id-label names: last one wins in substitution context; matching requires all labels.

## Definition of Done
- [ ] CLI complete and validated, with tests.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§2, §3)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§1.1, §1.2)
- Data Structures: `docs/subcommand-specs/run-user-commands/DATA-STRUCTURES.md`
- Parity Approach: `docs/PARITY_APPROACH.md`
