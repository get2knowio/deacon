---
number: 318
title: "[Features Package] [Polish] JSON output contract and error message standardization"
author: pofallon
createdAt: 2025-10-13T23:44:56Z
updatedAt: 2025-10-13T23:44:56Z
labels:
  - type:enhancement
  - scope:small
  - priority:low
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
Ensure compliance with Consistency Theme 1 and 6. If the subcommand emits any JSON output (summary), ensure it is sent to stdout, logs go to stderr, and the structure matches DATA-STRUCTURES.md or an agreed summary. Standardize error messages to match SPEC wording exactly.

## Specification Reference
**From SPEC.md Section:** §10 Output Specifications

**From GAP.md Section:** Notes on output issues and message formats.

## Implementation Requirements
- [ ] Audit command for stdout/stderr separation.
- [ ] Update any error strings to match spec format.
- [ ] Add tests asserting correct streams and error messages.

## Acceptance Criteria
- [ ] Output contract verified by tests.
- [ ] Error messages standardized.
- [ ] CI green.

## Dependencies
- Tracks: #310
- Blocked By: #314, #316
