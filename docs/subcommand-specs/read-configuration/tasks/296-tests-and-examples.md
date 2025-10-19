issue: 296
title: "[read-configuration] Tests and examples: coverage for flags, features, merged, and errors"
labels:
  - subcommand: read-configuration
  - type: enhancement
  - type: testing
  - priority: high
  - scope: medium
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: ___________

## Description
Add comprehensive unit, integration, and smoke test coverage for the read-configuration subcommand, spanning CLI flags, feature resolution, merged configuration semantics (with and without containers), workspace output, and error conditions. Provide minimal examples/fixtures to ensure deterministic tests and update examples documentation if new examples are added.

## Specification Reference

From SPEC.md Section: §2. Command-Line Interface, §4. Configuration Resolution, §5. Core Execution Logic, §9. Error Handling Strategy, §10. Output Specifications, §14. Edge Cases, §15. Testing Strategy

From GAP.md Section: 1.1 Missing Flags, 1.3 Argument Validation, 3.3 Feature Resolution, 3.4 Merge Algorithm, 9. Output Specifications, 15. Testing Strategy

### Expected Behavior
Relevant excerpts summarized from SPEC.md and DIAGRAMS.md:
- Always emit a structured JSON payload to stdout with at least `configuration`; include `workspace` when a config/workspace is read; include `featuresConfiguration` when requested or needed; include `mergedConfiguration` when requested.
- When container is provided and mergedConfiguration requested, derive metadata from the container; otherwise, derive from features + build info.
- Additional features from `--additional-features` are incorporated into the features plan.
- Validation errors print to stderr only and exit with code 1; stdout remains empty.

### Current Behavior
Extracted from GAP.md:
- Output structure is not compliant (missing `workspace`, `featuresConfiguration`, and merged semantics incorrect).
- No container path; no features resolution path; weak validation.
- Limited tests exist; critical cases lacking per §15.

## Implementation Requirements

### 1. Code Changes Required

Files to Modify / Add
- `crates/deacon/tests/` – Add integration tests for read-configuration covering selectors, features, merged semantics, and errors. Use `assert_cmd` and fixtures.
- `crates/deacon/tests/smoke_basic.rs` – Extend to assert baseline selector requirement and stable JSON payload structure when invoked properly without Docker.
- `crates/deacon/src/commands/read_configuration.rs` – Only if needed to expose test seams or adjust behavior to pass tests per spec; avoid functional drift beyond spec compliance.
- `fixtures/config/` – Add minimal fixtures if not already present for: valid basic, with features, invalid (non-object), and missing file scenario.
- `examples/read-configuration/` – Add or update a minimal example illustrating `--include-features-configuration` and `--include-merged-configuration` without requiring Docker.

Specific Tasks
- [ ] Unit tests for argument parsing/validation (e.g., id-label format, terminal pairing, selector requirement).
- [ ] Integration test: reads configuration from workspace, asserts `configuration` and `workspace` fields present; `featuresConfiguration` and `mergedConfiguration` absent by default.
- [ ] Integration test: `--include-features-configuration` adds `featuresConfiguration` section (without requiring Docker).
- [ ] Integration test: `--include-merged-configuration` with no container derives metadata from features/build info; asserts merged semantics (env last-wins, mounts dedup by target, lifecycle arrays merged).
- [ ] Integration test: `--additional-features` JSON merges into plan and is reflected in `featuresConfiguration`.
- [ ] Negative tests: missing selector, bad id-label, malformed additional-features JSON, missing config path, non-object root; assert exact stderr messages and empty stdout.
- [ ] If container workflows are not yet implemented, add TODO-skipped tests or mark with cfg to run when Docker available; do not silently pass—assert explicit Not Implemented errors where appropriate per repo policy.
- [ ] Update examples and examples/README.md only if a new example is added for clarity.

### 2. Data Structures

