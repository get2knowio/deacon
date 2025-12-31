# [features publish] Validate semantic version and standardized errors

https://github.com/get2knowio/deacon/issues/328

<!-- Labels: subcommand:features-publish, type:enhancement, priority:medium, scope:small -->

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Validate that the feature's `version` in `devcontainer-feature.json` is a valid semantic version before any publish operations, and emit standardized error messages per spec.

## Specification Reference
**From SPEC.md Section:** §9. Error Handling Strategy – Invalid semantic version

**From GAP.md Section:** 5. Error Handling Gaps – Missing semantic version validation

### Expected Behavior
- On invalid version, exit 1 with error: "Invalid semantic version: <detail>."

### Current Behavior
- No pre-validation; would fail downstream

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Validate version via `semver` crate before processing

#### Specific Tasks
- [ ] Parse and validate using `semver::Version::parse`
- [ ] Return standardized error on failure (Theme 6)

### 2. Data Structures
N/A

### 3. Validation Rules
- [ ] Error message exactness per Theme 6

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Message Standardization
- [ ] Theme 4 - Semantic Versioning

## Testing Requirements

### Unit Tests
- [ ] Valid versions accepted
- [ ] Invalid versions rejected with exact message

### Integration Tests
- [ ] End-to-end run fails early on invalid version

## Acceptance Criteria
- [ ] Validation added and tested
- [ ] CI passes

## Dependencies

**Blocks:** #323

**Related:** #324
