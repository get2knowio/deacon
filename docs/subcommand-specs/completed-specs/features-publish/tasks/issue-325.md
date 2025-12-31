# [features publish] Idempotency: skip existing versions/tags with warnings

https://github.com/get2knowio/deacon/issues/325

<!-- Labels: subcommand:features-publish, type:enhancement, priority:high, scope:medium -->

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Implement idempotent behavior: when attempting to publish a feature with a version whose semantic tags already exist, the command should skip publishing and emit a warning, exiting successfully.

## Specification Reference
**From SPEC.md Section:** §6. State Management – Idempotency

**From GAP.md Section:** 2. Core Execution Logic Gaps – Idempotency and Version Existence Checks

### Expected Behavior
```pseudocode
if get_manifest_digest(ref@X.Y.Z) exists:
  warn("Version already exists; skipping")
  continue
```

### Current Behavior
- Always attempts to publish, leading to errors on re-run

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Pre-check existence before publish
- `crates/core/src/oci.rs` — Use `get_manifest_digest` (see #324)

#### Specific Tasks
- [ ] Check `X.Y.Z` tag existence for each feature prior to upload
- [ ] Warn and skip when present (without treating as error)
- [ ] Continue to evaluate and publish missing semantic tags (if mixed state)

### 2. Data Structures
N/A

### 3. Validation Rules
N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output (reflect skipped state appropriately)
- [ ] Theme 6 - Error Messages (consistent warning text)

## Testing Requirements

### Unit Tests
- [ ] Behavior when exact version exists
- [ ] Mixed state: some tags exist, some missing

### Integration Tests
- [ ] Re-publish same version: skip, exit 0

## Acceptance Criteria
- [ ] Idempotent behavior implemented
- [ ] Logs warn and JSON reflects no new `publishedTags` for existing versions
- [ ] CI checks pass

## Dependencies

**Blocked By:** #324

**Related:** #323
