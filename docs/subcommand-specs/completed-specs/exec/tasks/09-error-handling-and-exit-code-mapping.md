---
subcommand: exec
type: enhancement
priority: medium
scope: small
---

# [exec] Error Handling: Signal Exit Code Mapping and Docker Errors

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Implement exit code mapping to follow POSIX convention: numeric code if present; else `128 + signal` if terminated by signal; else `1`. Improve error messages for docker CLI unavailability and container-not-found cases, ensuring exact strings per spec.

## Specification Reference
- From SPEC.md Section: §9 Error Handling Strategy (Exit Code Mapping)
- From GAP.md Section: 6. Exit Code Handling Gaps

### Expected Behavior
- Map `signal` to `128 + signal` where signal is numeric or can be mapped from name.
- Default to `1` when neither code nor signal available.

### Current Behavior
- Pass-through numeric exit code only.

## Implementation Requirements

### 1. Code Changes Required
- `crates/core/src/process/exit_mapping.rs` — New utility to compute mapped exit.
- `crates/deacon/src/commands/exec.rs` — Use mapping before process exit.

### 2. Data Structures
```rust
pub struct ExecutionResult { pub code: Option<i32>, pub signal: Option<i32> }
```

### 3. Validation Rules
- [ ] None; computation only.

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Map code=123 → 123; signal=9 → 137; neither → 1.

### Integration Tests
- [ ] Simulate docker exec terminated by signal and assert mapped exit.

## Acceptance Criteria
- [ ] Exit mapping implemented and covered by tests; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§9)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§6)
