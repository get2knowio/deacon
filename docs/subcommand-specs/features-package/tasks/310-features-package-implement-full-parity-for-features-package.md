---
number: 310
title: "[Features Package] Implement Full Parity for `features package`"
author: pofallon
createdAt: 2025-10-13T23:43:48Z
updatedAt: 2025-10-13T23:45:15Z
labels:
  - tracking
  - type:enhancement
  - priority:high
  - scope:medium
  - subcommand:features-package
---

## Overview
`features package` creates distributable archives for one feature or an entire collection and writes devcontainer-collection.json metadata. Current implementation supports only single-feature packaging and lacks collection mode, collection metadata, and some CLI flags.

**Current Completeness:** ~35% (from GAP.md Executive Summary)

**Specification Location:** `docs/subcommand-specs/features-package/`

## Implementation Status

### ✅ Already Implemented
- Basic single-feature packaging (create .tgz for a feature directory)
- Basic error handling for invalid `devcontainer-feature.json`
- Text-mode logging for single feature path

### ❌ Missing Functionality
- Collection mode (package multiple features from `src/`)
- `devcontainer-collection.json` generation (both modes)
- `--force-clean-output-folder` flag behavior
- Default positional `target` (defaults to `.`)
- Output logs and summary for collection mode
- Comprehensive tests for collection and force-clean

## Implementation Plan

This tracking issue coordinates the following implementation issues:

- [ ] #311 - [CLI] Add `--force-clean-output-folder` and default `target`
- [ ] #312 - [Core] Mode detection: single vs collection
- [ ] #313 - [Core] Implement collection packaging over `src/*`
- [ ] #314 - [Core] Generate `devcontainer-collection.json` (both modes)
- [ ] #315 - [Output] Improve text logs for collection packaging
- [ ] #316 - [Testing] Add unit/integration tests for collection & force-clean
- [ ] #317 - [Docs] Update examples and README entries
- [ ] #318 - [Polish] JSON output contract (if any) and error messages
- [ ] #319 - [Refactor] Small helpers: metadata extraction, tar utility
- [ ] #320 - [Validation] Robust errors: invalid folders and empty collections

## Cross-Cutting Concerns

This implementation must address:

- [ ] Consistency Theme 1: JSON Output Contract Compliance (stdout vs stderr; if emitting JSON summary)
- [ ] Consistency Theme 2: CLI Validation Rules (positional default, flag presence)
- [ ] Consistency Theme 3: Collection Mode vs Single Mode (always write devcontainer-collection.json)
- [ ] Infrastructure Item X: None required for this subcommand

## Quality Gates

Before closing this tracking issue, ALL sub-issues must pass:

- [ ] All CI checks pass (build, test, fmt, clippy)
- [ ] JSON output matches DATA-STRUCTURES.md exactly (if enabled)
- [ ] All specification validation rules enforced
- [ ] Smoke tests updated in `crates/deacon/tests/smoke_basic.rs`
- [ ] Examples updated in `examples/feature-management/`
- [ ] Gap analysis updated with "COMPLETED" markers

## References

- Specification: `docs/subcommand-specs/features-package/SPEC.md`
- Gap Analysis: `docs/subcommand-specs/features-package/GAP.md`
- Data Structures: `docs/subcommand-specs/features-package/DATA-STRUCTURES.md`
- Parity Approach: `docs/PARITY_APPROACH.md`
