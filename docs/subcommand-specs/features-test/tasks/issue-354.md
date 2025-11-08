# [features-test] Duplicate/idempotence testing

<!-- Labels: subcommand:features-test, type:enhancement, priority:medium -->
Tracks: #345, depends on: #351

## Issue Type
- [x] Core Logic Implementation

## Description
Add duplicate install test: run feature install twice and verify idempotency per spec, reporting a separate result named `feature-id (duplicate/idempotence)`.

## Specification Reference
- SPEC.md §5 Core Execution Logic; §10 Output
- GAP.md §2.2 Test Modality Support

### Expected Behavior
- Perform install steps twice in a fresh container and ensure the second run doesn’t fail.

## Implementation Requirements
- Use infrastructure from #351; script approach or a dedicated idempotence check.
- Add `--skip-duplicated` to skip.

## Testing Requirements
- Integration: example feature that fails on duplicate install; ensure result reflects failure.

## Acceptance Criteria
- Duplicate test runs, aggregates result, and respects skip flag; CI green.

## References
- PARITY_APPROACH.md Theme 1 (JSON Output Contract)

Issue: https://github.com/get2knowio/deacon/issues/354
