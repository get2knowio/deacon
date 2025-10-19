issue: 294
title: "[read-configuration] Validation & Error messages: exact strings and edge cases"
labels:
  - subcommand: read-configuration
  - type: enhancement
  - type: error-handling
  - priority: high
  - scope: medium
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: ___________

## Description
Implement strict CLI validation for read-configuration and emit the exact error messages defined in the spec. This covers the required selector rule, id-label format validation, terminal dimensions pairing, and JSON parsing errors for additional-features. Ensure stderr messages match the spec verbatim and that error flows print no JSON to stdout.

## Specification Reference

From SPEC.md Section: §2. Command-Line Interface, §3. Input Processing Pipeline, §9. Error Handling Strategy, §14. Edge Cases, §15. Testing Strategy

From GAP.md Section: 1.1 Missing Flags (validation gaps), 1.3 Argument Validation, 2.1 Missing Components (selector requirement), 8. Error Handling Strategy (missing messages), 14. Edge Cases

### Expected Behavior
Extracted from SPEC.md:
- At least one of --container-id, --id-label, or --workspace-folder must be provided; otherwise, exit 1 with:
	- "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
- Every --id-label must match <name>=<value> with a non-empty value; otherwise, exit 1 with:
	- "Unmatched argument format: id-label must match <name>=<value>."
- --terminal-columns and --terminal-rows must be provided together; if exactly one is provided, exit 1 with a clear validation error.
- --additional-features must parse as a JSON object mapping string -> (string|boolean|object); on parse error, exit 1 with a parse/validation error.
- When a config path is requested but not found, exit 1 with:
	- "Dev container config (<path>) not found."
- For a non-object root in a config file, exit 1 with:
	- "Dev container config (...) must contain a JSON object literal."
- On error, log to stderr (honoring --log-format/--log-level); do not print JSON to stdout.

### Current Behavior
Extracted from GAP.md:
- No enforcement of the required selector constraint; command assumes workspace-based resolution.
- No validation for id-label format (flag missing and no checks).
- Terminal dimension pairing validation not implemented (flags missing).
- No validation for additional-features JSON (flag missing).
- Some errors bubble from config loader but exact strings are not standardized across all cases.

## Implementation Requirements

### 1. Code Changes Required

Files to Modify
- `crates/deacon/src/commands/read_configuration.rs` – Add pre-execution validation enforcing selector requirement and wiring for new validations/messages. Ensure no stdout on error paths.
- `crates/deacon/src/cli.rs` – Define flags if not already present for this subcommand: `--container-id`, `--id-label <name=value>...`, `--terminal-columns`, `--terminal-rows`, `--additional-features`. Add clap-level `requires_all` between terminal flags where possible; still validate at runtime for exact message wording.
- `crates/core/src/config.rs` or validation helper module (if applicable) – Reuse/extend existing error types or add a small error enum for this command using `thiserror` to ensure consistent messages.

Specific Tasks
- [ ] Enforce required selector rule: require any of `container_id`, non-empty `id_label`, or `workspace_folder`; else return error with exact message.
- [ ] Validate `--id-label` inputs against regex `/.+=.+/`; on failure, return error with exact message.
- [ ] Validate terminal dimension pairing: if exactly one of `terminal_columns` or `terminal_rows` is set, return error. Message should clearly indicate the pairing requirement (align with spec wording style).
- [ ] Parse `--additional-features` as JSON object; on parse error, return a clear parse/validation error (stderr) and exit 1.
- [ ] Ensure that when `--config` or `--workspace-folder` points to a missing file, the error message strictly matches: `Dev container config (<path>) not found.`
- [ ] Ensure non-object root error uses the exact required string: `Dev container config (...) must contain a JSON object literal.`
- [ ] Guarantee that on any validation or parsing error, nothing is printed to stdout.
- [ ] Add tracing fields for validation failures to aid diagnostics without altering message text.

### 2. Data Structures

