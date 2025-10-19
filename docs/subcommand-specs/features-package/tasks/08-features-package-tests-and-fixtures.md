---
subcommand: features-package
type: enhancement
priority: high
scope: medium
labels: ["subcommand: features-package", "type: enhancement", "priority: high", "scope: medium"]
---

# [features-package] Comprehensive Tests and Fixtures

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation
- [ ] Other: 

## Description
Add unit, integration, and smoke tests covering single and collection packaging, `devcontainer-collection.json` generation, `--force-clean-output-folder`, and default `target` behavior. Include minimal fixtures to keep tests deterministic and hermetic.

## Specification Reference

**From SPEC.md Section:** “§15. Testing Strategy”

**From GAP.md Section:** “7. TEST COVERAGE GAPS”

### Expected Behavior
Tests reflect the scenarios enumerated in SPEC §15 and GAP §7.

### Current Behavior
Tests cover single-feature basics only.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify/Add
- `crates/deacon/tests/test_features_cli.rs`
  - Add integration tests:
    - [ ] `features_package_collection_two_features`
    - [ ] `features_package_generates_collection_json`
    - [ ] `features_package_force_clean_removes_previous_artifacts`
    - [ ] `features_package_default_target_current_dir`
- `crates/deacon/src/commands/features.rs`
  - Add unit-test-only helpers behind `#[cfg(test)]` if helpful.

#### Fixtures
- `fixtures/features/collection-basic/`
  - `src/feature-a/devcontainer-feature.json`
  - `src/feature-b/devcontainer-feature.json`

### 2. Data Structures
N/A.

### 3. Validation Rules
- [ ] Ensure error tests assert exact messages from Task 07.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 1 - JSON Output Contract (when `--json` used)
- [ ] Theme 2 - CLI Validation
- [ ] Theme 6 - Error Messages

## Acceptance Criteria
- [ ] New tests added and passing locally.
- [ ] Fixtures added and used only in tests.
- [ ] Smoke test extended if behavior is user-visible.
- [ ] CI green.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§15)
- `docs/subcommand-specs/features-package/GAP.md` (Section 7)
- `AGENTS.md` testing guidance
