---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: docker"]
---

# [run-user-commands] Implement container selection by id, labels, or workspace

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement spec-compliant container selection: direct by `--container-id`, by repeatable `--id-label` filters (match all), or inferred labels from `--workspace-folder`. Return the exact error message when not found.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Discovery and merge)

**From GAP.md Section:** 2.1 Container Selection (missing id/labels, partial workspace)

### Expected Behavior
- If `--container-id` is provided, inspect directly.
- Else if `--id-label` provided, list/inspect containers and select one matching all labels.
- Else derive id labels from `--workspace-folder` and config path; find container.
- If not found → error: `Dev container not found.`

### Current Behavior
- Only workspace-based lookup partially exists with a different error message.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/docker.rs` – Add helpers for label lookup and selection logic.
- `crates/core/src/setup/container_select.rs` or `crates/core/src/run_user_commands/container_select.rs` – New module with selection policy.
- `crates/deacon/src/commands/run_user_commands.rs` – Wire selection results and error mapping.

#### Specific Tasks
- [ ] Implement `select_container(args) -> Result<ContainerRef>` with the precedence above.
- [ ] Ensure exact error string: `Dev container not found.`
- [ ] Unit tests covering id, labels, workspace fallback, and not-found.

### 2. Data Structures
- Reuse `RunUserCommandsArgs` and simple `ContainerRef` (id string) as return.

### 3. Validation Rules
- [ ] None beyond inputs; rely on CLI validation from task 01.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages (exact string)

## Testing Requirements

### Unit Tests
- [ ] Selection by id.
- [ ] Selection by multiple labels (all must match).
- [ ] Workspace-derived labels path.
- [ ] Not-found case returns exact error.

### Integration Tests
- [ ] With Docker available, create a labeled container and verify discovery.

### Smoke Tests
- [ ] None for this task alone.

## Acceptance Criteria
- [ ] Deterministic selection implemented; exact error message.
- [ ] CI passes.

## Implementation Notes
- For label-based selection, consider using `docker ps --filter label=...` to narrow candidates before inspect.

## Definition of Done
- [ ] Container selection working with tests.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§2.1)
