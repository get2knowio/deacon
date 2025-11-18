---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Implement CLI Flags: Execution Environment & Probe Defaults

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling

## Description
Introduce `--remote-env` (repeatable, value may be empty) and `--default-user-env-probe` to control environment injection and the probe mode when the config does not set `userEnvProbe`. Ensure regex validation and exact error messages.

## Specification Reference
- From SPEC.md Section: §2 Command-Line Interface, §3 Input Processing Pipeline, §5 Core Execution Logic
- From GAP.md Section: 1. CLI Interface Gaps (remote-env, default-user-env-probe), 5. Environment Handling Gaps

### Expected Behavior
- `--remote-env <name=value>`: repeatable; regex `/.+=.*/` (value may be empty).
- `--default-user-env-probe {none|loginInteractiveShell|interactiveShell|loginShell}`: default `loginInteractiveShell`.
- Validation message: `remote-env must match <name>=<value>` for invalid entries.

### Current Behavior
- Only `--env` exists with different semantics; empty values unsupported.
- No `--default-user-env-probe` flag.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/exec.rs` — Add flags and parsing.
- `crates/deacon/src/validation.rs` — Add validation helpers for `remote-env` allowing empty value.
- Map parsed values into `ParsedInput.remote_env_kv` and `ParsedInput.default_user_env_probe`.

### 2. Data Structures
```rust
pub struct ParsedInput {
    pub default_user_env_probe: Option<String>, // enum string
    pub remote_env_kv: Vec<String>,             // list of "name=value" (value may be empty)
}
```

### 3. Validation Rules
- [ ] `--remote-env` must match `/.+=.*/`.
- [ ] Allow empty value (e.g., `--remote-env FOO=`).
- [ ] Error message: "remote-env must match <name>=<value>".

### 4. Cross-Cutting Concerns
- [ ] Theme 2 - CLI Validation
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Accept `--remote-env FOO=BAR` and `--remote-env BAZ=`.
- [ ] Reject malformed entries (`FOO`, `=BAR`).
- [ ] Parse `--default-user-env-probe` for all enum values.

### Integration Tests
- [ ] Ensure parsed values flow to the environment merge step (stub for now; full in core logic tasks).

### Smoke Tests
- [ ] Add a smoke test covering `--remote-env BAZ=` acceptance.

### Examples
- [ ] Update example to show empty env value semantics.

## Acceptance Criteria
- [ ] Flags implemented with correct help text and enum values.
- [ ] Validation rules enforced; exact error messages.
- [ ] Tests pass and CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§2–§3, §5)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§1, §5)
- DATA-STRUCTURES: `docs/subcommand-specs/exec/DATA-STRUCTURES.md`