Required from DATA-STRUCTURES.md to shape test assertions:
```rust
// Output payload shape
struct ReadConfigurationOutput {
		configuration: DevContainerConfig,
		workspace: Option<WorkspaceConfig>,
		featuresConfiguration: Option<FeaturesConfig>,
		mergedConfiguration: Option<MergedDevContainerConfig>,
}
```

### 3. Validation Rules (as test cases)
- [ ] Requires any of `--container-id`, `--id-label`, or `--workspace-folder`.
- [ ] `--id-label` must match `<name>=<value>`.
- [ ] `--terminal-columns` and `--terminal-rows` must be paired.
- [ ] `--additional-features` must be valid JSON object.

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 1 - JSON Output Contract: Assert exact field presence/omission rules in tests.
- [ ] Theme 2 - CLI Validation: Use clap/runtime validations and assert exact error messages.
- [ ] Theme 6 - Error Messages: Compare stderr strings exactly; avoid substring-only assertions.

## Testing Requirements

### Unit Tests
- [ ] Parser-level tests for id-label regex, terminal pairing, and additional-features JSON parsing.
- [ ] Pure logic tests for merge behavior (if implemented in pure functions) to assert last-wins env and mount deduplication by target.

### Integration Tests
- [ ] Basic happy path with workspace: verify `configuration` and `workspace` present, others omitted.
- [ ] Features-only output: `--include-features-configuration` adds featuresConfiguration.
- [ ] Merged without container: `--include-merged-configuration` produces `mergedConfiguration` consistent with spec semantics.
- [ ] Additional features merging: supplied JSON shows up in featuresConfiguration.
- [ ] Error cases: missing selector, invalid id-label, malformed additional-features, config not found, non-object root; assert stderr and exit code.

### Smoke Tests
- [ ] Ensure smoke_basic.rs reflects structured payload expectations and that behavior is resilient when Docker is unavailable (well-defined errors, not panics).

### Examples
- [ ] Add/update example(s) under `examples/read-configuration/` demonstrating features-only and merged-no-container flows.
- [ ] Update `examples/README.md` index with a one-line description per example.
- [ ] Add matching fixtures under `fixtures/` when examples are used in tests.

## Acceptance Criteria

- [ ] Test suite covers all flows listed above with deterministic assertions.
- [ ] Output JSON structure is asserted precisely (presence/absence rules) and matches DATA-STRUCTURES.md.
- [ ] Error messages match the spec exactly and stdout is empty on failure.
- [ ] All CI checks pass:
	```bash
	cargo build --verbose
	cargo test --verbose -- --test-threads=1
	cargo fmt --all
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	```
- [ ] No `unwrap()`/`expect()` in test-targeted code paths; errors include helpful context.
- [ ] Where container-specific features are not implemented, tests assert explicit Not Implemented errors (per repo policy) rather than skipping silently.

## Implementation Notes

Key Considerations
- Prefer hermetic tests that do not require Docker unless specifically testing container paths; gate with cfg/env as needed.
- Keep examples minimal and focused on one concept each (features-only, merged-no-container).
- When verifying merged semantics, assert: env last-wins per key, mount dedup by target with stable order, lifecycle arrays merged per metadata ordering.

Edge Cases to Handle
- Only container flags (no config/workspace): should still succeed when container path implemented; until then, assert a clear Not Implemented error instead of passing silently.
- `--override-config` without base workspace: allowed; test that configuration reflects override-only.
- `--id-label` order differences should not affect `${devcontainerId}`; include a test when container path becomes available.

Reference Implementation
- Follow SPEC.md §15 pseudocode for test cases and DIAGRAMS.md flows. Mirror error strings from SPEC.md §9.

## Definition of Done

- [ ] Tests comprehensively cover flags, feature resolution output, merged semantics, and error flows.
- [ ] Examples and fixtures are in place and referenced by tests as needed.
- [ ] CI is green with zero clippy warnings.

