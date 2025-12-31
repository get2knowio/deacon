---
subcommand: run-user-commands
type: enhancement
priority: high
scope: large
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: lifecycle"]
---

# [run-user-commands] Lifecycle execution: waitFor semantics, skip-non-blocking, and parallel object syntax

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Enhance lifecycle orchestration to support `waitFor` semantics and `--skip-non-blocking-commands` early exit, object-form parallel execution with per-step buffered output, and `--skip-post-attach`. Integrate markers and env/secrets from prior tasks.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (Main execution) and §11 Performance Considerations (parallelization)

**From GAP.md Section:** 3.1 Lifecycle Hook Execution, 3.6 WaitFor and Early Exit, 7. Performance and Caching Gaps

### Expected Behavior
- Execute hooks in order with markers; when configured `waitFor` phase is reached and `--skip-non-blocking-commands` is set, return `"skipNonBlocking"`.
- Support object syntax where values run concurrently and output is buffered per named step.
- Respect `--skip-post-attach`.

### Current Behavior
- Basic string/array execution exists; no parallel object syntax; waitFor not implemented; partial skip-post-attach.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/container_lifecycle.rs` – Add parallel object syntax execution and buffered logs; implement waitFor early exit.
- `crates/core/src/run_user_commands/lifecycle.rs` – Orchestrate phases, integrate markers, env, secrets, and early exit mapping to result.

#### Specific Tasks
- [ ] Implement object syntax parsing and parallel execution with per-step buffering.
- [ ] Implement waitFor boundary and early exit result mapping.
- [ ] Integrate `--skip-post-attach` behavior.

### 2. Data Structures
- Use `LifecycleHooksInstallMap` and `MergedDevContainerConfig`.

### 3. Validation Rules
- [ ] Validate object values are strings/arrays; error if invalid shapes.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 5 - Marker Idempotency
- [x] Theme 6 - Error Message Standardization

## Testing Requirements

### Unit Tests
- [ ] Parallel execution buffers output correctly and attributes failures.
- [ ] waitFor + skip-non-blocking returns `"skipNonBlocking"` at the right boundary.

### Integration Tests
- [ ] Hooks with object syntax run parallel; failure in one aborts others (with cleanup where possible).

## Acceptance Criteria
- [ ] Lifecycle orchestration supports parallel object syntax and waitFor semantics with early exit.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§5, §11)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§3.1, §3.6, §7)
