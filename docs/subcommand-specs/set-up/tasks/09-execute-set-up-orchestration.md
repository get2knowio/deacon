---
subcommand: set-up
type: enhancement
priority: high
scope: large
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: orchestration"]
---

# [set-up] Implement execute_set_up orchestration and stdout JSON result

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement the end-to-end `execute_set_up` function tying together argument normalization, container discovery, system patching, container-side substitution, lifecycle execution with markers, optional dotfiles, and final JSON output including configuration blobs based on flags.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic, §10 Output Specifications, §9 Error Handling Strategy

**From GAP.md Section:** 1.2 Core Execution Logic (100% Missing), 1.9 Error Handling, 1.10 Output Specifications

### Expected Behavior
- Follow SPEC §5 pseudocode phases 1-4.
- On success, print one-line JSON with optional `configuration` and/or `mergedConfiguration` depending on flags.
- On errors, print error JSON with `message` and `description` and exit with code 1.

### Current Behavior
- No orchestration exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/setup/mod.rs` – New module exporting `execute_set_up`.
- `crates/deacon/src/commands/set_up.rs` – Wire CLI to core and handle exit code/printing per Theme 1.
- `crates/deacon/src/main.rs` – Ensure result printing route matches other subcommands.

#### Specific Tasks
- [ ] Implement phases:
  - Initialization: build params from CLI, validate, normalize.
  - Discovery: inspect container, read optional config, extract image metadata, merge.
  - Main execution: patch `/etc/*`, probe env and substitute, run lifecycle and dotfiles per flags.
  - Post: construct `SetUpResult::Success` with optional fields and serialize to stdout.
- [ ] Map errors to `SetUpResult::Error` with exact messages.
- [ ] Add tracing spans: `container.inspect`, `setup.patch`, `setup.substitute`, `lifecycle.run`, `dotfiles.install`.

### 2. Data Structures
- Reuse types from tasks 02-08.

### 3. Validation Rules
- [ ] Enforce CLI validation before docker calls.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation (ensured upstream)
- [x] Theme 5 - Marker Idempotency
- [x] Theme 6 - Error Message Standardization
- [x] Theme 8 - Two-Phase Substitution

## Testing Requirements

### Unit Tests
- [ ] Mocked execution covering success with both config outputs.
- [ ] Error path for container not found.

### Integration Tests
- [ ] Implement all 7 tests from SPEC §15 under `crates/deacon/tests/integration_set_up.rs`.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include a minimal `set-up` invocation assertion.

### Examples
- [ ] Add `examples/set-up/basic/` with a simple project and README.

## Acceptance Criteria
- [ ] End-to-end works per spec; prints exact JSON shapes.
- [ ] All new tests pass; CI green with clippy -D warnings.

## Implementation Notes
- Ensure logs go to stderr and not mixed with JSON stdout.

### Edge Cases to Handle
- `--skip-post-create` path skips hooks and dotfiles but still performs system patching and substitution.

## Definition of Done
- [ ] Orchestration implemented with tests and examples.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§5, §9, §10, §15)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.2, §1.9, §1.10)
