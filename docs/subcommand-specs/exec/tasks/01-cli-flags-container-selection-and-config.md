---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Implement CLI Flags: Container Selection & Config

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [ ] Other: 

## Description
Add the full set of container selection and configuration flags for the exec subcommand and enforce required validation. These flags are critical to target the correct container and to resolve configuration when `--workspace-folder`/`--config` are used.

## Specification Reference

- From SPEC.md Section: §2 Command-Line Interface, §3 Input Processing Pipeline, §4 Configuration Resolution
- From GAP.md Section: 1. CLI Interface Gaps, 3. Configuration Resolution Gaps

### Expected Behavior
From SPEC §3 parse pseudocode (container/config subset):

- Supported flags:
  - `--workspace-folder <PATH>`
  - `--container-id <ID>`
  - `--id-label <name=value>` (repeatable; regex `/.+=.+/`)
  - `--config <PATH>`
  - `--override-config <PATH>`
  - `--mount-workspace-git-root` (boolean, default true)
- Validation:
  - One of `--container-id`, `--id-label`, or `--workspace-folder` is required.
  - `--id-label` must match `<name>=<value>` with non-empty value.

### Current Behavior
Per GAP §1 and §3:
- Missing `--container-id` and `--mount-workspace-git-root`.
- `--workspace-folder`, `--config`, `--override-config` exist globally but not wired at subcommand scope.
- `--id-label` exists but value non-empty validation is partial.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/exec.rs` — Add clap flags and map into internal args struct.
- `crates/deacon/src/cli.rs` (if arg structs live here) — Ensure flags are subcommand-scoped and help text matches spec.
- `crates/deacon/src/validation.rs` (or equivalent) — Add helpers for `id-label` regex and paired requirements.

#### Specific Tasks
- [ ] Add flags: `--container-id`, `--id-label <name=value>...`, `--workspace-folder`, `--config`, `--override-config`, `--mount-workspace-git-root`.
- [ ] Enforce requirement: one of `--container-id`, `--id-label`, or `--workspace-folder` must be provided; error: "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
- [ ] Validate `--id-label` format with regex `/.+=.+/`; error: "id-label must match <name>=<value>".
- [ ] Ensure help/descriptions mirror SPEC §2 exactly.
- [ ] Wire parsed values into the internal input structure used by exec.

### 2. Data Structures

Required from DATA-STRUCTURES.md:
```rust
pub struct ParsedInput {
    pub workspace_folder: Option<String>,
    pub container_id: Option<String>,
    pub id_labels: Option<Vec<String>>, // list of "name=value"
    pub config_file: Option<String>,    // URI/path
    pub override_config_file: Option<String>,
    pub mount_workspace_git_root: bool,
    // ...
}
```

### 3. Validation Rules
- [ ] Regex: `--id-label` must match `/.+=.+/`.
- [ ] Required: One of `--container-id`, `--id-label`, or `--workspace-folder`.
- [ ] Error message: "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
- [ ] Error message: "id-label must match <name>=<value>".

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 2 - CLI Validation: Use clap `required_unless_present_any`/manual check; validate before any IO.
- [ ] Theme 6 - Error Messages: Use the exact messages from SPEC.

## Testing Requirements

### Unit Tests
- [ ] Parse each flag and assert values in `ParsedInput`.
- [ ] Validation error when none of the selection flags are provided.
- [ ] Validation error when `--id-label` value does not include a non-empty value.

### Integration Tests
- [ ] `exec` with `--container-id` targets the right container (happy path; mock if needed).
- [ ] Error when `--config`/`--workspace-folder` is provided but config not found (per SPEC §4).

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include one `exec` invocation using `--container-id` or `--id-label`.

### Examples
- [ ] Add/update example in `examples/cli/` demonstrating `--container-id` and `--id-label` usage.
- [ ] Update `examples/README.md` index.

## Acceptance Criteria
- [ ] All flags implemented with correct types and help text.
- [ ] Validation rules enforced with exact error messages.
- [ ] Parsing populates `ParsedInput` correctly.
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
- Ensure flags are subcommand-scoped for `exec` even if also available globally.
- Keep default `mount_workspace_git_root=true`.

### Edge Cases to Handle
- Multiple `--id-label` entries; preserve order.
- Paths relative to CWD for `--workspace-folder` and `--config`.

### Reference Implementation
- SPEC §3 pseudocode and §2 CLI list.

## Definition of Done
- [ ] Code implements all requirements from specification.
- [ ] Tests pass and examples updated.
- [ ] GAP.md updated to mark these flags as implemented.

## References
- Specification: `docs/subcommand-specs/exec/SPEC.md` (§2–§4)
- Gap Analysis: `docs/subcommand-specs/exec/GAP.md` (§1, §3)
- Data Structures: `docs/subcommand-specs/exec/DATA-STRUCTURES.md`
- Parity Approach: `docs/PARITY_APPROACH.md`
