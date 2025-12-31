---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: variables"]
---

# [run-user-commands] Implement two-phase variable substitution

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add two-phase substitution: pre-container (host) for `${devcontainerId}`, `${env:*}`, `${localWorkspaceFolder*}` and in-container for `${containerEnv:*}`, `${containerWorkspaceFolder*}` using probed container env. Validate missing variable names produce errors per spec.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Variable Substitution Rules)

**From GAP.md Section:** 2.2 Variable Substitution (partially implemented)

### Expected Behavior
- Before-container substitution applies to config/override using host context and derived devcontainerId.
- After container selection and env probe, apply containerEnv and container workspace substitutions.
- `${env:NAME:default}` defaulting supported; `${env:}` is an error referencing the source file.

### Current Behavior
- Single-pass substitution only; no containerEnv or devcontainerId support.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/variable.rs` – Extend parser/engine for devcontainerId, defaults, basename forms, and containerEnv pass.
- `crates/core/src/run_user_commands/substitution.rs` – New orchestration for two-phase substitution.

#### Specific Tasks
- [ ] Implement `${devcontainerId}` derivation from labels/selection.
- [ ] Implement `${containerEnv:VAR}` and `${containerEnv:}` error.
- [ ] Implement `${containerWorkspaceFolder}` and `${containerWorkspaceFolderBasename}`.
- [ ] Implement `${localWorkspaceFolderBasename}` and default value `${env:NAME:default}` handling.

### 2. Data Structures
- Reuse existing substitution engine structures.

### 3. Validation Rules
- [ ] Error on missing variable name (e.g., `${env:}` and `${containerEnv:}`) with message referencing config source.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 8 - Two-Phase Variable Substitution
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Defaults, basename, and devcontainerId derivation.
- [ ] ContainerEnv substitution after probe.

### Integration Tests
- [ ] End-to-end test reading from container env and asserting substituted config.

## Acceptance Criteria
- [ ] Two-phase substitution fully implemented and tested.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§2.2)
