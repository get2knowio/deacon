# [up] Implement workspace identification flags and validation

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation

## Description
Add support for container identification via `--id-label` and workspace mounting behavior `--mount-workspace-git-root`, plus terminal dimension flags. Enforce required validation rules ensuring either workspace folder or id labels are supplied and ensure terminal dimension flags imply each other.

## Specification Reference

**From SPEC.md Section:** §2. Command-Line Interface; §3. Input Processing Pipeline; §4. Configuration Resolution

**From GAP.md Section:** §1. CLI Flags – Missing (`--id-label`, `--mount-workspace-git-root`, `--terminal-columns`, `--terminal-rows`); §1 Validation Rules Not Enforced (1, 2, 5)

### Expected Behavior
- Accept repeatable `--id-label <name=value>` and validate `/.+=.+/`.
- Accept `--mount-workspace-git-root` boolean (default true).
- Accept `--terminal-columns <n>` and `--terminal-rows <n>` with mutual implication.
- Validation: at least one of `--workspace-folder` or `--id-label`; at least one of `--workspace-folder` or `--override-config`.

### Current Behavior
- `--workspace-folder`, `--config`, `--override-config` exist.
- `--id-label`, `--mount-workspace-git-root`, terminal dims missing. Validation rules not enforced.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` - add flags with help text, defaults, validation (clap validators, conflicts/requires).
- `crates/deacon/src/commands/up.rs` - extend `UpArgs` to carry new fields; enforce validation early.
- `crates/core/src/container.rs` - reuse `parse_id_labels` helpers already present to validate/normalize.

#### Specific Tasks
- [ ] Add `--id-label <name=value>` repeatable with regex validation.
- [ ] Add `--mount-workspace-git-root` (default true).
- [ ] Add `--terminal-columns`, `--terminal-rows` with `requires` each other and numeric parsing.
- [ ] Enforce paired requirement rules (1) and (2) before expensive operations; emit exact messages per SPEC.

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
// ParsedInput.providedIdLabels: string[]
// ProvisionOptions.mountWorkspaceGitRoot: bool
// ProvisionOptions.terminalDimensions: { columns: number, rows: number }
```

### 3. Validation Rules
- [ ] `--id-label` must match `/.+=.+/`.
- [ ] At least one of `--workspace-folder` or `--id-label`.
- [ ] At least one of `--workspace-folder` or `--override-config`.
- [ ] Terminal dims imply one another.
- [ ] Error messages per SPEC §2 Validation Rules.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 2 - CLI Validation: conflicts/requires/regex.
- [x] Theme 6 - Error Messages: exact text.

## Testing Requirements

### Unit Tests
- [ ] `id-label` parsing valid/invalid.
- [ ] Terminal dims pairing logic.
- [ ] Validation rule enforcement and messages.

### Integration Tests
- [ ] `up` with only `--id-label` (no workspace) resolves container when present.
- [ ] Error when neither `--workspace-folder` nor `--id-label` provided.

### Smoke Tests
- [ ] Adjust smoke to include a minimal `--id-label` scenario.

### Examples
- [ ] Update `examples/cli/custom-container-name/` or add a simple label example.

## Acceptance Criteria
- [ ] Flags present with correct help and defaults.
- [ ] Validation rules enforced before runtime ops.
- [ ] CI checks pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2, §3)
- `docs/subcommand-specs/up/GAP.md` (§1)
- `docs/PARITY_APPROACH.md` (Theme 2, 6)
