# [up] Implement updateContentCommand and prebuild mode

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Execute `updateContentCommand` in the lifecycle and implement `--prebuild` semantics: stop after onCreate+updateContent; rerun updateContent if previously run. Respect `--skip-post-attach`. Ensure proper lifecycle ordering and background task wait pattern.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (setupInContainer), §15. Testing Strategy (prebuild)
- From GAP.md Section: §15 Lifecycle Commands – Missing; Critical Missing Features (2, 3)

### Expected Behavior
- Lifecycle order: initialize (host) → onCreate → updateContent → postCreate → postStart → postAttach (unless skipped).
- Prebuild mode stops after updateContent; on subsequent runs reruns updateContent.
- Background tasks are awaited via `finishBackgroundTasks` before disposal when outcome success.

### Current Behavior
- updateContent not executed; prebuild not implemented; skip-post-attach missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - lifecycle runner enhancements; flags support and gating.
- Consider factoring shared lifecycle execution with `run_user_commands.rs` to a helper.

### 2. Data Structures
```rust
// LifecycleHooksInstallMap with updateContentCommand entries
```

### 3. Validation Rules
- [ ] `--prebuild` conflicts logically with `--skip-post-create` semantics; align per spec.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [ ] Infrastructure Item 5 - Environment Probing System (needed for some lifecycle env cases; coordinate).
- [ ] Infrastructure Item 7 - Secrets Management & Log Redaction (ensure redaction during lifecycle output).

## Testing Requirements
- Unit: lifecycle ordering decisions.
- Integration: prebuild run halts after updateContent; second run reruns updateContent; skip-post-attach respected.
- Error: lifecycle failure stops subsequent phases and yields error JSON.

## Acceptance Criteria
- Lifecycle phases implemented correctly; prebuild semantics validated.
- CI green with added tests.

## References
- `docs/subcommand-specs/up/SPEC.md` (§5, §15)
- `docs/subcommand-specs/up/GAP.md` (§15)
