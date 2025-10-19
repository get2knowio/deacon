---
subcommand: upgrade
type: documentation
priority: medium
scope: medium
---

# [upgrade] Examples and Documentation

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [x] Other: Documentation & Examples

## Description
Add example projects and documentation for the `upgrade` subcommand, including a basic upgrade flow, dry-run, and pin-feature scenarios. Update top-level README and examples index.

## Specification Reference

**From SPEC.md Section:** §1 Overview, §10 Output, §14 Edge Cases (doc notes)

**From GAP.md Section:** 6.2 Examples, 7.1 User Documentation, 7.2 Code Documentation

### Expected Behavior
- Examples demonstrate how to run `upgrade` in typical workflows
- Docs clarify that `--dry-run` still edits config when pinning flags are used

### Current Behavior
- No examples/docs coverage for `upgrade`.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify/Add
- `examples/upgrade/basic/` (new) — minimal `devcontainer.json` with a feature
- `examples/upgrade/dry-run/` (new)
- `examples/upgrade/pin-feature/` (new)
- `examples/README.md` — add section for upgrade
- `README.md` — brief command synopsis and pointers
- `docs/subcommand-specs/upgrade/` — ensure rustdoc comments exist and link SPEC

#### Specific Tasks
- [ ] Create minimal fixtures with documented commands to run
- [ ] Document caveat about dry-run + hidden flags editing config
- [ ] Add screenshots or sample outputs where helpful

### 2. Data Structures
- N/A

### 3. Validation Rules
- N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON output contract description for dry-run
- [ ] Theme 6 - Error messages reproduced in docs

## Testing Requirements

### Unit/Integration
- [ ] Ensure examples can be used in tests

### Smoke Tests
- [ ] Optional: Light invocation in smoke tests to ensure examples stay valid

## Acceptance Criteria
- [ ] Example directories created with working configs
- [ ] README and examples index updated
- [ ] Rustdoc comments added for public functions in upgrade module
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md`
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§6.2, §7)
- Parity: `docs/PARITY_APPROACH.md`
