---
subcommand: build
type: enhancement
priority: high
scope: medium
---

# [build] Enforce validation rules and BuildKit/Compose gating

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Error Handling
- [x] Testing & Validation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Other: ___________

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Implement the required validation and gating rules for the new flags. This includes mutual exclusion (`--push` with `--output`), BuildKit-only gating for specific flags, and Compose-mode restrictions, all with exact error messages and exit codes.

## Specification Reference

**From SPEC.md Section:** §2 Flag Taxonomy and Argument Validation Rules; §9 Error Handling Strategy

**From GAP.md Section:** 1.2 Missing Validation Rules; 4.2 Error Message Format

### Expected Behavior
- On parse/validation, before expensive work:
  - Error if `--output` is used with `--push` with message: "--push true cannot be used with --output."
  - Error if BuildKit is disabled and any of `--platform`, `--push`, `--output`, `--cache-to` are provided.
  - In Compose mode, error if any of `--platform`, `--push`, `--output`, `--cache-to` are provided.
  - Error if `--config` filename is not `devcontainer.json` or `.devcontainer.json` with message: "Filename must be devcontainer.json or .devcontainer.json (...)."

### Current Behavior
- Partial or missing validations; Compose configs are hard-rejected instead of constraint-based handling.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` – add `conflicts_with` for `--push` vs `--output` when possible; custom validation hooks as needed.
- `crates/deacon/src/commands/build.rs` – add explicit pre-execution validation function applying spec rules and emitting precise error messages.
- `crates/deacon/src/commands/build.rs` – augment `extract_build_config` or pre-checks to identify Compose mode and apply the Compose-specific restrictions (this issue does not implement Compose build behavior yet; simply enforce errors).

#### Specific Tasks
- [ ] Implement mutual exclusion of `--push` and `--output` with exact message.
- [ ] Implement BuildKit-only gating and emit errors when unavailable but flags used.
- [ ] Implement Compose-mode restriction errors for the specified flags.
- [ ] Validate `--config` filename per spec and harmonize the error text.

### 2. Data Structures
N/A beyond `BuildArgs` fields from issue 01.

### 3. Validation Rules
- [ ] Mutual exclusion: `--push` vs `--output` → "--push true cannot be used with --output."
- [ ] BuildKit gating: `--platform|--push|--output|--cache-to` require BuildKit; error if disabled.
- [ ] Compose restrictions: same flags not supported in Compose mode; emit "not supported" error.
- [ ] `--config` filename validation as specified.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 2 - CLI Validation: enforce mutually exclusive and gated flags.
- [x] Theme 6 - Error Messages: use exact messages and add context via `anyhow::Context` where useful.

## Testing Requirements

### Unit Tests
- [ ] Tests for mutual exclusion error message.
- [ ] Tests for BuildKit gating error messages.
- [ ] Tests for Compose restriction error messages (using a mocked config indicating Compose mode).
- [ ] Tests for invalid `--config` filename messaging.

### Integration Tests
- [ ] Add integration tests covering failure cases and exit code 1.

### Smoke Tests
- [ ] Ensure smoke test continues to pass in environments without Docker by accepting well-defined errors.

### Examples
- [ ] Document examples that demonstrate expected errors in `examples/build/README.md` (optional).

## Acceptance Criteria
- [ ] All validations run before attempting any Docker interaction.
- [ ] Exact error text matches the spec.
- [ ] Exit code is 1 on errors.
- [ ] CI checks pass with fmt/clippy strictness.

## Implementation Notes
- BuildKit detection can leverage existing `should_use_buildkit` helper but add user-friendly messages per spec.

### Edge Cases to Handle
- Flags present multiple times or in various orders.
- Compose detection must work even when `--config` points to an alternate location.

## Definition of Done
- [ ] Validation path implemented and covered by tests.
- [ ] No behavior ambiguity remains for the flagged options.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§2, §9)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§1.2, §4.2)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
- Diagrams: `docs/subcommand-specs/build/DIAGRAMS.md`
- Parity Approach: `docs/PARITY_APPROACH.md`
