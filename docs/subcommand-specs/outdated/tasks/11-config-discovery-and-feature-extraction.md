# [outdated] Config Discovery and Feature Extraction

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
Implement or reuse configuration discovery and feature extraction logic needed by `outdated`: resolve the effective `devcontainer.json`, apply pre-container variable substitution as required to evaluate the `features` block, and extract a versionable, ordered list of features.

## Specification Reference

- From SPEC.md Section: §4. Configuration Resolution; §3 Input Pipeline
- From GAP.md Section: 2.2 Phase 2: Configuration Resolution

### Expected Behavior
- Discover config under `--workspace-folder` when `--config` is not provided.
- Load config and evaluate the `features` array/object to an ordered list while preserving declaration order.
- Exclude non-OCI identifiers per spec and return versionable ones.

### Current Behavior
- No `outdated`-specific extraction exists yet.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/outdated.rs`
  - Reuse config loader from `read-configuration` if available; otherwise add minimal loader consistent with core utilities.
  - Implement `user_features_to_array(config)` behavior per spec to preserve order.

#### Specific Tasks
- [ ] Implement discovery of `.devcontainer/devcontainer.json` or `.devcontainer.json`.
- [ ] Preserve order from config when producing the feature list.

### 2. Data Structures
- Use the `DevContainerConfig` subset defined in DATA-STRUCTURES.

### 3. Validation Rules
- [ ] Config not found → exit 1 (handled in error handling task).

### 4. Cross-Cutting Concerns
- Theme 2 - CLI Validation: ensure inputs validated before heavy work.

## Testing Requirements

### Unit Tests
- [ ] Feature extraction preserves order for object and array forms.

### Integration Tests
- [ ] Discovery paths both file names; variable substitution sufficient to evaluate features.

### Smoke Tests
- [ ] Minimal discovery case works in smoke test.

### Examples
- [ ] Include examples using both config file names.

## Acceptance Criteria
- [ ] Config discovery and feature extraction implemented and tested.
- [ ] CI passes.

## Implementation Notes
- Keep substitution scope minimal for pre-container evaluation; do not implement post-container substitution here.

### Edge Cases to Handle
- No features present → output empty set.
- Mixed feature identifier styles; skip non-OCI identifiers.

### References
- SPEC: §3, §4
- GAP: §2.2