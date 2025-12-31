# [up] Implement dotfiles flags and installation workflow

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation

## Description
Add support for dotfiles configuration flags and integrate the dotfiles installation workflow into `setupInContainer`. Clone the repository, detect and run install script or custom command, and honor target path. Ensure idempotency using a marker file.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (setupInContainer)
- From GAP.md Section: §16 Dotfiles Support – Completely Missing
- From PARITY_APPROACH.md: Infrastructure Item 6 – Dotfiles Installation Workflow

### Expected Behavior
- Flags: `--dotfiles-repository`, `--dotfiles-install-command`, `--dotfiles-target-path` (default `~/dotfiles`).
- Installation during setup; skipped when `--skip-post-create` is set (as per SPEC note under lifecycle control on skip behavior).
- Idempotent via marker file.

### Current Behavior
- No dotfiles support; core has `core/src/dotfiles.rs` utilities available.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add dotfiles flags.
- `crates/deacon/src/commands/up.rs` - call into `core::dotfiles` during setupInContainer, with redaction and logging.
- `crates/core/src/dotfiles.rs` - reuse existing utilities; extend if needed to support marker path.

### 2. Data Structures
```rust
// DotfilesConfiguration { repository?: string, installCommand?: string, targetPath?: string }
```

### 3. Validation Rules
- [ ] Validate repository URL format (basic) and target path accessibility.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [x] Infrastructure Item 6 - Dotfiles workflow.
- [ ] Theme 5 - Marker File Idempotency Pattern.

## Testing Requirements
- Unit: install script detection, command invocation mapping.
- Integration: end-to-end dotfiles install with a sample repo; idempotent rerun respects marker.

## Acceptance Criteria
- Flags work; workflow installs dotfiles; idempotent; tests pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§5)
- `docs/subcommand-specs/up/GAP.md` (§16)
- `docs/PARITY_APPROACH.md` (Item 6)
