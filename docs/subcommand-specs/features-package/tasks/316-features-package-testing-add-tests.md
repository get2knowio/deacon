---
number: 316
title: "[Features Package] [Testing] Add tests: collection mode, force-clean, default target"
author: pofallon
createdAt: 2025-10-13T23:44:38Z
updatedAt: 2025-10-13T23:44:38Z
labels:
  - priority:high
  - scope:medium
  - subcommand:features-package
  - type:test
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Add comprehensive tests to cover collection packaging, devcontainer-collection.json generation, behavior of `--force-clean-output-folder`, and default target path `.`. Ensure smoke tests are updated if CLI UX changes.

## Specification Reference
**From SPEC.md Section:** §15 Testing Strategy

**From GAP.md Section:** 7. TEST COVERAGE GAPS

### Expected Behavior
- Tests validate presence and structure of output artifacts.
- Tests verify force-clean removes stale files.
- Tests verify default target works.

### Current Behavior
- Single-feature tests exist; collection tests missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/tests/test_features_cli.rs` — Add new tests.
- `fixtures/` — Add collection fixture with `src/a`, `src/b`.

### 2. Cross-Cutting Concerns
- [ ] Theme 1 - JSON vs text output separation (ensure tests check correct streams when relevant).
- [ ] Theme 6 - Error Messages (assert exact text for errors).

## Acceptance Criteria
- [ ] New tests added and passing.
- [ ] Smoke tests updated if CLI output changes.
- [ ] CI green.

## Dependencies
- Tracks: #310
- Blocked By: #313, #314
