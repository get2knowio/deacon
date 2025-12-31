# [outdated] Implement CLI Subcommand and Flags

Labels:
- subcommand: outdated
- type: enhancement
- priority: high
- scope: medium

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Add the `outdated` subcommand to the CLI with its specific flags and validation. This enables users to invoke the command and select output format (text/json) and optionally hint terminal dimensions. Also wire a dispatcher entry to call an `execute_outdated` implementation stub.

## Specification Reference

- From SPEC.md Section: §2. Command-Line Interface; §3. Input Processing Pipeline
- From GAP.md Section: 1. Missing CLI Interface

### Expected Behavior
- Syntax: `devcontainer outdated --workspace-folder <path> [--config <path>] [--output-format <text|json>] [--log-level <...>] [--log-format <...>] [--terminal-columns <n> --terminal-rows <n>]`.
- Validation: `--terminal-columns` implies `--terminal-rows` and vice versa; both must be positive integers.
- `--output-format` defaults to `text`.

### Current Behavior
- No `Outdated` variant exists in `Commands`.
- No flags or validation for the subcommand.
- No dispatcher branch.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs`
  - Add `Commands::Outdated { output_format, terminal_columns, terminal_rows }`.
  - Add dispatcher branch to call `execute_outdated(...)`.
- `crates/deacon/src/commands/outdated.rs` (NEW)
  - Create module exporting `OutdatedArgs` and `execute_outdated(args) -> anyhow::Result<()>` (stub for now).

#### Specific Tasks
- [ ] Add subcommand variant `Outdated` with flags:
  - `--output-format <text|json>` (default: text)
  - `--terminal-columns <n>` (requires `--terminal-rows`)
  - `--terminal-rows <n>` (requires `--terminal-columns`)
- [ ] Reuse global `--workspace-folder` and `--config`.
- [ ] Enforce positive terminal dimensions via `Cli::validate`.
- [ ] Help text and descriptions per SPEC wording.

### 2. Data Structures

Required from DATA-STRUCTURES.md (Rust translation):
```rust
pub struct OutdatedArgs {
    pub workspace_folder: Option<std::path::PathBuf>,
    pub config_file: Option<std::path::PathBuf>,
    pub output_format: crate::cli::OutputFormat,
    pub log_level: crate::cli::LogLevel,
    pub log_format: crate::cli::LogFormat,
    pub terminal_columns: Option<u32>,
    pub terminal_rows: Option<u32>,
}
```

### 3. Validation Rules
- [ ] `--terminal-columns` and `--terminal-rows` must be provided together (clap `requires`).
- [ ] Terminal values must be positive integers (reject zero).
- [ ] `--output-format` must be one of `text` or `json`.
- [ ] Error message format per Theme 6.

### 4. Cross-Cutting Concerns
- Theme 2 - CLI Validation: paired flags via clap `requires`, positivity checks.
- Theme 6 - Error Messages: standardized phrasing.

## Testing Requirements

### Unit Tests
- [ ] Parsing with defaults (`--output-format` defaults to text).
- [ ] Validation for terminal dimensions pairing and positivity.

### Integration Tests
- [ ] `deacon outdated --help` shows correct flags and descriptions.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include `outdated --help` sanity.

### Examples
- [ ] N/A here (covered in separate docs/examples task).

## Acceptance Criteria
- [ ] `outdated` subcommand appears in `--help` with correct flags.
- [ ] Dispatcher branch compiles and calls `execute_outdated` stub.
- [ ] Global validation covers terminal flags.
- [ ] CI passes: build, test, fmt, clippy.

## Implementation Notes
- Reuse existing enums: `OutputFormat`, `LogLevel`, `LogFormat`.
- Keep logic minimal; core execution in separate tasks.

### Edge Cases to Handle
- Only one terminal dimension provided → clap parse error.
- Zero or negative terminal values → validation error.

### References
- Specification: `docs/subcommand-specs/outdated/SPEC.md` (§2, §3)
- Gap Analysis: `docs/subcommand-specs/outdated/GAP.md` (§1)
- Parity Approach: `docs/PARITY_APPROACH.md` (Themes 2, 6)