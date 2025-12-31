# [features-test] Output: text formatting, JSON results contract, and smoke tests

<!-- Labels: subcommand:features-test, type:enhancement, priority:medium, testing -->
Tracks: #345

## Issue Type
- [x] Testing & Validation
- [x] Error Handling

## Description
Implement result aggregation and output per spec, including:
- Text mode banner, per-test lines with pass/fail, final summary, quiet mode.
- JSON mode returning an array of `{ testName, result }` objects to stdout; logs to stderr.
- Smoke tests to enforce contract.

## Specification Reference
- SPEC.md §10 Output Specifications; DATA-STRUCTURES.md
- GAP.md §4 Output and Results Gaps
- PARITY_APPROACH.md Theme 1 (JSON Output Contract)

## Implementation Requirements
- Define output writer in `crates/core` and call from command.
- Ensure no trailing newline/whitespace in JSON; stderr for logs.

## Testing Requirements
- Unit: JSON serialization matches schema exactly.
- Integration: run sample tests and verify text and JSON outputs.
- Smoke: extend `crates/deacon/tests/smoke_basic.rs`.

## Acceptance Criteria
- Output matches spec; CI green.

Issue: https://github.com/get2knowio/deacon/issues/355
