---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Core: Container Selection and Config Discovery

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Implement container selection by `--container-id` or `--id-label` (and inference from `--workspace-folder` when provided). Add config discovery via `--config`/`--workspace-folder` and correct error message when not found.

## Specification Reference
- From SPEC.md Section: §4 Configuration Resolution, §5 Core Execution Logic
- From GAP.md Section: 3. Configuration Resolution Gaps, 4. Core Execution Logic Gaps

### Expected Behavior
- If `--container-id` provided, target it directly.
- Else resolve by labels; if `--workspace-folder` provided, infer labels `devcontainer.local_folder` and `devcontainer.config_file`.
- If config flags provided but config missing → error: "Dev container config (<path>) not found."
- If no container found → error: "Dev container not found."

### Current Behavior
- No support for explicit `--container-id`.
- Partial config discovery; missing exact error handling.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/exec.rs` — Wire selection logic; use docker client/list/inspect.
- `crates/core/src/docker/inspect.rs` (or similar) — Utilities to resolve container by ID or labels.

### 2. Data Structures
- Use `ParsedInput` fields introduced by tasks 01–04.

### 3. Validation Rules
- [ ] If any of `--workspace-folder`, `--config`, `--override-config` specified but no config found/readable → exact error message.

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages
- [ ] Theme 2 - Validate early before docker exec

## Testing Requirements

### Unit Tests
- [ ] Container label selector builds correct query for multiple `--id-label` entries.

### Integration Tests
- [ ] `--container-id` targets container.
- [ ] Missing config error message matches exactly.
- [ ] Missing container → error "Dev container not found."

## Acceptance Criteria
- [ ] Selection works for ID and labels; config discovery aligned to spec; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§4–§5)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§3–§4)
