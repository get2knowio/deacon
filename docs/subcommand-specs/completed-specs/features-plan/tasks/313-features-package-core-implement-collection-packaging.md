---
number: 313
title: "[Features Package] [Core] Implement collection packaging over `src/*`"
author: pofallon
createdAt: 2025-10-13T23:44:18Z
updatedAt: 2025-10-13T23:44:18Z
labels:
  - type:enhancement
  - priority:high
  - scope:large
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
Add support for collection packaging. Iterate over `target/src/*/` subfolders, validate each contains a valid feature (`devcontainer-feature.json`), produce one `.tgz` per feature in the output folder, and accumulate metadata for collection summary.

## Specification Reference
**From SPEC.md Section:** §5 Core Execution Logic, §7 External System Interactions

**From GAP.md Section:** 1. CRITICAL: Collection Mode Not Implemented

### Expected Behavior
- For each `src/<featureId>`: create `<featureId>.tgz` tarball in output folder.
- Skip hidden/system entries; error on invalid feature with clear message.
- Return list of metadata objects representing packaged features.

### Current Behavior
- Only single-feature packaging is implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Implement `package_feature_collection` and helpers.
- `crates/core/src/...` — Consider extracting reusable tar/metadata functions if appropriate.

#### Specific Tasks
- [ ] Implement folder scan and validation for `src/*`.
- [ ] Package each valid feature; collect `{ id, version, name, description, options, installsAfter, dependsOn }`.
- [ ] Structured logging for progress per feature.
- [ ] Handle empty collection with clear error.

### 2. Data Structures
- Use structures from DATA-STRUCTURES.md for collection entries.

### 3. Validation Rules
- [ ] Error on missing or invalid `devcontainer-feature.json` in any subfolder.
- [ ] Error on empty `src/` (no valid features).

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - Correct log stream separation.
- [ ] Theme 6 - Standardized error messages.

## Testing Requirements
- [ ] Integration test with fixture `src/a`, `src/b`.
- [ ] Unit test for directory iteration and filtering.
- [ ] Error test for empty collection.

## Acceptance Criteria
- [ ] Collection packaging implemented and validated by tests.
- [ ] Produces one `.tgz` per feature and metadata list.
- [ ] CI checks pass (build, fmt, clippy, tests).

## Dependencies
- Tracks: #310
- Blocked By: #312 (Mode detection)
