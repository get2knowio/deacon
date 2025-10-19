# [features-test] Scenario testing: JSONC parsing, per-scenario execution, and global scenarios

<!-- Labels: subcommand:features-test, type:enhancement, priority:high -->
Tracks: #345, depends on: #351

## Issue Type
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Implement scenario testing: parse `scenarios.json` (JSONC), build per-scenario workspaces/containers, run scenario scripts, support `test/_global` scenarios, and aggregate results.

## Specification Reference
- SPEC.md §5 Core Execution Logic; §7 External System Interactions; §10 Output
- DIAGRAMS.md Scenario Test sequence
- GAP.md §3.3 Scenario Testing and §2.2 Modality Support

### Expected Behavior
- Parse `test/<feature>/scenarios.json` and `test/_global/scenarios.json` with JSONC tolerance.
- Apply `--filter` to scenario names; skip when `--skip-scenarios`.
- For each scenario, run its script (same name) inside the container.
- Aggregate results with test names like `feature-id (scenario-name)`.

## Implementation Requirements
- JSONC parser crate selection (no network in tests): e.g., `jsonc-parser` equivalent in Rust or pre-strip comments.
- Container launch via infrastructure; support base image/remote user where applicable.
- Global scenarios executed when no `--features` specified or when `--global-scenarios-only`.

## Testing Requirements
- Unit: JSONC parsing including comments and trailing commas.
- Integration: example scenarios including `_global`.

## Acceptance Criteria
- Scenario tests run and report; CI green.

## References
- PARITY_APPROACH.md Theme 2, Theme 6

Issue: https://github.com/get2knowio/deacon/issues/353
