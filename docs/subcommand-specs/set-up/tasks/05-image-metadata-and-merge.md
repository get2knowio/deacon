---
subcommand: set-up
type: enhancement
priority: medium
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: medium", "area: config"]
---

# [set-up] Extract image metadata and merge configuration with lifecycle origin map

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement extraction of devcontainer metadata from the running container image labels and merge it with optional `--config` to produce a `CommonMergedDevContainerConfig`. Track lifecycle command origins in a `LifecycleHooksInstallMap` for diagnostics.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution

**From GAP.md Section:** 1.2 Core Execution Logic (Phase 2), 1.7 Lifecycle Hook Execution (origin tracking)

### Expected Behavior
- Read `--config` if provided; parse into base config.
- Inspect container image labels for features and lifecycle metadata; map into config shape.
- Merge using union/override rules; produce arrays for lifecycle commands.
- Build `lifecycleCommandOriginMap` capturing origin string (e.g., `image:label`, `config:file`).

### Current Behavior
- Some config parsing exists but no image metadata extraction nor set-up merge behavior.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/config/metadata.rs` – New: image label parsing utilities.
- `crates/core/src/config/merge.rs` – Extend merge to produce `CommonMergedDevContainerConfig` and origin map.
- `crates/core/src/setup/merge.rs` – Glue functions for set-up.

#### Specific Tasks
- [ ] Implement `get_image_metadata_from_container(inspect)`.
- [ ] Implement `merge_configuration(config, image_metadata) -> (CommonMergedDevContainerConfig, LifecycleHooksInstallMap)`.
- [ ] Unit tests covering lifecycle arrays and origin mapping.

### 2. Data Structures
- Use shapes from DATA-STRUCTURES.md (`CommonDevContainerConfig`, `CommonMergedDevContainerConfig`, `LifecycleHooksInstallMap`).

### 3. Validation Rules
- [ ] Invalid metadata formats should be ignored with a warning; do not fail set-up.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract (merged config may be returned later)
- [x] Theme 6 - Error Messages (warnings standardized)

## Testing Requirements

### Unit Tests
- [ ] Merge precedence tests (config overrides metadata where appropriate).
- [ ] Lifecycle flattening to arrays and origin attribution.

### Integration Tests
- [ ] Labelled image scenario creates expected merged lifecycle arrays.

### Smoke Tests
- [ ] None until wired in.

## Acceptance Criteria
- [ ] Metadata extraction and merge functions compile and pass tests.

## Implementation Notes
- Align label keys with containers.dev conventions used by upstream CLI.

### Edge Cases to Handle
- Missing or malformed labels → warn and continue with config only.

## Definition of Done
- [ ] Merge produces correct lifecycle arrays and origin map.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.2, §1.7)
