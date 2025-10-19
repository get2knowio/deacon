# [up] Implement JSON stdout result and error contract

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Implement the required JSON output on stdout for the up subcommand, including both success and error shapes. This is critical for tooling and automation integrating with deacon and must strictly match the specification schema. Logs should remain on stderr with the existing log-level and log-format options respected.

## Specification Reference

**From SPEC.md Section:** §10. Output Specifications; §5. Core Execution Logic

**From GAP.md Section:** §9. Output Specifications – CRITICAL Missing; Summary of Critical Missing Features (1, 5)

### Expected Behavior
Success case output (stdout):
```json
{
  "outcome": "success",
  "containerId": "<string>",
  "composeProjectName": "<string, optional>",
  "remoteUser": "<string>",
  "remoteWorkspaceFolder": "<string>",
  "configuration": { "...": "included when --include-configuration" },
  "mergedConfiguration": { "...": "included when --include-merged-configuration" }
}
```

Error case output (stdout):
```json
{
  "outcome": "error",
  "message": "<string>",
  "description": "<string>",
  "containerId": "<string, optional>",
  "disallowedFeatureId": "<string, optional>",
  "didStopContainer": true,
  "learnMoreUrl": "<string, optional>"
}
```

Exit codes: 0 for success, 1 for failure.

### Current Behavior
- Returns `Result<()>` without structured stdout JSON. Exit codes not explicitly managed.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/up.rs` - return a structured UpResult on success; map errors to error JSON; ensure logs to stderr only.
- `crates/deacon/src/cli.rs` - wire flags `--include-configuration`, `--include-merged-configuration` into `UpArgs` if missing; pass through.
- `crates/deacon/src/commands/mod.rs` - if needed, add shared result serialization helpers.

#### Specific Tasks
- [ ] Define an internal struct mirroring UpResult schema or reuse types aligning with `DATA-STRUCTURES.md`.
- [ ] On success, assemble values: containerId (docker or compose), composeProjectName (when compose), remoteUser, remoteWorkspaceFolder.
- [ ] Conditionally include `configuration` and `mergedConfiguration` when flags are set.
- [ ] Serialize to JSON (serde_json) and write to stdout; ensure no trailing newline.
- [ ] Capture and map user/system/config errors to error shape; ensure exit code 1.
- [ ] Keep human logs on stderr and unaffected by JSON emission.

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
// UpResult schema (informal in DATA-STRUCTURES.md)
// Implement concrete Rust types for success and error shapes.
```

### 3. Validation Rules
- [ ] JSON output goes to stdout only; logs go to stderr (Theme 1).
- [ ] Error messages use standardized phrasing (Theme 6), but mapped into `message`/`description` fields.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: emit exactly as schema; no trailing whitespace.
- [x] Theme 6 - Error Messages: consistent phrasing; include actionable description.
- [x] Quality gates: no unwrap/expect; add tracing spans around result emission.

## Testing Requirements

### Unit Tests
- [ ] Serialize success result shape and verify fields and absences when flags off.
- [ ] Serialize error result shape, including optional fields.

### Integration Tests
- [ ] Run `up` in a minimal scenario and assert stdout parses to success schema; logs on stderr.
- [ ] Error scenario (invalid mount) yields error JSON and exit code 1.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to expect JSON on stdout and logs on stderr.

### Examples
- [ ] Update `examples/observability/json-logs/` if needed to include `up` example.

## Acceptance Criteria

- [ ] Success and error JSON emitted on stdout exactly per schema.
- [ ] Exit code 0 on success; 1 on error.
- [ ] `--include-configuration` and `--include-merged-configuration` control optional blobs.
- [ ] CI checks pass:
  ```bash
  cargo build --verbose
  cargo test --verbose -- --test-threads=1
  cargo test --doc
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  ```

## Implementation Notes

### Key Considerations
- Ensure separation of stdout/stderr streams to avoid corrupting JSON.
- Serialize without trailing newline; tests should use exact match.

### Edge Cases to Handle
- Error before container id known: omit `containerId`.
- Compose path: include `composeProjectName` when available.

## Definition of Done
- [ ] JSON contract implemented with tests and smoke.
- [ ] Documentation updated where necessary.
- [ ] GAP.md updated to mark §9 items completed.

## References
- Specification: `docs/subcommand-specs/up/SPEC.md` (§10)
- Gap Analysis: `docs/subcommand-specs/up/GAP.md` (§9)
- Data Structures: `docs/subcommand-specs/up/DATA-STRUCTURES.md`
- Parity Approach: `docs/PARITY_APPROACH.md` (Themes 1, 6)
