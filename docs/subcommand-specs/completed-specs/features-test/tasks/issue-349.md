# [features-test] CLI flags group 2: scenario control and filtering

<!-- Labels: subcommand:features-test, type:enhancement, priority:high -->
Tracks: #345

## Issue Type
- [x] Missing CLI Flags
- [x] Error Handling

## Description
Add scenario-related flags and enforce validation rules per spec.

## Specification Reference
- SPEC.md §2 Command-Line Interface
- GAP.md §1.2 Argument Validation Gaps

### Expected Behavior
- Support:
  - `--filter <string>`
  - `--global-scenarios-only`
  - `--skip-scenarios`
- Enforce mutual exclusions:
  - `--global-scenarios-only` vs `--features`
  - `--skip-scenarios` vs `--global-scenarios-only`
  - `--filter` vs `--skip-scenarios`

## Implementation Requirements
- Add flags in `cli.rs`; validate in command handler.
- Emit exact error messages per spec.

## Testing Requirements
- Unit: mutually exclusive combinations produce clap errors.
- Integration: filtering behavior with example scenarios.

## Acceptance Criteria
- Flags work; exclusivity enforced with exact messages.
- CI green.

## References
- PARITY_APPROACH.md Theme 2 (CLI Validation Rules)

Issue: https://github.com/get2knowio/deacon/issues/349
