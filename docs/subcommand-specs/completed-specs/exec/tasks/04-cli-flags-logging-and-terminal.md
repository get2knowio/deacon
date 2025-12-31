---
subcommand: exec
type: enhancement
priority: medium
scope: small
---

# [exec] Implement CLI Flags: Logging & Terminal Dimensions

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation
- [ ] Core Logic Implementation

## Description
Add `--log-level`, `--log-format`, `--terminal-columns`, and `--terminal-rows` for `exec`. Enforce paired requirement between rows/columns and wire `--log-format json` to influence PTY selection (covered in core task 08).

## Specification Reference
- From SPEC.md Section: §2 CLI, §5 Core Execution Logic, §10 Output Specifications
- From GAP.md Section: 1. CLI Interface Gaps, 7. PTY/TTY Handling Gaps

### Expected Behavior
- `--log-level {info|debug|trace}` (default `info`)
- `--log-format {text|json}` (default `text`)
- `--terminal-columns <N>` requires `--terminal-rows <N>` and vice versa; error message: "terminal-columns requires terminal-rows and vice versa"

### Current Behavior
- Logging flags exist globally; terminal dimensions missing; JSON format does not affect PTY.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/exec.rs` — Add terminal flags; ensure validation.
- `crates/deacon/src/validation.rs` — Helper for paired requirement.

### 2. Data Structures
```rust
pub struct ParsedInput {
    pub log_level: Option<String>,
    pub log_format: Option<String>,
    pub term_cols: Option<u16>,
    pub term_rows: Option<u16>,
}
```

### 3. Validation Rules
- [ ] Paired requirement: columns <-> rows, exact error text.

### 4. Cross-Cutting Concerns
- [ ] Theme 2 - CLI Validation
- [ ] Theme 6 - Error Messages
- [ ] Theme 1 - JSON Output Contract (for logs)

## Testing Requirements
- [ ] Unit: paired requirement validation error.
- [ ] Integration: parsed dims propagate to exec options (covered in task 08).

## Acceptance Criteria
- [ ] Flags present and validated; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§2, §5, §10)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§1, §7)
