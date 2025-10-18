---
number: 311
title: "[Features Package] [CLI] Add `--force-clean-output-folder` and default `target`"
author: pofallon
createdAt: 2025-10-13T23:43:57Z
updatedAt: 2025-10-13T23:43:57Z
labels:
  - type:enhancement
  - priority:high
  - scope:small
  - subcommand:features-package
---

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: _n/a_

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Add the `--force-clean-output-folder, -f` boolean flag and make the positional `target` default to `.`. These are required for spec parity and improve ergonomics and reproducibility.

## Specification Reference

**From SPEC.md Section:** §2 Command-Line Interface

**From GAP.md Section:** 3. MISSING: `--force-clean-output-folder` Flag; 4. MISSING: Positional `target` Argument

### Expected Behavior
- `deacon features package` uses `.` as the default target path.
- `--force-clean-output-folder` (or `-f`) removes any existing content under `--output-folder` before packaging.

### Current Behavior
- `path` is required; no default `.`.
- No `--force-clean-output-folder` flag exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` — Add flag/positional defaults.
- `crates/deacon/src/commands/features.rs` — Honor `force_clean` in execution.

#### Specific Tasks
- [ ] Add `-f, --force-clean-output-folder` boolean to the `features package` args.
- [ ] Change positional `target` to default to `.` when omitted.
- [ ] Thread new args into command handler.

### 2. Data Structures
_No new persistent structures._

### 3. Validation Rules
- [ ] Ensure output folder exists (create if missing) after optional cleanup.
- [ ] Error if cleanup fails (permission issues).
- [ ] Error message format per Theme 6.

## Testing Requirements

### Unit/Integration Tests
- [ ] Test default target `.` behavior (works with valid single-feature dir).
- [ ] Force-clean removes pre-populated files in output folder before packaging.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` if CLI behavior changed.

## Acceptance Criteria
- [ ] Flag and default positional implemented and wired.
- [ ] Tests pass and cover new behavior.
- [ ] All CI checks pass (build, test, fmt, clippy).

## Dependencies

**Tracks:** #310

**Blocked By:** None
