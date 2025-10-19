# [outdated] Documentation and Help Text

Labels:
- subcommand: outdated
- type: docs
- priority: medium
- scope: small

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [x] Other: Documentation

## Description
Update user-facing documentation for the `outdated` subcommand, including `--help` descriptions, repository README references, examples directory, and CLI parity checklist updates.

## Specification Reference

- From SPEC.md Section: §1 Overview; §2 CLI; §10 Output Specifications
- From GAP.md Section: 9. Documentation Gaps

### Expected Behavior
- `deacon outdated --help` shows accurate descriptions and defaults.
- `README.md`, `EXAMPLES.md`, and `CLI_PARITY.md` mention and reflect the new command.
- An `examples/feature-management/outdated/` path contains a minimal demonstration.

### Current Behavior
- No mention of the command.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/deacon/src/cli.rs`
  - Ensure subcommand and flag docstrings match spec.
- `README.md`, `EXAMPLES.md`, `CLI_PARITY.md`
  - Add/adjust sections for `outdated`.
- `examples/feature-management/outdated/`
  - Add a minimal config and sample outputs.

#### Specific Tasks
- [ ] Update help messages.
- [ ] Add example and index entry in `examples/README.md`.
- [ ] Update `CLI_PARITY.md` to mark `outdated` tasks as in-progress/done as they complete.

### 2. Data Structures
- N/A.

### 3. Validation Rules
- [ ] Verify docs align with implemented defaults and flags.

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: reflect in docs that JSON is printed to stdout.
- Theme 6 - Error Messages: include exact user-facing messages where specified.

## Testing Requirements

### Unit Tests
- [ ] N/A.

### Integration Tests
- [ ] Help output snapshot test if desired.

### Smoke Tests
- [ ] Ensure `deacon outdated --help` exits 0.

### Examples
- [ ] Provide minimal runnable example.

## Acceptance Criteria
- [ ] Docs and examples updated and consistent.
- [ ] CI passes.

## Implementation Notes
- Keep documentation concise and consistent with other subcommand sections.

### Edge Cases to Handle
- None.

### References
- SPEC: §1, §2, §10
- GAP: §9