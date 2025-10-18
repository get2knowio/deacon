---
number: 319
title: "[Features Package] [Refactor] Extract helpers for metadata and tar packaging"
author: pofallon
createdAt: 2025-10-13T23:45:03Z
updatedAt: 2025-10-13T23:45:03Z
labels:
  - type:refactor
  - priority:medium
  - scope:small
  - subcommand:features-package
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Other: Refactor

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Extract reusable helper functions for: (a) reading/parsing `devcontainer-feature.json` into the metadata struct, and (b) creating a `.tgz` from a feature directory. This makes single and collection paths share code and eases testing.

## Specification Reference
**From SPEC.md Section:** §5 Core Execution Logic (implementation detail)

**From GAP.md Section:** Suggested plan item.

## Implementation Requirements
- [ ] Introduce helpers in `crates/core/` or a shared module.
- [ ] Update single and collection packaging to use them.
- [ ] Add unit tests for helpers.

## Acceptance Criteria
- [ ] DRY implementation with covered helpers.
- [ ] CI green.

## Dependencies
- Tracks: #310
- Blocked By: #312, #313
