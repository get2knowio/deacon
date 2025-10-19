---
subcommand: set-up
type: enhancement
priority: medium
scope: small
labels: ["subcommand: set-up", "type: enhancement", "priority: medium", "area: errors"]
---

# [set-up] Standardize error handling and JSON output mapping

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement a standardized mapping for user and system errors into the `{ outcome: "error", message, description }` shape, and ensure success results strictly follow the output spec with optional fields only when requested. Centralize common error messages to avoid drift.

## Specification Reference

**From SPEC.md Section:** §9 Error Handling Strategy, §10 Output Specifications

**From GAP.md Section:** 1.9 Error Handling (Minimal), 1.10 Output Specifications (Missing)

### Expected Behavior
- User errors: missing `--container-id`, invalid `--remote-env`, missing config path.
- System errors: container not found, lifecycle failure, patch failures (warn and continue).
- Output: exactly one JSON line to stdout; logs to stderr; exit code 0/1 accordingly.

### Current Behavior
- Not implemented for set-up.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/setup/errors.rs` – New error enum/types for set-up domain with `thiserror`.
- `crates/core/src/setup/output.rs` – Helpers to serialize `SetUpResult` and print to stdout with no trailing newline.
- `crates/deacon/src/commands/set_up.rs` – Use helpers and map clap/IO errors to domain errors.

#### Specific Tasks
- [ ] Add error variants with exact messages required by spec.
- [ ] Add `print_setup_result()` ensuring stdout/stderr separation.
- [ ] Unit tests for message strings and JSON shape.

### 2. Data Structures
- Use `SetUpResult` from task 02.

### 3. Validation Rules
- [ ] Ensure exact messages: `Dev container not found.`, `Dev container config (<path>) not found.`, `Invalid --remote-env entry: expected NAME=VALUE.`

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Error serialization tests for all variants.
- [ ] Success serialization with optional fields omitted when not requested.

### Integration Tests
- [ ] Invoke command with invalid inputs to assert messages and exit code.

### Smoke Tests
- [ ] Ensure stderr vs stdout separation by capturing both streams.

## Acceptance Criteria
- [ ] Errors and outputs match spec exactly.
- [ ] CI green.

## Implementation Notes
- Use `anyhow::Context` at boundaries but convert to domain errors for output.

## Definition of Done
- [ ] Error and output helpers implemented with tests.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§9, §10)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.9, §1.10)
