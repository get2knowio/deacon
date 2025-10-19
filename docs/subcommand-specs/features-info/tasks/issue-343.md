# [features info] Refactor: separate data fetching and output formatting; introduce OutputFormat enum

https://github.com/get2knowio/deacon/issues/343

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Refactor `features info` implementation to separate data fetching from output formatting and introduce a type-safe `OutputFormat` enum instead of boolean flags. This supports clean verbose composition and standardized JSON outputs.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic
- From GAP.md Section: 7. Code Refactoring Recommendations

### Expected Behavior
- Functions like `fetch_manifest_data`, `fetch_tags_data`, `format_manifest_text/json`, etc.
- `OutputFormat { Text, Json }` used across code paths.

### Current Behavior
- Mixed concerns, boolean `json` parameter passed around.

## Implementation Requirements
- [ ] Create `OutputFormat` enum and use clap `ValueEnum`
- [ ] Extract fetchers and formatters into separate functions/modules
- [ ] Update command entry to orchestrate calling these

## Testing Requirements
- [ ] Unit tests for new helpers
- [ ] Ensure no behavior regressions

## Acceptance Criteria
- [ ] Refactor complete, code more testable
- [ ] CI passes

## Dependencies
Blocked By: #334 (flag definition), and pairs with #338 (formatter)
