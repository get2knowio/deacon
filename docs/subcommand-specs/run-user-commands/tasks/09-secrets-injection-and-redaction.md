---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: security"]
---

# [run-user-commands] Implement secrets injection and log redaction

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Inject secrets loaded from `--secrets-file` into the execution environment for lifecycle hooks and dotfiles, and ensure all occurrences of secret values are redacted from logs. Filter `BASH_FUNC_*` keys from injection per security guidance.

## Specification Reference

**From SPEC.md Section:** §12 Security Considerations (Secrets Handling), §5 Core Execution Logic

**From GAP.md Section:** 3.5 Secrets Handling (partial)

### Expected Behavior
- Load one or more secrets files (JSON) and merge into env (last wins).
- Redact all occurrences of secret values in stderr logs with `********`.
- Filter keys matching `^BASH_FUNC_` to avoid function export injection.

### Current Behavior
- Secrets can be loaded but are not injected or redacted comprehensively.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/secrets.rs` – Redaction utilities and merge logic.
- `crates/core/src/container_lifecycle.rs` – Accept env for each hook execution and apply redaction to logs.

#### Specific Tasks
- [ ] Implement merge and filter of secrets; provide injected env map per command.
- [ ] Implement log sink wrapper that replaces secret values with `********`.

### 2. Data Structures
- Use plain map for secrets; no new public types needed.

### 3. Validation Rules
- [ ] Invalid JSON should produce error JSON with description and exit 1.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Item 7 - Secrets Management & Log Redaction
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Merge precedence across multiple secrets files.
- [ ] Redaction across log streams.
- [ ] Filtering of BASH_FUNC_* keys.

### Integration Tests
- [ ] Lifecycle can access SECRET env and logs are masked.

## Acceptance Criteria
- [ ] Secrets injected and redacted; CI green.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§5, §12)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§3.5)
