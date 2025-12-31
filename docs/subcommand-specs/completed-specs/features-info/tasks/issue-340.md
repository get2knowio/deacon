# [features info] JSON-mode error handling: `{}` + exit 1 and standardized messages

https://github.com/get2knowio/deacon/issues/340

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Standardize error handling for JSON output per spec: when `--output-format json` is selected, any error should print `{}` to stdout and exit with status code 1. For text output, print actionable error messages with sentence case and a trailing period.

## Specification Reference
- From SPEC.md Section: §9. Error Handling Strategy
- From GAP.md Section: 3.5 Error Handling for JSON Mode

### Expected Behavior
- JSON mode: `{}` on error; no extra whitespace; exit 1.
- Text mode: `Error: ...` with actionable sentence-case message.

### Current Behavior
- Inconsistent; placeholders do not implement this behavior.

## Implementation Requirements
- [ ] Shared helper `handle_json_or_text_error(format, message)` in `crates/deacon` or `crates/core`
- [ ] Replace ad-hoc error prints in features info with helper
- [ ] Ensure exit codes are correct across all modes

## Testing Requirements
- Unit Tests:
  - [ ] Helper function behavior
- Integration Tests:
  - [ ] Manifest not found -> `{}` in JSON
  - [ ] No tags -> `{}` in JSON

## Acceptance Criteria
- [ ] All error paths comply with Theme 1 and Theme 6
- [ ] CI checks pass

## Dependencies
Blocked By: None
