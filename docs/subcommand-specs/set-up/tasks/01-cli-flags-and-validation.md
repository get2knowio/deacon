---
subcommand: set-up
type: enhancement
priority: high
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: cli"]
---

# [set-up] Implement CLI flags and validation

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add the `set-up` subcommand to the CLI with all required flags and validation. This enables users to target an already-running container and shape behavior (lifecycle control, dotfiles, output selection) while enforcing spec-compliant argument rules and error messages.

## Specification Reference

**From SPEC.md Section:** §2. Command-Line Interface, §3. Input Processing Pipeline, §9. Error Handling Strategy

**From GAP.md Section:** 1.1 CLI Interface (100% Missing)

### Expected Behavior
- Define `devcontainer set-up --container-id <id> [options]` with flags listed in SPEC §2.
- Validate `--remote-env` entries match `<name>=<value>`.
- Validate terminal dimensions coupling: `--terminal-columns` requires `--terminal-rows` and vice versa.
- If `--config` is provided and path is missing, error: `Dev container config (<path>) not found.`

### Current Behavior
- Subcommand not present; no flags, no validation.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` – Add `SetUp` variant and flags per spec.
- `crates/deacon/src/commands/mod.rs` – Export `set_up` command module.
- `crates/deacon/src/commands/set_up.rs` – New file; define `SetUpArgs` and stub `run` delegating to core (returns NotImplemented for now).
- `crates/deacon/src/output.rs` – Ensure JSON/stdout vs logs/stderr wiring supports this subcommand (reuse existing pattern).

#### Specific Tasks
- [ ] Add subcommand: `set-up` with required and optional flags:
  - Required: `--container-id <string>`
  - Optional: `--config <path>`, `--log-level <info|debug|trace>`, `--log-format <text|json>`,
    `--terminal-columns <number>`, `--terminal-rows <number>`,
    `--skip-post-create`, `--skip-non-blocking-commands`,
    `--remote-env <NAME=VALUE>` (repeatable),
    `--dotfiles-repository <string>`, `--dotfiles-install-command <string>`, `--dotfiles-target-path <path>`,
    `--container-session-data-folder <path>`,
    `--user-data-folder <path>`, `--container-data-folder <path>`, `--container-system-data-folder <path>`,
    `--include-configuration`, `--include-merged-configuration`,
    `--docker-path <string>`
- [ ] Add clap validations:
  - `--terminal-columns` requires `--terminal-rows` (and vice versa)
  - `--remote-env` value regex `/.+=.*/`
- [ ] Resolve `--config` to a file; if missing, surface exact message: `Dev container config (<path>) not found.`
- [ ] Map flags into `SetUpArgs` with strong types.
- [ ] Wire `SetUpArgs` → `deacon_core::setup::execute_set_up` (to be implemented in later issues); for now, return a clear Not Implemented error string: `Not implemented yet: set-up core execution`.

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
// Input structures (CLI-facing)
pub struct SetUpOptions {
    pub docker_path: Option<String>,
    pub container_data_folder: Option<String>,
    pub container_system_data_folder: Option<String>,
    pub container_session_data_folder: Option<String>,
    pub container_id: String,
    pub config_file: Option<std::path::PathBuf>,
    pub log_level: LogLevel, // info|debug|trace
    pub log_format: LogFormat, // text|json
    pub terminal_dimensions: Option<(u16, u16)>, // (columns, rows)
    pub post_create_enabled: bool,
    pub skip_non_blocking: bool,
    pub remote_env: std::collections::BTreeMap<String, String>,
    pub persisted_folder: Option<std::path::PathBuf>,
    pub dotfiles: DotfilesConfiguration,
    pub include_config: bool,
    pub include_merged_config: bool,
}

pub struct DotfilesConfiguration {
    pub repository: Option<String>,
    pub install_command: Option<String>,
    pub target_path: String, // default ~/dotfiles
}
```

### 3. Validation Rules
- [ ] Validate `--remote-env` with regex `/.+=.*/`; error message: `Invalid --remote-env entry: expected NAME=VALUE.`
- [ ] Paired requirements: `--terminal-columns` requires `--terminal-rows` and vice versa.
- [ ] Path existence for `--config`; error message: `Dev container config (<path>) not found.`

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: Ensure subcommand can emit JSON via stdout; human logs to stderr.
- [x] Theme 2 - CLI Validation: Use clap `requires` and custom validators; exact error messages.
- [x] Theme 6 - Error Messages: Use exact strings from SPEC for missing config.

## Testing Requirements

### Unit Tests
- [ ] Flag parsing happy path with full set of options.
- [ ] `--remote-env` invalid entry rejects with expected error text.
- [ ] Terminal dimensions coupling enforced.
- [ ] Missing `--container-id` produces clap error.

### Integration Tests
- [ ] `crates/deacon/tests/integration_set_up.rs`: invocation with `--config` to non-existent path returns the precise message.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` to include `deacon set-up --help` snapshot to guard flags.

### Examples
- [ ] Add a minimal usage snippet to `examples/README.md` referencing set-up (to be expanded in later issues).

## Acceptance Criteria
- [ ] Subcommand appears in `--help` with all flags and descriptions.
- [ ] Validations enforced and error messages match specification exactly.
- [ ] `cargo build --verbose` PASS, `cargo test --verbose -- --test-threads=1` PASS.
- [ ] `cargo fmt --all -- --check` PASS, `cargo clippy --all-targets -- -D warnings` PASS.

## Implementation Notes
- Keep `SetUpArgs` minimal and map directly to core `SetUpOptions` later.
- Reuse global logging flags/types to avoid duplication.

### Edge Cases to Handle
- Duplicate `--remote-env` keys: last one wins.
- Non-integer terminal dimensions should produce clap parse error.

### Reference Implementation
- Mirror flag surface from SPEC §2; similar to TS CLI set-up command.

**Related to Infrastructure (PARITY_APPROACH.md):**
- Theme 5 Marker pattern referenced by later issues; no markers here.
- Theme 8 Two-Phase substitution comes later.

## Definition of Done
- [ ] Flags and validations implemented and tested.
- [ ] Help text matches spec semantics.
- [ ] Not Implemented stub returns clear error until core exists.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§2, §3, §9)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.1)
- Data Structures: `docs/subcommand-specs/set-up/DATA-STRUCTURES.md`
- Parity Approach: `docs/PARITY_APPROACH.md` (Themes 1, 2, 6)
