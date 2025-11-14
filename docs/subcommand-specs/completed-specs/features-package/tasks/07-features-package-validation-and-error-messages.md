---
subcommand: features-package
type: enhancement
priority: medium
scope: small
labels: ["subcommand: features-package", "type: enhancement", "priority: medium", "scope: small"]
---

# [features-package] Validation Rules and Error Message Standardization

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement and enforce validation rules and standardized error messages for features packaging: invalid target structure, empty collection, missing `devcontainer-feature.json`, and output path misuse.

## Specification Reference

**From SPEC.md Section:** “§9. Error Handling Strategy” and “§14. Edge Cases and Corner Cases”

**From GAP.md Section:** Spread across sections 1–4 and 6.

### Expected Behavior
- Exact error messages:
  - "Target does not contain a feature or a collection (src/)."
  - "Invalid feature folder: devcontainer-feature.json not found."
  - "No features found under src/."
  - "Output path exists and is not a directory."

### Current Behavior
- Some errors are surfaced but with inconsistent wording; others missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/features.rs`
  - Normalize error returns with `anyhow::bail!` and `.with_context(...)` where needed to include paths.
  - Add unit tests alongside or in dedicated test module for message text.

### 2. Data Structures
N/A.

### 3. Validation Rules
- [ ] Implement all four exact error messages above.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 2 - CLI Validation (apply before expensive work)
- [ ] Theme 6 - Error Messages (exact text)

## Testing Requirements

### Unit Tests
- [ ] Each error path produces the exact message string.

### Integration Tests
- [ ] Invalid collection (missing files) surfaces the proper message.

## Acceptance Criteria
- [ ] All specified error messages implemented and covered by tests.
- [ ] CI green.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§9, §14)
- `docs/subcommand-specs/features-package/GAP.md` (Sections 1–4,6)
- `docs/PARITY_APPROACH.md` (Themes 2,6)
