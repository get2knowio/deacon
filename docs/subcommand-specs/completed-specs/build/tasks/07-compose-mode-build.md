---
subcommand: build
type: enhancement
priority: high
scope: large
---

# [build] Implement Compose mode build with restrictions and tagging

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Replace the hard rejection of Compose configurations with the specified constrained Compose build flow: compute override for features/labels, determine original service image name, and optionally retag per `--image-name`. Enforce unsupported flags (`--platform`, `--push`, `--output`, `--cache-to`) in Compose mode.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Compose mode); §5 Core Execution Logic (Compose flow)

**From GAP.md Section:** 2.1 Compose Configuration Support

### Expected Behavior
- When a Compose-based config is detected:
  - Validate that unsupported flags are absent; otherwise error.
  - Generate a Compose override file for features/labels (scoped to build only).
  - Read the computed service image name and apply `--image-name` tags if provided.
  - Return final name(s) in output.

### Current Behavior
- Compose configs return a validation error advising manual compose build.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – add `build_and_extend_compose(...)` and `derive_original_service_image(...)` helpers per spec pseudocode; integrate with main control flow.
- `crates/core/src/compose/*.rs` (new or existing) – utilities to read compose files, env file, compute project name, and write override file.

#### Specific Tasks
- [ ] Implement Compose detection and restriction enforcement.
- [ ] Generate and use override for features/labels (content produced by later metadata/features tasks; scaffold here is sufficient to unblock control flow).
- [ ] Derive original image name and apply retags when `--image-name` is set.

### 2. Data Structures
Use `ComposeExtendResult` from DATA-STRUCTURES for return values.

```rust
pub struct ComposeExtendResult {
    pub overrideImageName: Option<String>,
    pub labels: Option<std::collections::HashMap<String,String>>,
}
```

### 3. Validation Rules
- Already captured in validation issue; ensure this flow honors them at runtime.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Tests around derive-original-image logic given a mocked compose config structure.

### Integration Tests
- [ ] End-to-end with a minimal compose fixture under `fixtures/config/compose-multiservice` to assert tagging behavior and error on restricted flags.

### Smoke Tests
- [ ] Ensure environments without Docker see consistent, well-defined errors.

## Acceptance Criteria
- [ ] Compose-based configs build/tag per spec constraints.
- [ ] Restricted flags error out with correct messages.
- [ ] CI checks pass.

## Definition of Done
- [ ] Compose flow replaces hard error and returns spec output.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§4, §5)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§2.1)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
- Diagrams: `docs/subcommand-specs/build/DIAGRAMS.md`
