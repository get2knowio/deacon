---
subcommand: templates
type: enhancement
priority: low
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: low", "scope: small"]
---

# [templates] Cleanup Non-Spec Flags and Align Log-Level Handling

## Issue Type
- [x] Infrastructure/Cross-Cutting Concern

## Description
Remove or deprecate non-spec CLI flags for templates subcommands (e.g., `--force`, `--dry-run`, `--output`, `--username`, `--password-stdin`) and align log-level handling with spec (per-command `--log-level` where relevant or reuse global if already standardized across CLI). Update help text and docs accordingly.

## Specification Reference

**From GAP.md Section:** 1.1, 1.2, 1.4 Notes on non-spec flags and log-level differences

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/templates.rs` — Remove/deprecate flags; adjust help and parsing.
- Audit docs/examples to ensure no references to removed flags.

### 2. Validation Rules
- [ ] If deprecating temporarily, emit warnings; otherwise remove outright for parity.

## Testing Requirements
- [ ] Update CLI help tests; ensure absence of deprecated flags.

## Acceptance Criteria
- [ ] CLI aligns strictly with spec flags; docs updated.

## Definition of Done
- [ ] No lingering references to removed flags; CI green.
