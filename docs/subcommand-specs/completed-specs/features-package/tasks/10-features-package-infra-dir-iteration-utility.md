---
subcommand: features-package
type: enhancement
priority: low
scope: small
labels: ["subcommand: features-package", "type: enhancement", "priority: low", "scope: small"]
---

# [Infrastructure] features-package Directory Iteration Utility

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Add a tiny utility to list and sort `src/*/` entries deterministically while filtering out non-directories and hidden/system items. This is reused by collection packaging and improves test stability.

## Specification Reference

**From SPEC.md Section:** “§11. Performance Considerations” and “§13. Cross-Platform Behavior”

**From GAP.md Section:** Related to collection mode robustness.

### Expected Behavior
- Deterministic iteration order (lexicographic by folder name).
- Ignores entries that are not directories; ignores names starting with `.`.

### Current Behavior
- Not implemented; ad-hoc iteration would lead to non-deterministic tests.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify/Add
- `crates/deacon/src/commands/features.rs`
  - Add `fn list_feature_dirs(src_dir: &Path) -> Result<Vec<PathBuf>>` returning sorted list.
  - Use in `package_feature_collection` (Task 02).

### 2. Data Structures
N/A.

### 3. Validation Rules
N/A.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 6 - Error Messages: include context on fs errors.

## Testing Requirements

### Unit Tests
- [ ] Verify ordering and filtering behavior across mixed entries.

## Acceptance Criteria
- [ ] Utility implemented and used by collection packaging.
- [ ] Deterministic order confirmed by tests.
- [ ] CI green.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§11, §13)
