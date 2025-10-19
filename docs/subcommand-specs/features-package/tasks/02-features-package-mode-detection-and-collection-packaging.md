---
subcommand: features-package
type: enhancement
priority: high
scope: large
labels: ["subcommand: features-package", "type: enhancement", "priority: high", "scope: large"]
---

# [features-package] Implement Mode Detection and Collection Packaging

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add detection logic for single vs. collection mode and implement collection packaging per spec. In collection mode, iterate `src/*/` subfolders, package each feature into a `.tgz` artifact, and aggregate metadata for later collection file generation.

## Specification Reference

**From SPEC.md Section:** “§4. Configuration Resolution” and “§5. Core Execution Logic”

**From GAP.md Section:** “1. CRITICAL: Collection Mode Not Implemented”

### Expected Behavior
Pseudocode (SPEC §5):
```
IF is_single_feature(input.target) THEN
  metas = package_single_feature(target, output)
ELSE
  metas = package_feature_collection(join(target,'src'), output)
END IF
```

### Current Behavior
Only single-feature packaging is implemented; no detection for `src/` and no iteration.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs`
  - Add helpers:
    - `fn detect_packaging_mode(target: &Path) -> Result<PackagingMode>`
    - `async fn package_feature_collection(src_dir: &Path, output_dir: &Path) -> Result<Vec<FeatureMetadata>>>`
  - Update `execute_features_package` to:
    - Apply `force_clean` (if provided by CLI; wiring added later) then ensure output dir exists
    - Detect mode and branch accordingly
    - Return combined result, including count/logs
- Consider small refactor: extract current single-feature path into `async fn package_single_feature(dir: &Path, out: &Path) -> Result<FeatureMetadata>` that also returns parsed metadata.

#### Specific Tasks
- [ ] Implement mode detection: single if `<target>/devcontainer-feature.json` exists; else collection if `<target>/src` exists and contains subdirs.
- [ ] Implement iteration across `src/*/` directories; skip non-directories and hidden files.
- [ ] For each feature folder, validate presence of `devcontainer-feature.json`; error if missing with: "Invalid feature folder: devcontainer-feature.json not found." (Theme 6)
- [ ] Produce `.tgz` artifacts using existing `create_feature_package` (adjust extension per Task 03) and collect metadata list.
- [ ] Log "Packaging single feature..." vs. "Packaging feature collection..." and per-feature results.

### 2. Data Structures
Required from DATA-STRUCTURES.md:
```rust
// Minimal usage within this task
pub struct FeatureMetadata { /* use from deacon_core::features */ }
```

### 3. Validation Rules
- [ ] Error if neither single nor collection structure is detected: "Target does not contain a feature or a collection (src/)."
- [ ] Error if collection is empty: "No features found under src/."

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 3 - Collection Mode vs Single Mode: correct detection and iteration.
- [ ] Theme 6 - Error Messages: exact messages as specified above.

## Testing Requirements

### Unit Tests
- [ ] `detect_packaging_mode` for single, collection, and invalid targets.
- [ ] `package_feature_collection` with 2+ valid features creates artifacts and returns metadata vec.
- [ ] Invalid subfolder missing `devcontainer-feature.json` returns error with exact message.

### Integration Tests
- [ ] End-to-end run on a temp directory with `src/feature-a` and `src/feature-b`.
- [ ] Verify output files exist and logs contain per-feature messages.

### Smoke Tests
- [ ] Add smoke test that runs `features package` against a collection fixture.

### Examples
- [ ] Add `examples/feature-management/collection/` with minimal two features to demonstrate.

## Acceptance Criteria
- [ ] Mode detection implemented with robust errors.
- [ ] Collection packaging implemented; artifacts created for each feature.
- [ ] Logs reflect mode and per-feature outputs.
- [ ] CI green on build/test/fmt/clippy.

## Implementation Notes
- Keep filesystem iteration deterministic (sort entries) to ensure stable logs and tests.
- Reuse `parse_feature_metadata` for validation and metadata capture.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§4–5)
- `docs/subcommand-specs/features-package/GAP.md` (Section 1)
- `docs/PARITY_APPROACH.md` (Theme 3, Theme 6)
