---
subcommand: build
type: enhancement
priority: critical
scope: medium
---

# [build] Implement spec-compliant JSON output contract

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Align the build subcommand's stdout output with the spec’s JSON contract. Replace the current `BuildResult` JSON with `{ outcome: 'success', imageName }` on success and `{ outcome: 'error', message, description? }` on error. Ensure logs are written to stderr and stdout contains only the result JSON.

## Specification Reference

**From SPEC.md Section:** §10 Output Specifications

**From GAP.md Section:** 4.1 Standard Output Format; 4.2 Error Message Format; 8.2 Build Result Structure

### Expected Behavior
- On success: stdout prints JSON with `outcome: "success"` and `imageName` as a string or array.
- On error: stdout prints JSON with `outcome: "error"`, `message`, and optional `description`.
- No extra whitespace or logs in stdout; logs go to stderr.

### Current Behavior
- A large `BuildResult` structure is serialized to JSON. Field names and shape don’t match the spec; logs may mix with stdout.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – introduce new output enums/structs (success/error) and update `output_result` to emit spec-compliant JSON to stdout and move human-readable logs to stderr.
- `crates/deacon/src/commands/build.rs` – update call sites to pass final image names.

#### Specific Tasks
- [ ] Add `BuildSuccessResult` and `BuildErrorResult` per DATA-STRUCTURES.md.
- [ ] Modify `output_result` to emit the new shape, using only `imageName` and not `image_id`.
- [ ] Ensure only result JSON is printed to stdout; ensure log lines use stderr via existing redaction writer on stderr.
- [ ] Ensure single vs multiple image names render as string vs string[].
- [ ] Wire error paths to produce spec-compliant error JSON and exit code 1.

### 2. Data Structures

```rust
#[derive(Serialize)]
#[serde(tag = "outcome")]
enum ExecutionResult {
    #[serde(rename = "success")]
    Success { imageName: serde_json::Value },
    #[serde(rename = "error")]
    Error { message: String, #[serde(skip_serializing_if = "Option::is_none")] description: Option<String> },
}
```

### 3. Validation Rules
- Not applicable; focuses on output shape. Pair with exact error messages per validation task.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: strict stdout JSON shape, no noise.
- [x] Theme 6 - Error Messages: ensure message text matches spec where applicable.

## Testing Requirements

### Unit Tests
- [ ] Tests to assert success JSON shape (single and multiple tags).
- [ ] Tests to assert error JSON shape with/without description.

### Integration Tests
- [ ] CLI invocation with `--output-format json` returning the expected JSON.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to reflect new JSON success shape.

### Examples
- [ ] Update `examples/observability/json-logs/` if applicable to show separation of stdout/stderr.

## Acceptance Criteria
- [ ] Stdout contains only the spec-compliant result JSON.
- [ ] Error cases emit spec error JSON with exit code 1.
- [ ] CI checks pass.

## Implementation Notes
- For multiple tags: use `serde_json::Value::Array` vs `Value::String` based on count.
- Ensure no trailing newline if the spec implies strictness; if current writer enforces newline, align with global convention used elsewhere.

## Definition of Done
- [ ] Output shapes and tests updated.
- [ ] Smoke tests adjusted.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§10)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§4.1, §8.2)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
