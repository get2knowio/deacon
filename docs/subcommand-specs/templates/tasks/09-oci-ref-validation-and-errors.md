---
subcommand: templates
type: enhancement
priority: medium
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: medium", "scope: small"]
---

# [templates] Add OCI Ref Validation and Standardized Error Messages

## Issue Type
- [x] Error Handling
- [x] Testing & Validation

## Description
Introduce strict OCI reference parsing and consistent error messages for `--template-id` and `templates metadata <templateId>`. Validate against the allowed patterns and provide actionable messages, failing before network calls.

## Specification Reference

**From SPEC.md Section:** §2 CLI (argument validation), §7 External System Interactions (manifest fetch)

**From GAP.md Section:** 1.1/1.3 malformed ref handling

### Expected Behavior
- Reject invalid refs before attempting network access.
- Use messages per Theme 6 formatting (sentence case, period).

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/templates.rs` — Add ref parser and validate flags.
- `crates/core/src/oci.rs` or `crates/core/src/refs.rs` — Centralize ref parsing utility for reuse.

### 2. Validation Rules
- [ ] Regex for name segments: `[a-z0-9]+([._-][a-z0-9]+)*` with path segments; tag or `@sha256:` digest allowed.

## Testing Requirements
- [ ] Unit tests covering valid/invalid refs; ensure early failure.

## Acceptance Criteria
- [ ] Invalid refs produce standardized errors; tests pass.

## Definition of Done
- [ ] Parser shared across apply/metadata; messages aligned.
