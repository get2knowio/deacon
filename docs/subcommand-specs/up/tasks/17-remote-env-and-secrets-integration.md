# [up] Implement remote-env flags and integrate secrets redaction

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Infrastructure/Cross-Cutting Concern

## Description
Add `--remote-env` flag for injecting environment variables into the container runtime and lifecycle, enforce `name=value` format, and ensure secret values are redacted in logs when echoed by lifecycle or setup routines. Coordinate with existing secret redaction utilities in core.

## Specification Reference
- From SPEC.md Section: §2. Command-Line Interface (Additional mounts/env/cache/build); §12. Security Considerations
- From GAP.md Section: §1 Missing Flags (`--remote-env`); §11 Security Considerations – Redaction completeness

### Expected Behavior
- `--remote-env` accepted multiple times; normalized and passed into container/run and lifecycle env.
- Redaction rules applied to logs and tracing where values may appear.

### Current Behavior
- Flag missing; redaction config exists but not integrated for up lifecycle.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add `--remote-env` with validator.
- `crates/deacon/src/commands/up.rs` - inject into runtime and lifecycle; hook redaction around outputs.
- `crates/core/src/redaction.rs` - ensure APIs accessible; extend if needed for environment injection.

### 2. Data Structures
```rust
// ProvisionOptions.remoteEnv: map<string,string>
```

### 3. Validation Rules
- [ ] Enforce `<name>=<value>` format; exact error messaging.

### 4. Cross-Cutting Concerns
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.
- [x] Infrastructure Item 7 - Secrets Management & Log Redaction.

## Testing Requirements
- Unit: parser and redaction mapping.
- Integration: env visible in container; logs do not leak raw values.

## Acceptance Criteria
- Remote env works; secrets redacted; tests pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2, §12)
- `docs/subcommand-specs/up/GAP.md` (§1, §11)
- `crates/core/src/redaction.rs`
