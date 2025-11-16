---
number: 320
title: "[Features Package] [Validation] Robust errors: invalid feature folders and empty collections"
author: pofallon
createdAt: 2025-10-13T23:45:06Z
updatedAt: 2025-10-13T23:45:06Z
labels:
  - priority:medium
  - scope:small
  - subcommand:features-package
  - type:bug
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Improve validation for collection mode: emit clear, spec-compliant errors when a subfolder under `src/` lacks a `devcontainer-feature.json` or when no valid features are found.

## Specification Reference
**From SPEC.md Section:** §9 Error Handling Strategy

**From GAP.md Section:** 1. Collection Mode not implemented (includes validation gaps)

## Implementation Requirements
- [ ] Validate each `src/*` entry and surface which folder failed.
- [ ] Error when collection is empty: "No features found in src/."
- [ ] Add tests asserting exact messages.

## Acceptance Criteria
- [ ] Improved errors implemented and covered by tests.
- [ ] CI green.

## Dependencies
- Tracks: #310
- Blocked By: #313
