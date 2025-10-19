# [features-test] Implement project collection structure and discovery

<!-- Labels: subcommand:features-test, type:enhancement, priority:high -->
Tracks: #345

## Issue Type
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Implement collection-aware layout and discovery. The CLI must accept `--project-folder` (default `.`) and validate presence of `src/` and `test/` subdirectories. Discover features from `src/<id>/` and map to tests in `test/<id>/`.

## Specification Reference
- From SPEC.md §2 CLI and §3 Input Processing Pipeline
- From GAP.md §2 Test Discovery and Structure Gaps

### Expected Behavior
- Validate project folder contains `src/` and `test/`.
- When `--features` is omitted, test all features under `src/`.
- When `--features` is provided, validate each `<id>` exists under `test/`.

## Implementation Requirements

### Code Changes Required
- Files to Modify:
  - `crates/deacon/src/cli.rs` — add flag `--project-folder/-p`.
  - `crates/deacon/src/commands/features.rs` — implement discovery and validation logic.
- Specific Tasks:
  - Add structure validation with precise error messages.
  - Implement feature enumeration from `src/`.
  - Wire through to subsequent test execution phases.

### Validation Rules
- Error if either `src/` or `test/` is missing: "Project folder must contain both 'src' and 'test' directories."
- Error if a requested feature ID has no corresponding `test/<id>` directory.

## Testing Requirements
- Unit: discovery with empty folders, missing folders, single/multiple features.
- Integration: run against `fixtures/features/...` to verify enumeration.
- Smoke: add minimal collection under `examples/features/minimal-feature/` and run `features test`.

## Acceptance Criteria
- Discovery works and passes tests.
- Errors match spec messages.
- CI green.

## References
- DATA-STRUCTURES.md (result arrays)
- DIAGRAMS.md (sequences for prepare and run)

Issue: https://github.com/get2knowio/deacon/issues/347
