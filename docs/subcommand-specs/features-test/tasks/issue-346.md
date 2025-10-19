# [features-test] Fix Docker unavailability handling and error policy

<!-- Labels: subcommand:features-test, type:bug, priority:critical -->
Tracks: #345

## Issue Type
- [x] Error Handling
- [x] Core Logic Implementation

## Description
When Docker is unavailable (or `docker run` fails), the current implementation logs and returns success. This silently passes tests, violating the Prime Directive (No Silent Fallbacks) and the spec's requirement to fail fast on system errors.

## Specification Reference
- From SPEC.md §9 Error Handling Strategy: "Docker unavailable or failing commands: surface stderr; exit non‑zero."
- From GAP.md §5.1 Missing Error Cases: "CRITICAL BUG: Returns success when Docker is unavailable."

### Expected Behavior
- Surface the Docker error and exit non-zero. Do not mark tests as passed.

### Current Behavior
- Returns `Ok(true)` after logging, causing false success.

## Implementation Requirements

### Code Changes Required
- Files to Modify:
  - `crates/deacon/src/commands/features.rs` — test execution path
- Specific Tasks:
  - Replace `Ok(true)` on docker error with an error return using `anyhow!` and `.context(...)`.
  - Ensure exit code propagates as non-zero from the command handler.
  - Add a user-facing error message: "Docker is required for feature testing."

### Validation Rules
- No silent fallbacks per `.github/copilot-instructions.md` Prime Directive 6.

## Testing Requirements
- Unit/Integration:
  - Simulate docker missing (e.g., PATH without docker) and assert non-zero exit and exact error message.
  - Negative test: failing container start returns error.
- Smoke Tests:
  - Adjust `smoke_basic.rs` to accept well-defined Docker-unavailable error for this subcommand.

## Acceptance Criteria
- Non-zero exit when Docker is unavailable.
- Error message matches: "Docker is required for feature testing." (with context for underlying error).
- All CI checks pass.

## References
- GAP.md §5.1
- PARITY_APPROACH.md — Prime Directive 6

Issue: https://github.com/get2knowio/deacon/issues/346
