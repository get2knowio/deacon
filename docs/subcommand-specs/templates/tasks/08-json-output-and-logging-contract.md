---
subcommand: templates
type: enhancement
priority: medium
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: medium", "scope: small", "cross-cutting"]
---

# [templates] Enforce JSON Output Contract and Logging Separation

## Issue Type
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Description
Ensure all templates subcommands comply with JSON output conventions: JSON to stdout only, human-readable logs to stderr, exact structures per DATA-STRUCTURES.md, and no trailing whitespace. Apply consistent error messages and exit codes.

## Specification Reference

**From SPEC.md Section:** §10 Output Specifications

**From PARITY_APPROACH.md Theme 1:** JSON Output Contract Compliance

### Expected Behavior
- apply -> `{ files: string[] }`
- publish -> `{ [templateId]: { publishedTags?: string[], digest?: string, version?: string } }`
- metadata -> `Template` JSON or `{}`
- generate-docs -> no structured stdout
- All logs to stderr; no extra newlines in JSON

## Implementation Requirements

### 1. Code Changes Required
- Audit `crates/deacon/src/commands/templates.rs` for stdout/stderr separation.
- Add small utility to write compact JSON to stdout without trailing newline if necessary.

### 2. Validation Rules
- [ ] On errors, return appropriate exit code and ensure stdout contract (e.g., `{}` for metadata missing).

## Testing Requirements
- [ ] Integration tests asserting stdout vs stderr separation and exact JSON shapes.

## Acceptance Criteria
- [ ] All templates subcommands adhere to Theme 1; CI green.

## Definition of Done
- [ ] PR includes tests covering stdout/stderr separation.
