# [features publish] Packaging integration polish: reuse existing artifacts when present

https://github.com/get2knowio/deacon/issues/332

<!-- Labels: subcommand:features-publish, type:enhancement, priority:low, scope:small -->

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Optimize publish flow to reuse existing artifacts from `features package` when available, rather than always re-packaging into a temp directory.

## Specification Reference
**From SPEC.md Section:** §5. Core Execution Logic – Ensure artifacts exist (may reuse)

**From GAP.md Section:** 2. Core Execution Logic Gaps – Automatic Packaging Integration (partial)

### Expected Behavior
- If packaged output exists in a conventional location, reuse it
- Otherwise package into temp dir

### Current Behavior
- Always packages into temp dir

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Detect existing packaged output

#### Specific Tasks
- [ ] Check for `devcontainer-collection.json` and tarballs in output dir
- [ ] Reuse when present; fall back to packaging

### 2. Data Structures
N/A

### 3. Validation Rules
N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - Ensure output unaffected; logs make behavior clear

## Testing Requirements

### Integration Tests
- [ ] Scenario with pre-existing package

## Acceptance Criteria
- [ ] Reuse implemented without breaking flow
- [ ] CI passes

## Dependencies

**Related:** #325