Required from DATA-STRUCTURES.md:
```rust
// ParsedInput fields relevant to validation
struct ParsedInput {
		container_id: Option<String>,
		id_label: Vec<String>, // each must match <name>=<value>
		workspace_folder: Option<String>,
		terminal_columns: Option<u32>,
		terminal_rows: Option<u32>,
		additional_features: serde_json::Value, // must be a JSON object when provided
}
```

### 3. Validation Rules
- [ ] Required selector: One of `--container-id`, `--id-label`, `--workspace-folder` must be provided.
- [ ] `--id-label` format: must match `<name>=<value>` (non-empty on both sides).
- [ ] Terminal pairing: `--terminal-columns` requires `--terminal-rows` and vice versa.
- [ ] `--additional-features`: must parse as a JSON object; reject non-object types and invalid JSON.
- [ ] Error messages (exact):
	- "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
	- "Unmatched argument format: id-label must match <name>=<value>."
	- "Dev container config (<path>) not found."
	- "Dev container config (...) must contain a JSON object literal."

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 1 - JSON Output Contract: No JSON output on error; successful runs always emit the structured payload.
- [ ] Theme 2 - CLI Validation: Implement clap-level and runtime validation; prefer explicit runtime errors to guarantee exact strings.
- [ ] Theme 6 - Error Messages: Use exact strings from the spec; avoid variation and punctuation drift.

## Testing Requirements

### Unit Tests
- [ ] Selector requirement: no selector flags -> exit 1 with the exact missing-argument message.
- [ ] `--id-label` bad formats (e.g., `foo`, `name=`, `=value`) -> exit 1 with id-label message.
- [ ] Terminal flags: only columns or only rows provided -> exit 1 with pairing error.
- [ ] `--additional-features` invalid JSON and valid-but-non-object (e.g., `[]`, `123`) -> exit 1 with parse/validation error.

### Integration Tests
- [ ] Missing config when path is provided: verify exact "Dev container config (<path>) not found." message.
- [ ] Non-object root config file: verify exact object-literal error string.
- [ ] Ensure no stdout JSON is printed on failures (stdout empty, stderr contains message).

### Smoke Tests
- [ ] Ensure existing smoke tests still pass; add a negative-path smoke asserting selector requirement behavior when invoking without any selectors.

### Examples
- [ ] Not required for this issue; covered by tests. If helpful, add a tiny invalid fixture under `fixtures/` (non-object root) for deterministic assertions.

## Acceptance Criteria

- [ ] All validation rules enforced as specified.
- [ ] Exact error messages emitted (string-equal) for the scenarios listed.
- [ ] No JSON printed to stdout on error; stderr contains only the error/logs per `--log-format`.
- [ ] Tests added and passing:
	```bash
	cargo build --verbose
	cargo test --verbose -- --test-threads=1
	cargo fmt --all
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	```
- [ ] No `unwrap()` or `expect()` in production code paths for these validations.
- [ ] Errors use `thiserror` or well-scoped anyhow with `.context(...)` at CLI boundary.
- [ ] Tracing added where helpful; message text remains spec-exact.

## Implementation Notes

Key Considerations
- GAP.md highlights missing selector validation and id-label validation as critical; tackle these first to unblock broader test coverage.
- Prefer runtime validation for exact messages because clap's auto-generated errors may not match required strings.

Edge Cases to Handle
- Only container flags provided (no config/workspace): must pass selector validation and proceed; validation should not reject this.
- Multiple `--id-label` entries: each must validate; first failure should stop processing with the exact message.
- Paths with spaces in `--config` or `--workspace-folder`: error messages must include the resolved path in parentheses without additional quoting.
- Permission-denied vs not-found: preserve loader’s detailed stderr, but keep top-level message wording per spec where required.

Reference Implementation
- Align with SPEC.md pseudocode in §3 and message strings in §9.

## Definition of Done

- [ ] Code implements all validation and error message requirements from the specification.
- [ ] All tests pass (unit, integration, smoke) locally and in CI.
- [ ] Documentation (this issue’s acceptance criteria) satisfied; no drift in error strings.
- [ ] CI pipeline passes all checks and no new clippy warnings introduced.


