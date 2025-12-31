---
subcommand: templates
type: enhancement
priority: high
scope: medium
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: medium"]
---

# [templates] Implement Apply CLI Flags, JSON Validation, and Output Contract

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Description
Align the `templates apply` CLI with the specification: introduce the required flags, strict JSON validation for `--template-args`, `--features`, and `--omit-paths`, and ensure the command produces the exact JSON output shape `{ files: string[] }` to stdout while sending logs to stderr. This enables spec-compliant invocation and programmatic consumption.

## Specification Reference

**From SPEC.md Section:** §2 Command-Line Interface; §3 Input Processing Pipeline; §10 Output Specifications

**From GAP.md Section:** 1.1 `templates apply` Command — CLI mismatches and missing JSON output

### Expected Behavior
Extracted from SPEC (§2, §3, §10):
- Flags:
  - `--workspace-folder, -w <path>` (required, default `.`)
  - `--template-id, -t <oci-ref>` (required)
  - `--template-args, -a <json>` (object of string->string; default `{}`)
  - `--features, -f <json>` (array of `{ id, options }`; default `[]`)
  - `--omit-paths <json>` (array of strings; default `[]`)
  - `--tmp-dir <path>` (optional)
  - `--log-level <info|debug|trace>` (optional)
- Validation errors exit non-zero with exact messages per spec.
- Stdout emits exactly `{ "files": ["..."] }` on success; no extra whitespace/newlines.

### Current Behavior
From GAP.md §1.1: positional `template` arg instead of `--template-id`, `--output` instead of `--workspace-folder`, key=value `--option` instead of JSON `--template-args`, missing `--features`, missing `--omit-paths`, no JSON output shape.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/templates.rs` — Redefine `apply` subcommand flags and parsing; emit JSON output to stdout.
- `crates/core/src/templates.rs` — Extend `ApplyOptions` with `omit_paths` and pass-through `features` holder (or separate struct) if needed by later tasks.
- `crates/core/src/serde_utils.rs` (if exists) or add helper — JSON/JSONC parsing utilities with validation and error messages per spec.

#### Specific Tasks
- [ ] Replace positional `template` with `--template-id, -t <oci-ref>`.
- [ ] Add `--workspace-folder, -w <path>` (default `.`) replacing `--output`.
- [ ] Add `--template-args, -a <json>` (object; values must be strings).
- [ ] Add `--features, -f <json>` (array; each entry has `id: string`; `options?: object`).
- [ ] Add `--omit-paths <json>` (array of strings; default empty).
- [ ] Add optional `--tmp-dir <path>`.
- [ ] Validate JSON inputs per §3; return spec error messages:
  - `Invalid template arguments provided` for malformed or wrong-shape `--template-args`/`--features`.
  - `Invalid --omit-paths argument provided` for malformed omit paths.
- [ ] Print success JSON to stdout: `{ files: string[] }`; logs to stderr only (Theme 1).
- [ ] Map flags into a new/updated `ApplyArgs` passing through to core.

### 2. Data Structures

Required from DATA-STRUCTURES.md:
```rust
// CLI layer
pub struct ApplyArgs {
    pub workspace_folder: String,
    pub template_id: String, // OCI ref
    pub template_args: String, // JSON string; parsed to TemplateOptions
    pub features: String,     // JSON string; parsed to TemplateFeatureOption[]
    pub omit_paths: Option<String>,
    pub tmp_dir: Option<String>,
    pub log_level: String,
}

// Core layer extension
#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    pub options: std::collections::HashMap<String, deacon_core::features::OptionValue>,
    pub overwrite: bool,
    pub dry_run: bool,
    // New in this issue:
    pub omit_paths: Vec<String>,
}
```

### 3. Validation Rules
- [ ] `--template-args` must parse to a JSON object; all values are strings (Numbers/bools invalid).
- [ ] `--features` must parse to JSON array; each entry requires `id: string`; `options` if present is object.
- [ ] `--omit-paths` must parse to JSON array of strings.
- [ ] Error message strings must match spec exactly.

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 1 - JSON Output Contract: stdout only, exact structure.
- [ ] Theme 2 - CLI Validation: conflicts/requires (none), strict validation and messages.
- [ ] Theme 6 - Error Messages: exact text.

## Testing Requirements

### Unit Tests
- [ ] Test flag parsing defaults and required flags.
- [ ] JSON validation: malformed `--template-args`, non-object, non-string values.
- [ ] JSON validation: malformed `--features`, missing `id`, wrong types.
- [ ] JSON validation: malformed `--omit-paths`.

### Integration Tests
- [ ] CLI invocation happy path with minimal valid inputs; assert stdout JSON structure and stderr logs.
- [ ] Error scenarios emit correct messages and non-zero exit.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include `templates apply` basic run (dry registry mocking acceptable when core is wired in later tasks).

### Examples
- [ ] Add/update example usage in `examples/template-management/templates-apply/README.md`.
- [ ] Update `examples/README.md` index entry.

## Acceptance Criteria

- [ ] CLI flags implemented and validated per spec.
- [ ] Stdout JSON `{ files: [] }` on success, logs to stderr.
- [ ] Exact error messages and non-zero exit on validation failures.
- [ ] All CI checks pass:
  ```bash
  cargo build --verbose
  cargo test --verbose -- --test-threads=1
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  ```

## Implementation Notes

### Key Considerations
- Input JSON must be parsed before any network I/O (fail fast).
- Keep CLI backward-compatible flags deprecated only if maintainers choose; default is spec alignment.

### Edge Cases to Handle
- Empty `{}` for `--template-args` and `[]` for `--features` and `--omit-paths` are valid.
- Extra whitespace and JSONC comments should be rejected unless we intentionally support JSONC; spec text says JSON (strict); if JSONC utility exists, ensure consistent behavior.

### Reference Implementation
- Reference Dev Containers CLI TypeScript implementation of `templates apply` flag handling.

## Definition of Done

- [ ] CLI matches spec semantics for `apply`.
- [ ] Tests added and passing.
- [ ] Docs/examples updated.
