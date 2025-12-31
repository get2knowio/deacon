---
number: 312
title: "[Features Package] [Core] Mode detection: single vs collection"
author: pofallon
createdAt: 2025-10-13T23:44:11Z
updatedAt: 2025-10-13T23:44:11Z
labels:
  - type:enhancement
  - priority:high
  - scope:medium
  - subcommand:features-package
---

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: _n/a_

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Implement robust detection of packaging mode. If `target/devcontainer-feature.json` exists, run single-feature packaging. Else, if `target/src/` exists, run collection packaging over `src/*`. Error clearly if neither condition holds.

## Specification Reference
**From SPEC.md Section:** §4 Configuration Resolution, §5 Core Execution Logic

**From GAP.md Section:** 1. CRITICAL: Collection Mode Not Implemented

### Expected Behavior
- Single-mode when `devcontainer-feature.json` is present in `target`.
- Collection-mode when `src/` is present under `target`.
- Error when neither present: "No feature found at target. Expected devcontainer-feature.json or src/."

### Current Behavior
- Always assumes single-feature mode.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Introduce `PackagingMode` enum and detection function.

#### Specific Tasks
- [ ] Add `enum PackagingMode { Single, Collection }`.
- [ ] Implement `fn detect_mode(target: &Path) -> Result<PackagingMode>`.
- [ ] Wire detection into command execution path.
- [ ] Add tracing logs indicating detected mode.

### 2. Data Structures
_No new persisted structures._

### 3. Validation Rules
- [ ] Error if both `devcontainer-feature.json` and `src/` missing.
- [ ] Explicit error message (Theme 6).

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages.
- [ ] Theme 1 - Logs to stderr; if any JSON summary later, ensure stdout separation.

## Testing Requirements
- [ ] Unit tests for `detect_mode` across: single, collection, neither.
- [ ] Integration test calling CLI against sample fixtures.

## Acceptance Criteria
- [ ] Mode detection implemented and covered by tests.
- [ ] Error messages match spec.
- [ ] All CI checks pass.

## Dependencies
- Tracks: #310
- Blocked By: None
