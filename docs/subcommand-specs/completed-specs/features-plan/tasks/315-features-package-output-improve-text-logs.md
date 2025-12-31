---
number: 315
title: "[Features Package] [Output] Improve text logs for collection packaging"
author: pofallon
createdAt: 2025-10-13T23:44:33Z
updatedAt: 2025-10-13T23:44:33Z
labels:
  - type:enhancement
  - priority:medium
  - scope:small
  - subcommand:features-package
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: Output polish

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Enhance logs to distinguish between single and collection modes, and log each packaged feature with file name, size, and digest if available. Follow Theme 1 (JSON to stdout, human-readable logs to stderr).

## Specification Reference
**From SPEC.md Section:** §10 Output Specifications

**From GAP.md Section:** 6. OUTPUT ISSUES: Missing Collection Artifacts (logging)

### Expected Behavior
- Print "Packaging feature collection..." when collection mode detected.
- For each feature: "Created package: <id>.tgz (size: N bytes)".
- Print a summary count at end.

### Current Behavior
- Only logs for single feature.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/features.rs` — Add log lines under the right spans.

### 2. Cross-Cutting Concerns
- [ ] Theme 1: Ensure logs to stderr when `--json` is used for outer CLI (if applicable).

## Testing Requirements
- [ ] Integration test asserts presence of mode message and per-feature lines.

## Acceptance Criteria
- [ ] Clear, structured logs for collection mode.
- [ ] Tests pass; CI green.

## Dependencies
- Tracks: #310
- Blocked By: #312, #313
