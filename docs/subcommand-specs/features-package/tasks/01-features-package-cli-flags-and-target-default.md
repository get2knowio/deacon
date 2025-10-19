---
subcommand: features-package
type: enhancement
priority: high
scope: medium
labels: ["subcommand: features-package", "type: enhancement", "priority: high", "scope: medium"]
---

# [features-package] Implement CLI Flags & Target Default

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Align the `features package` CLI with the spec: add `--output-folder/-o` (alias existing `--output`), add `--force-clean-output-folder/-f`, and make the positional `target` default to `.`. This improves ergonomics and ensures parity with tooling and docs.

## Specification Reference

**From SPEC.md Section:** “§2. Command-Line Interface” and “§3. Input Processing Pipeline”

**From GAP.md Section:** “3. MISSING: `--force-clean-output-folder` Flag” and “4. MISSING: Positional `target` Argument”

### Expected Behavior
- `devcontainer features package [target] [--output-folder <dir>] [--force-clean-output-folder]`
- `target` defaults to current directory when omitted.
- `--output-folder`, `-o` set output directory; create if missing.
- `--force-clean-output-folder`, `-f` clears output directory before packaging.

### Current Behavior
- Uses `--output` only; no `-o` alias; `--force-clean-output-folder` missing.
- `path` positional is required and does not default to `.`.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs`
  - Update `FeatureCommands::Package`:
    - Change `path: String` to `path: Option<String>` with default `.` handling.
    - Add `#[arg(long = "output-folder", short = 'o', alias = "output")] output_folder: Option<String>`.
    - Add `#[arg(long = "force-clean-output-folder", short = 'f')] force_clean_output_folder: bool`.
- `crates/deacon/src/commands/features.rs`
  - `execute_features`: update match arm to pass new fields.
  - `execute_features_package`: accept `target: &str`, `output_dir: &str`, `force_clean: bool` (wire-up only; core logic in separate tasks).

#### Specific Tasks
- [ ] Add `--output-folder` with `-o` short alias and `--output` alias for back-compat.
- [ ] Add `--force-clean-output-folder` with `-f` short alias.
- [ ] Make positional `target` optional; default to `.` when not provided.
- [ ] Validate that `output-folder` is a non-empty path string; error if empty.
- [ ] Use exact error message: "Output folder path must not be empty." (Theme 6)

### 2. Data Structures
N/A for this task.

### 3. Validation Rules
- [ ] Required pairing: none.
- [ ] Mutual exclusion: none.
- [ ] Defaults: `target = "."`, `output-folder = "./output"` if not provided by user (final default applied in logic task).
- [ ] Error message: "Output folder path must not be empty."

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 2 - CLI Validation: Apply clap validation and clear errors.
- [ ] Theme 6 - Error Messages: Use exact messages and actionable guidance.

## Testing Requirements

### Unit Tests
- [ ] CLI parsing: omitting `target` resolves to `.`.
- [ ] `-o` and `--output-folder` map to the same value; `--output` alias accepted.
- [ ] `-f` sets force-clean flag.
- [ ] Empty `--output-folder ""` yields error with exact message.

### Integration Tests
- [ ] `deacon features package -o out` runs and uses `out`.
- [ ] `deacon features package` with no target uses current dir.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` if it asserts CLI help/usage.

### Examples
- [ ] Update `examples/feature-management/*` README snippets to use `-o`.

## Acceptance Criteria
- [ ] Flags implemented with correct aliases and defaults.
- [ ] Positional `target` defaults to `.`.
- [ ] Validation error on empty output folder path with exact message.
- [ ] All CI checks pass.

## Implementation Notes
- Keep `--output` as an alias for compatibility; prefer `--output-folder` in docs.
- Actual defaulting of output path to `./output` can be centralized in execution function to minimize duplication across callers.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§2–3)
- `docs/subcommand-specs/features-package/GAP.md` (Sections 3–4)
- `docs/PARITY_APPROACH.md` (Themes 1–2,6)
