---
subcommand: run-user-commands
type: enhancement
priority: high
scope: small
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: output"]
---

# [run-user-commands] Implement JSON stdout result and standardized error mapping

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement the one-line JSON stdout contract for success and error results, ensuring all logs go to stderr and exit codes follow the spec. Centralize mapping of early exit states (`skipNonBlocking`, `prebuild`, `stopForPersonalization`, `done`).

## Specification Reference

**From SPEC.md Section:** §10 Output Specifications, §9 Error Handling Strategy

**From GAP.md Section:** 4. Output Contract Gaps, 6. Error Handling Gaps

### Expected Behavior
- Success: `{ "outcome": "success", "result": "..." }`
- Error: `{ "outcome": "error", "message": string, "description": string }`
- Logs on stderr only; no trailing newline in JSON.

### Current Behavior
- No structured output; mixed logs.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/run_user_commands/output.rs` – New helpers to serialize and print results.
- `crates/deacon/src/commands/run_user_commands.rs` – Integrate helpers and ensure exit code handling.

#### Specific Tasks
- [ ] Add `ExecutionResult` enum/structs as in DATA-STRUCTURES.md with serde tags.
- [ ] Add `print_result_stdout_one_line()` and use throughout.

### 2. Data Structures
```rust
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum ExecutionResult { /* success|error */ }
```

### 3. Validation Rules
- [ ] Ensure stdout/stderr separation strictly enforced.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Serialization shapes for success and error.

### Integration Tests
- [ ] Verify only stdout has JSON while stderr contains logs.

### Smoke Tests
- [ ] Update `smoke_basic.rs` to include a trivial invocation and validate JSON line.

## Acceptance Criteria
- [ ] JSON output contract implemented; CI green.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§9, §10)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§4, §6)
