---
subcommand: build
type: enhancement
priority: high
scope: medium
---

# [build] Implement metadata label injection (devcontainer + features + user labels)

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Augment builds to include the full devcontainer metadata label (JSON) and optionally feature customizations. Also pass through user-supplied labels from `--label` flags. Replace the singular `org.deacon.configHash` label with the spec-compliant set while retaining it if still useful.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic – Metadata label schema

**From GAP.md Section:** 3.2 Metadata Label Injection

### Expected Behavior
- Always inject devcontainer metadata label with merged configuration and feature metadata.
- Include or omit Feature `customizations` based on `--skip-persisting-customizations-from-features` flag.
- Add any `--label` name=value pairs to the build arguments.

### Current Behavior
- Only `org.deacon.configHash` label is applied.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – generate metadata label JSON and append `--label` args accordingly; add pass-through for user labels.
- `crates/core/src/` – add helper module to serialize devcontainer metadata label according to schema and merge feature metadata/customizations.

#### Specific Tasks
- [ ] Create serializer for devcontainer metadata label.
- [ ] Append `--label devcontainer.metadata=<json>` (or schema-defined key) to build args.
- [ ] Append all user `--label` entries.

### 2. Data Structures
Use the metadata schema defined in the implementors spec; if not yet present, define a struct mirroring upstream shape and serialize via `serde`.

### 3. Validation Rules
- Validate `--label` format `name=value`; emit an input error if parsing fails (align with CLI validation theme).

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: metadata itself is not printed but influences downstream tools.
- [x] Theme 2 - CLI Validation: label format validation.
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Test label serializer with/without feature customizations.
- [ ] Test that `--label` values are propagated to build args.

### Integration Tests
- [ ] Inspect built image labels for presence of metadata and user labels (when Docker available) or assert arg assembly.

### Smoke Tests
- [ ] Add a smoke that checks JSON output while labels are present (indirect verification).

## Acceptance Criteria
- [ ] Metadata label injected per schema; user labels passed through.
- [ ] Optional omission of feature customizations respects flag.
- [ ] CI checks pass.

## Definition of Done
- [ ] Label behavior implemented with tests.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§5)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§3.2)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
