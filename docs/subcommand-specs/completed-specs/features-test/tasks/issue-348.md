# [features-test] CLI flags group 1: project folder and feature selection

<!-- Labels: subcommand:features-test, type:enhancement, priority:high -->
Tracks: #345

## Issue Type
- [x] Missing CLI Flags

## Description
Add core selection flags to the `features test` subcommand.

## Specification Reference
- SPEC.md §2 Command-Line Interface
- GAP.md §1.1 Missing Required Flags

### Expected Behavior
- Support:
  - `--project-folder, -p <path>` (default `.`)
  - `--features, -f <id...>`
- Deprecate positional `target` while still accepting it.

## Implementation Requirements

### Code Changes Required
- Files:
  - `crates/deacon/src/cli.rs` — extend Clap definitions
  - `crates/deacon/src/commands/features.rs` — parse flags, update handler signature
- Tasks:
  - Implement parsing for `-p/--project-folder` and `-f/--features`.
  - Validate provided IDs map to `test/<id>/` subfolders (see discovery issue).

### Validation Rules
- If `-f` provided, feature IDs must exist under `test/`.

## Testing Requirements
- Unit: clap parsing, defaults, override positional.
- Integration: select subset of features.

## Acceptance Criteria
- Flags appear in `--help` and function correctly.
- CI green.

## References
- PARITY_APPROACH.md Theme 2 (CLI Validation Rules)

Issue: https://github.com/get2knowio/deacon/issues/348
