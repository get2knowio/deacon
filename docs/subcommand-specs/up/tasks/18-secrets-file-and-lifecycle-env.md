# [up] Implement --secrets-file support and lifecycle env injection with redaction

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Infrastructure/Cross-Cutting Concern
- [x] Security

## Description
Add `--secrets-file <path>` support to load environment variables from one or more files, merge with `--remote-env`, inject into lifecycle execution, and ensure robust log redaction. Follow merging rules (later wins), and integrate with tracing redaction to prevent secret leakage.

## Specification Reference
- From SPEC.md Section: §12. Security Considerations
- From GAP.md Section: §11 Security Considerations – Missing `--secrets-file` and redaction completeness

### Expected Behavior
- Accept multiple `--secrets-file` flags; read `KEY=VALUE` pairs (ignore comments and blanks).
- Merge into env for lifecycle commands and container runtime as appropriate.
- Redact values in logs, progress, and error chains.

### Current Behavior
- Core redaction utilities exist; `--secrets-file` unsupported in `up`; not injected into lifecycle.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add `--secrets-file` (repeatable).
- `crates/deacon/src/commands/up.rs` - read files via `core::secrets` helper; merge with `--remote-env`; feed into lifecycle and runtime; enable redaction.
- `crates/core/src/secrets.rs` - already provides helpers; extend if necessary for multi-file merges.

### 2. Data Structures
```rust
// ProvisionOptions.secretsP?: Promise<map<string,string>> (TS ref) -> in Rust, load eagerly into a Map
```

### 3. Validation Rules
- [ ] Validate file existence and readability; error with exact message on failure.
- [ ] Detect duplicate keys across files; later wins; warn at debug level.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [x] Infrastructure Item 7 - Secrets Management & Log Redaction.
- [x] Theme 1 - JSON Output Contract: never print secret values into JSON.

## Testing Requirements
- Unit: parse secrets file(s), merge order, invalid lines handling.
- Integration: lifecycle sees env; logs are redacted.

## Acceptance Criteria
- Secrets file supported; env injected; redaction verified; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§12)
- `docs/subcommand-specs/up/GAP.md` (§11)
- `crates/core/src/secrets.rs`
