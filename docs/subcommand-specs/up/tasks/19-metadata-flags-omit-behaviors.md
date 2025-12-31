# [up] Implement metadata omission flags

<!-- Suggested labels: subcommand: up, type: enhancement, priority: low, scope: small -->

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation

## Description
Add support for `--omit-config-remote-env-from-metadata` and `--omit-syntax-directive` flags. These control whether certain metadata (remote env keys or Dockerfile syntax directives) are included in labels/metadata when emitting or building extended images.

## Specification Reference
- From SPEC.md Section: §2. Command-Line Interface (Features/dotfiles/metadata)
- From GAP.md Section: §1 Missing Flags (metadata)

### Expected Behavior
- When `--omit-config-remote-env-from-metadata` is set, config remote env is excluded from metadata payloads/labels.
- When `--omit-syntax-directive` is set, upstream Dockerfile `# syntax=` directive is omitted in generated contexts.

### Current Behavior
- Flags not supported; default behavior always includes metadata and syntax directives when present.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add both flags with help text.
- `crates/deacon/src/commands/up.rs` - apply omission behavior when preparing metadata for labels and when generating Dockerfiles for features/UID update.
- `crates/deacon/src/commands/build.rs` - ensure shared Dockerfile generation respects omit flag if reused.

### 2. Data Structures
```rust
// ProvisionOptions.omitConfigRemotEnvFromMetadata?: bool
// ProvisionOptions.omitSyntaxDirective?: bool
```

### 3. Validation Rules
- [ ] None beyond standard flag parsing.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages (for any generation errors).

## Testing Requirements
- Unit: confirm directive omission in generated Dockerfile text; metadata payload excludes remote env keys.

## Acceptance Criteria
- Flags parsed; behavior applied; tests pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2)
- `docs/subcommand-specs/up/GAP.md` (§1)
