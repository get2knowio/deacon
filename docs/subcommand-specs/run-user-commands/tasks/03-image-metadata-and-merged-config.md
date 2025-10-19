---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: config"]
---

# [run-user-commands] Extract image metadata and build MergedDevContainerConfig

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add image metadata extraction from container labels and merge it with user/override configuration to produce a `MergedDevContainerConfig`, including lifecycle arrays, `waitFor`, and env maps.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Merge Algorithm)

**From GAP.md Section:** 2.3 Image Metadata Merge (completely missing)

### Expected Behavior
- Parse labels into feature/lifecycle metadata.
- Merge to produce arrays: onCreate/updateContent/postCreate/postStart/postAttach.
- Merge `remoteEnv`, `containerEnv`, and `waitFor` (last-defined wins).

### Current Behavior
- Only base devcontainer config is used; no image metadata merge.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/config/metadata.rs` – New utilities to parse image labels.
- `crates/core/src/config/merge.rs` – New or extended merge functions returning `MergedDevContainerConfig`.
- `crates/core/src/run_user_commands/merge.rs` – Glue specific to this subcommand.

#### Specific Tasks
- [ ] Implement `get_image_metadata_from_container(inspect) -> ImageMetadata`.
- [ ] Implement `merge_configuration(user_config, metadata) -> MergedDevContainerConfig`.
- [ ] Unit tests for label parsing and merge precedence.

### 2. Data Structures
```rust
pub struct MergedDevContainerConfig { /* from DATA-STRUCTURES.md */ }
```

### 3. Validation Rules
- [ ] Ignore malformed labels with warnings; do not fail execution.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 6 - Error Messages (warnings format)

## Testing Requirements

### Unit Tests
- [ ] Merge precedence and lifecycle arrays created properly.

### Integration Tests
- [ ] Label-bearing container leads to merged lifecycle arrays.

## Acceptance Criteria
- [ ] Merged config produced and tested.

## Definition of Done
- [ ] Functions compile and are covered by tests.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§2.3)
