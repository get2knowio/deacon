---
subcommand: run-user-commands
type: enhancement
priority: high
scope: large
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: orchestration"]
---

# [run-user-commands] Implement execute_subcommand orchestration

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement the end-to-end orchestration per SPEC §5: initialization, validation, discovery, merge, substitution, env probe, lifecycle (with markers, secrets, dotfiles), early exits, and final JSON output. Add tracing spans for major phases.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic, §10 Output Specifications

**From GAP.md Section:** Multiple (1–7, 11)

### Expected Behavior
- Follow the provided pseudocode flow; map results to `ExecutionResult` and print one-line JSON to stdout.

### Current Behavior
- Partial, non-compliant behavior.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/run_user_commands/mod.rs` – New module exporting `execute_run_user_commands`.
- `crates/deacon/src/commands/run_user_commands.rs` – Wire CLI to core; handle output/exit codes.

#### Specific Tasks
- [ ] Implement phases 1–4 with spans: `ruc.init`, `ruc.discover`, `ruc.merge`, `ruc.probe_env`, `ruc.lifecycle`, `ruc.output`.
- [ ] Integrate previous tasks’ modules (selection, merge, substitution, env, lifecycle, markers, dotfiles, secrets, output).

### 2. Data Structures
- Use `RunUserCommandsArgs`, `MergedDevContainerConfig`, `ContainerProperties`, `ExecutionResult`.

### 3. Validation Rules
- [ ] Ensure argument validation occurs before expensive operations.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation
- [x] Theme 5 - Marker Pattern
- [x] Theme 6 - Error Messages
- [x] Item 5 - Env Probe Cache
- [x] Item 6 - Dotfiles
- [x] Item 7 - Secrets Redaction
- [x] Item 8 - Two-Phase Substitution

## Testing Requirements

### Unit Tests
- [ ] Mocked flow returns expected results for each early-exit and success.

### Integration Tests
- [ ] Implement the SPEC §15 test suite for run-user-commands.

### Smoke Tests
- [ ] Add minimal JSON output verification to `smoke_basic.rs`.

## Acceptance Criteria
- [ ] Orchestration done; JSON output correct; CI green.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§5, §10, §15)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md`
