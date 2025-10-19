---
subcommand: upgrade
type: enhancement
priority: high
scope: small
---

# [upgrade] Dry-Run Mode: Print Lockfile JSON to Stdout

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Implement the `--dry-run` behavior: pretty-print the generated lockfile JSON (2-space indent) to stdout and skip writing the lockfile to disk. Important: if hidden pinning flags are used, the config edit still applies.

## Specification Reference

**From SPEC.md Section:** §10. Output Specifications

**From GAP.md Section:** 5.1 Dry-Run Mode

### Expected Behavior
- When `--dry-run` is set, print only JSON to stdout; logs go to stderr.
- Do not write the lockfile file. If pinning flags were used, the config file may still be edited per design decision.

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Branch on `args.dry_run`
  - Serialize lockfile with `serde_json::to_string_pretty` and write to stdout
  - Ensure no trailing newline beyond serializer default behavior

#### Specific Tasks
- [ ] Print lockfile JSON to stdout
- [ ] Ensure no file persisted
- [ ] Preserve existing stderr logging

### 2. Data Structures
- Reuse `Lockfile`

### 3. Validation Rules
- [ ] None beyond existing

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract: stdout only; no extra whitespace; deterministic ordering already handled in core `write_lockfile`—replicate ordering using the same sort routine or rely on stable map creation

## Testing Requirements

### Unit Tests
- [ ] Verify stdout contains valid JSON with expected keys

### Integration Tests
- [ ] Dry-run on a fixture with features prints expected structure and does not create lockfile

### Smoke Tests
- [ ] None

### Examples
- [ ] Add later in examples task

## Acceptance Criteria
- [ ] `--dry-run` prints JSON to stdout
- [ ] No filesystem writes to lockfile path
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§10)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§5.1)
