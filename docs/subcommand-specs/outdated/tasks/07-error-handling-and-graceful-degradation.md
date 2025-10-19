# [outdated] Error Handling and Graceful Degradation

Labels:
- subcommand: outdated
- type: bug
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Implement the error handling strategy required by the spec: configuration errors fail fast with exit code 1; registry/network failures do not fail the command and instead yield undefined fields for affected features while exiting 0. Invalid or non-versionable feature identifiers are skipped.

## Specification Reference

- From SPEC.md Section: §9. Error Handling Strategy
- From GAP.md Section: 6. Error Handling Gaps

### Expected Behavior
- Config not found → exit 1, stderr message exactly per repo conventions/spec.
- Terminal flags singly provided → clap parse error (already covered by CLI task).
- Registry/network failures (tags/manifests) → log error; set `wanted/latest` to undefined for that feature; continue; overall exit 0.
- Invalid identifiers → skip, no error.

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/outdated.rs`
  - Wrap per-feature work in error-handling that maps external errors to `None` fields.
  - Map config discovery/read failures to proper error with message.
- Add/adjust error types in `crates/core/src/errors.rs` if helpful, or use `anyhow::Context`.

#### Specific Tasks
- [ ] Ensure overall command succeeds when some features fail to resolve.
- [ ] Ensure logs go to stderr and JSON/text outputs remain clean.
- [ ] Ensure exact wording for “No devcontainer.json found in workspace” where applicable to match existing patterns.

### 2. Data Structures
- No new structures; rely on `Option` fields.

### 3. Validation Rules
- [ ] Preserve exit code 0 for partial failures; exit 1 only for fatal config/IO issues.

### 4. Cross-Cutting Concerns
- Theme 6 - Error Messages: exact messages; provide context via `anyhow::Context`.

## Testing Requirements

### Unit Tests
- [ ] Map a simulated registry error to `None` fields.

### Integration Tests
- [ ] Run `outdated` with mocked registry failure; assert exit 0 and partial undefineds.

### Smoke Tests
- [ ] N/A beyond success cases; ensure command returns 0 on registry issues.

### Examples
- [ ] Add a failing-registry example fixture if practical.

## Acceptance Criteria
- [ ] Error mapping implemented as per spec.
- [ ] Tests prove graceful degradation.
- [ ] CI passes.

## Implementation Notes
- Use `tracing::error!` with redaction safeguards where applicable.

### Edge Cases to Handle
- Total registry outage → all features have undefined wanted/latest but command still exits 0.
- Config missing → exit 1.

### References
- SPEC: §9
- GAP: §6