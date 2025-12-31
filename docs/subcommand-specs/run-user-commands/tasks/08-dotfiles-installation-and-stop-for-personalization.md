---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: dotfiles"]
---

# [run-user-commands] Implement dotfiles installation and stop-for-personalization

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement dotfiles installation inside the container when configured via flags. Support common installer script names and the explicit install command flag. Implement early exit when `--stop-for-personalization` is provided, returning result `"stopForPersonalization"`.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (dotfiles and early exit)

**From GAP.md Section:** 3.3 Dotfiles Installation (missing), 3.6 WaitFor and Early Exit

### Expected Behavior
- Clone from `--dotfiles-repository` (expand `owner/repo` to GitHub URL) or use local path.
- Install using `--dotfiles-install-command` or auto-detect common names (install.sh, bootstrap, setup, script/*).
- On success and `--stop-for-personalization`, return `"stopForPersonalization"` without running later hooks.

### Current Behavior
- Dotfiles not implemented; stop-for-personalization flag ignored.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/dotfiles.rs` – Extend for in-container execution.
- `crates/core/src/run_user_commands/dotfiles.rs` – New glue and early-exit signaling.

#### Specific Tasks
- [ ] Repo URL expansion and clone.
- [ ] Installer detection and execution.
- [ ] Early exit result mapping.

### 2. Data Structures
- Reuse dotfiles config fields from `RunUserCommandsArgs`.

### 3. Validation Rules
- [ ] If clone or install fails, return JSON error and exit 1.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Item 6 - Dotfiles Installation Workflow

## Testing Requirements

### Unit Tests
- [ ] Installer detection precedence.

### Integration Tests
- [ ] Install from a simple repo; verify early exit when requested.

## Acceptance Criteria
- [ ] Dotfiles workflow implemented; early exit works.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§5)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§3.3, §3.6)
