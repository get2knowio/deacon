---
subcommand: features-package
type: documentation
priority: medium
scope: small
labels: ["subcommand: features-package", "type: documentation", "priority: medium", "scope: small"]
---

# [features-package] Update Examples and Documentation

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [x] Other: Documentation

## Description
Refresh examples and README snippets to show collection packaging, `.tgz` artifacts, and the `devcontainer-collection.json` file. Document global `--log-level` usage for this subcommand to close the GAP.md documentation note.

## Specification Reference

**From SPEC.md Section:** “§1. Subcommand Overview” and “§10. Output Specifications”

**From GAP.md Section:** “5. MISSING: `--log-level` Flag” (documentation note)

### Expected Behavior
- Examples demonstrate both single and collection packaging.
- Docs mention: use `deacon --log-level debug features package ...` (global flag).

### Current Behavior
- Examples mostly show single feature; do not show collection metadata.

## Implementation Requirements

### 1. Code Changes Required
Docs and examples only.

#### Files to Modify/Add
- `examples/feature-management/` add `collection/` with two minimal features and a short README.
- Update `examples/README.md` index to include the new example.
- Update `EXAMPLES.md` or specific README sections mentioning `features package` usage and outputs.

### 2. Data Structures
N/A.

### 3. Validation Rules
N/A.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 1 - JSON Output Contract (document behavior when `--json`)
- [ ] Theme 3 - Collection Mode vs Single Mode (document detection rules)

## Acceptance Criteria
- [ ] New example added and referenced.
- [ ] Docs updated to reflect `.tgz` and metadata file.
- [ ] Note added about global `--log-level` for this subcommand.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§1, §10)
- `docs/subcommand-specs/features-package/GAP.md` (Section 5)
