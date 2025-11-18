---
subcommand: exec
type: enhancement
priority: medium
scope: medium
---

# [exec] Core: Exec PTY Selection, Terminal Dimensions, and CWD Resolution

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Implement PTY selection heuristic using stdin/stdout TTY detection and `--log-format json` override. Apply terminal dimensions when provided, propagate resize events, and resolve remote CWD using `remoteWorkspaceFolder` or `homeFolder` fallback. Respect `remoteUser` when set by merged config and allow extensions like `--user` / `--workdir` to override.

## Specification Reference
- From SPEC.md Section: §5 Core Execution Logic, §7 External System Interactions (Docker/exec), §10 Output Specifications, §14 Edge Cases
- From GAP.md Section: 4. Core Execution Logic Gaps, 7. PTY/TTY Handling Gaps, 10. Working Directory Resolution Gaps

### Expected Behavior
- PTY if stdin and stdout are TTYs, or when `--log-format json`.
- Apply `--terminal-columns/--terminal-rows` when provided; pair validated in task 04.
- CWD: `remoteWorkspaceFolder` else `homeFolder`.

### Current Behavior
- Simple TTY detection; no JSON override; root `/` fallback for id-label case; no resize handling.

## Implementation Requirements

### 1. Code Changes Required
- `crates/core/src/docker/exec.rs` — PTY and non-PTY exec paths; apply `-t`, `-i`, `-e`, `-w`, `-u` flags; handle resize.
- `crates/deacon/src/commands/exec.rs` — Decide PTY and terminal dims; compute final CWD; honor merged `remoteUser`.

### 2. Data Structures
```rust
pub struct ContainerProperties {
    pub remoteWorkspaceFolder: Option<String>,
    pub homeFolder: String,
    pub user: String,
}
```

### 3. Validation Rules
- [ ] Use dims only when both provided.

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] PTY selection logic for TTY vs non-TTY and JSON format.
- [ ] CWD resolution logic.

### Integration Tests
- [ ] Exec with PTY streams merged; without PTY keeps separate streams.
- [ ] Terminal dimensions applied (can assert docker argv contains `-e COLUMNS=... -e LINES=...` or equivalent strategy if used).

## Acceptance Criteria
- [ ] PTY selection and CWD resolution match spec; tests pass; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§5, §7, §10, §14)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§4, §7, §10)
