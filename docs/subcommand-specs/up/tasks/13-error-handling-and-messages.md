# [up] Standardize errors and messages per spec

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: small -->

## Issue Type
- [x] Error Handling
- [x] Testing & Validation

## Description
Ensure all user/system/config errors conform to the specification and Parity Approach theme: standardized messages, actionable details, correct mapping to JSON error shape, and accurate exit codes. Include GPU unsupported warnings and container exit during lifecycle error traces.

## Specification Reference
- From SPEC.md Section: §9. Error Handling Strategy
- From GAP.md Section: §8 Missing; Summary of Critical Missing Features references

### Expected Behavior
- Clear user errors for invalid args, formats, and filenames.
- System errors categorized and surfaced with context.
- Lifecycle failure stops execution with detailed message and description.

### Current Behavior
- Errors are surfaced via anyhow; standardized mapping and messages missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - centralize error mapping to JSON; add contexts with anyhow::Context.
- `crates/deacon/src/commands/read_configuration.rs` - enforce filename errors.

### 2. Data Structures
```rust
// UpErrorResult fields per DATA-STRUCTURES.md
```

### 3. Validation Rules
- [ ] Use exact error messages where specified in SPEC.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [x] Theme 1 - JSON Output Contract.

## Testing Requirements
- Unit: map representative errors to expected JSON; message text equality.
- Integration: GPU unsupported warning presence; lifecycle failure reporting.

## Acceptance Criteria
- Errors standardized and tested; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§9)
- `docs/subcommand-specs/up/GAP.md` (§8)
- `docs/PARITY_APPROACH.md` (Theme 6)
