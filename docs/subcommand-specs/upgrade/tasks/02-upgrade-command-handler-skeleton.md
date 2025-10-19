---
subcommand: upgrade
type: enhancement
priority: high
scope: medium
---

# [upgrade] Command Handler Skeleton and Args Struct

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Create the `upgrade` command module and a minimal `execute_upgrade` entrypoint that accepts parsed args, initializes logging, resolves the workspace and config path, and returns success. No lockfile or feature logic yet—this establishes the wiring used by later tasks.

## Specification Reference

**From SPEC.md Section:** §5. Core Execution Logic (Phase 1: Initialization)

**From GAP.md Section:** 2.1 Command Handler

### Expected Behavior
- A new `commands/upgrade.rs` exposing `execute_upgrade(args).await` and `UpgradeArgs`.
- Initializes logger per `--log-level` and uses global CLI patterns.
- Resolves `workspace_folder` absolute path and derives config path (but does not read config yet).

### Current Behavior
- No file or function exists for the upgrade command.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/deacon/src/commands/mod.rs`
  - Export `upgrade` module
- `crates/deacon/src/commands/upgrade.rs` (new)
  - Define `UpgradeArgs` to mirror parsed CLI fields relevant to execution
  - Define `pub async fn execute_upgrade(args: UpgradeArgs) -> anyhow::Result<()>`
  - Initialize tracing logger and basic spans
- `crates/deacon/src/cli.rs`
  - In dispatch, call `execute_upgrade` with converted args

#### Specific Tasks
- [ ] Create module and function signatures
- [ ] Map clap args to `UpgradeArgs`
- [ ] Initialize logging based on `--log-level`
- [ ] Add minimal tracing spans (e.g., `upgrade.start`)

### 2. Data Structures

From DATA-STRUCTURES.md:
```rust
pub struct UpgradeArgs {
    pub workspace_folder: std::path::PathBuf,
    pub config_file: Option<std::path::PathBuf>,
    pub docker_path: String,
    pub docker_compose_path: String,
    pub log_level: String,
    pub dry_run: bool,
    pub feature: Option<String>,
    pub target_version: Option<String>,
}
```

### 3. Validation Rules
- N/A in this task; validation handled by CLI layer in Task 01.

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 6 - Error Messages: ensure any early errors use standardized messages
- [ ] Logging orthodoxy: use `tracing` spans for initialization

## Testing Requirements

### Unit Tests
- [ ] Build-only compilation test for new module

### Integration Tests
- [ ] None yet

### Smoke Tests
- [ ] Ensure `upgrade --help` runs and dispatch code path compiles

### Examples
- [ ] None

## Acceptance Criteria
- [ ] Module exists with `execute_upgrade` and `UpgradeArgs`
- [ ] CLI dispatch compiles and runs to a no-op success
- [ ] Logging initializes without panics; spans created
- [ ] CI checks pass (build, test, fmt, clippy)

## Implementation Notes
- Mirror patterns in `up`/`build` for args mapping and tracing spans.

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§5)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§2.1)
