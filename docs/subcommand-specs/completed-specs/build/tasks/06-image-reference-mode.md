---
subcommand: build
type: enhancement
priority: high
scope: large
---

# [build] Implement image reference mode (config.image) with Features extension

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Support building from an existing base image specified via `config.image` by extending it with Features and metadata without a Dockerfile. This replaces the current hard error and aligns with the spec’s extend-image pathway.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (Image reference mode)

**From GAP.md Section:** 2.2 Image Reference Mode

### Expected Behavior
- When `config.image` is present and no Dockerfile is provided, extend the base image by applying Features and metadata.
- Tag the resulting image with provided `--image-name` values or derived default.
- Return final `imageName` in output.

### Current Behavior
- Returns a validation error that `image` cannot be built.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – implement `extend_image(...)` function to:
  - Pull base image if missing.
  - Generate a minimal Dockerfile layer sequence that adds Features and labels.
  - Build using empty context + build contexts for feature content.
  - Apply tags per `--image-name`.
- `crates/core/src/` – if needed, add helpers for generating feature install scripts and metadata labels (see next tasks on features and labels).

#### Specific Tasks
- [ ] Implement `extend_image` per spec pseudocode.
- [ ] Integrate with tagging logic from issue 04.
- [ ] Ensure metadata label schema is applied (see separate metadata task but add plumbing hooks here).

### 2. Data Structures
Use `ImageBuildOptions` from DATA-STRUCTURES to represent generated build inputs.

```rust
pub struct ImageBuildOptions {
    pub dstFolder: String,
    pub dockerfileContent: String,
    pub overrideTarget: String,
    pub dockerfilePrefixContent: String,
    pub buildArgs: std::collections::HashMap<String,String>,
    pub buildKitContexts: std::collections::HashMap<String,String>,
    pub securityOpts: Vec<String>,
}
```

### 3. Validation Rules
- Respect prior gating and mutual exclusions; this task focuses on the extend flow.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Test generation of Dockerfile content for extension (no Docker required).

### Integration Tests
- [ ] If feasible, a light-weight integration test using a base image like `alpine` to validate argument assembly (can be mocked if Docker unavailable in CI).

### Smoke Tests
- [ ] Add a smoke test to ensure `config.image` path no longer errors and returns success JSON.

## Acceptance Criteria
- [ ] `config.image` flows succeed with feature extension.
- [ ] Final tags and JSON output correct.
- [ ] CI checks pass.

## Definition of Done
- [ ] Hard error replaced with working extend-image path.
- [ ] Tests in place.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§5)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§2.2)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
