# [outdated] Core Command Skeleton and Pipeline

Labels:
- subcommand: outdated
- type: enhancement
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Create the `crates/deacon/src/commands/outdated.rs` module and implement the high-level pipeline: initialization, config discovery/load, lockfile read, feature extraction, parallel per-feature version computation using helpers, ordering, and handing off to output rendering.

## Specification Reference

- From SPEC.md Section: §5. Core Execution Logic; §4. Configuration Resolution; §6. State Management
- From GAP.md Section: 2. Missing Core Execution Logic; 8. Cross-Cutting Concerns (ordering, parallelization)

### Expected Behavior
- Follows pseudocode in SPEC §5: parallel iteration across features, compute map, reorder to match config order, return result shape.
- Skips non-versionable identifiers.

### Current Behavior
- No command implementation exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/deacon/src/commands/outdated.rs` (NEW)
  - Define `OutdatedArgs` (wired from CLI).
  - Implement `execute_outdated(args) -> anyhow::Result<()>` which:
    - Initializes host/logging context (reusing core helpers where available).
    - Resolves config path (respecting `--config` or discovery under `--workspace-folder`).
    - Reads devcontainer config and adjacent lockfile via `core::lockfile`.
    - Extracts versionable features in config order.
    - Spawns parallel tasks to compute `FeatureVersionInfo` for each feature using helpers (OCI + version utilities).
    - Reorders results to match config order.
    - Passes `OutdatedResult` to rendering layer.
- `crates/deacon/src/cli.rs`
  - Ensure dispatcher branch forwards required globals to `OutdatedArgs`.

#### Specific Tasks
- [ ] Implement feature iteration with `futures::stream` bounded concurrency.
- [ ] Skip invalid/non-OCI identifiers per SPEC §14.
- [ ] Record undefined fields when data is unavailable.

### 2. Data Structures
```rust
pub struct OutdatedResult {
    pub features: std::collections::HashMap<String, FeatureVersionInfo>,
}

pub struct FeatureVersionInfo {
    pub current: Option<String>,
    pub wanted: Option<String>,
    pub wanted_major: Option<String>,
    pub latest: Option<String>,
    pub latest_major: Option<String>,
}
```
(Match names exactly to DATA-STRUCTURES; field naming in Rust should map to camelCase in JSON via serde if needed.)

### 3. Validation Rules
- [ ] On missing config, return error (exit 1) with message from spec.
- [ ] For terminal hints, no additional validation beyond CLI.

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: final JSON must match schema.
- Theme 2 - CLI Validation: already handled at CLI level.
- Theme 6 - Error Messages: use standardized wording for config not found.

## Testing Requirements

### Unit Tests
- [ ] Verify reorder preserves config declaration order given unordered intermediate map.

### Integration Tests
- [ ] End-to-end run with a sample config and mocked helpers; assert structure prior to rendering.

### Smoke Tests
- [ ] To be added after rendering task.

### Examples
- [ ] N/A here; rendering handled separately.

## Acceptance Criteria
- [ ] Command skeleton compiles and orchestrates pipeline.
- [ ] Parallel computation integrated with graceful error handling (see error handling task).
- [ ] CI passes.

## Implementation Notes
- Use deterministic collection types or post-ordering step for stable output.
- Bound concurrency (e.g., 8) to avoid overwhelming registries.

### Edge Cases to Handle
- No features in config.
- Only non-versionable features.

### References
- SPEC: §5, §14
- GAP: §2, §8