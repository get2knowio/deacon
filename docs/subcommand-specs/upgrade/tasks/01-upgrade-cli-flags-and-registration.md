---
subcommand: upgrade
type: enhancement
priority: high
scope: medium
---

# [upgrade] Implement CLI Flags and Subcommand Registration

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Add the `upgrade` subcommand to the main CLI with all required flags and validation. This enables users and automation to invoke `devcontainer upgrade` with correct options, and enforces exact error messages and pairing rules before any costly work runs.

## Specification Reference

**From SPEC.md Section:** §2. Command-Line Interface, §3. Input Processing Pipeline

**From GAP.md Section:** 1.1 Subcommand Registration, 1.2 Argument Validation

### Expected Behavior
- Subcommand: `devcontainer upgrade --workspace-folder <PATH> [--config <PATH>] [--docker-path <PATH>] [--docker-compose-path <PATH>] [--log-level <LEVEL>] [--dry-run] [--feature <ID>] [--target-version <X[.Y[.Z]]>]`
- Pairing: If exactly one of `--feature` or `--target-version` is provided → error: "The '--target-version' and '--feature' flag must be used together."
- Format: `--target-version` must match `^\d+(\.\d+(\.\d+)?)?$` → else: "Invalid version '<value>'. Must be in the form of 'x', 'x.y', or 'x.y.z'"

### Current Behavior
- No `upgrade` variant exists. No flags or validation implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs`
  - Add `Commands::Upgrade { ... }` variant with flags
  - Wire dispatch to handler (introduced in a later task)
  - Implement clap validations (regex via `value_parser` or post-parse custom check)

#### Specific Tasks
- [ ] Add subcommand variant:
  - `dry_run: bool`
  - hidden: `feature: Option<String>`, `target_version: Option<String>`
- [ ] Add optional flags: `--docker-path`, `--docker-compose-path`, `--config`, `--log-level`
- [ ] Enforce pairing rule (feature <-> target-version)
- [ ] Validate `--target-version` format with regex
- [ ] Ensure exact error messages from the spec

### 2. Data Structures

Required from DATA-STRUCTURES.md (Rust adaptation for clap struct shape):
```rust
#[derive(Debug, Clone)]
pub struct UpgradeCliArgs {
    pub workspace_folder: std::path::PathBuf,
    pub config: Option<std::path::PathBuf>,
    pub docker_path: Option<String>,
    pub docker_compose_path: Option<String>,
    pub log_level: Option<String>,
    pub dry_run: bool,
    pub feature: Option<String>,
    pub target_version: Option<String>,
}
```

### 3. Validation Rules
- [ ] Pairing: if exactly one of `feature` or `target_version` is set → error
- [ ] Format: `target_version` must match `^\d+(\.\d+(\.\d+)?)?$`
- [ ] Error message: "The '--target-version' and '--feature' flag must be used together."
- [ ] Error message: "Invalid version '<value>'. Must be in the form of 'x', 'x.y', or 'x.y.z'"

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 2 - CLI Validation: Pairing + regex preflight before execution
- [ ] Theme 6 - Error Messages: Exact strings; use clap validators or custom parsing with `anyhow::bail!`

## Testing Requirements

### Unit Tests
- [ ] Missing pairing triggers exact error message
- [ ] Invalid `--target-version` triggers exact error message
- [ ] Valid combinations parse successfully

### Integration Tests
- [ ] CLI parse round-trip for common flag combos

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include `upgrade --help` presence

### Examples
- [ ] N/A for this task; help text coverage only

## Acceptance Criteria

- [ ] `upgrade` subcommand appears in `--help`
- [ ] All flags present with correct clap annotations
- [ ] Pairing and regex validation enforced with exact messages
- [ ] No side effects yet; dispatch placeholder added for next task
- [ ] CI checks pass:
  ```bash
  cargo build --verbose
  cargo test --verbose -- --test-threads=1
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  ```

## Implementation Notes
- Use `ArgAction::SetTrue` for `--dry-run` if applicable.
- For hidden flags, set `hide = true` and short aliases `-f`/`-v`.
- Prefer custom `value_parser` or a small post-parse validation to print the exact error strings.

### Edge Cases to Handle
- `--feature` without `--target-version` and vice versa
- Whitespace or leading/trailing spaces in version string

## References
- Specification: `docs/subcommand-specs/upgrade/SPEC.md` (§2–3)
- Gap Analysis: `docs/subcommand-specs/upgrade/GAP.md` (§1)
- Parity Approach: `docs/PARITY_APPROACH.md` (Theme 2, Theme 6)
