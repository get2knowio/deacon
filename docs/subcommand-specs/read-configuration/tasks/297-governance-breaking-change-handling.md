---
issue: 297
title: "[read-configuration] Governance: Breaking change handling and migration notes for output structure"
---

## Issue Body

## Issue Type
- [x] Other: Governance/Docs

## Parent Issue
Tracks: #286 (tracking issue)

## Description
Coordinate the break
ing change from emitting raw configuration to emitting the specified output object. Optionally add a temporary `--legacy-output` flag, update docs, and note migration in release notes.

## Specification Reference
**From GAP.md Section:** §19 Breaking Changes Required; §16 Migration Notes

### Expected Behavior
- Document breaking change
- Optionally gate with temporary `--legacy-output` for one release

## Implementation Requirements

### 1. Code/Docs Changes Required
- `crates/deacon/src/commands/read_configuration.rs` — Optional: wire `--legacy-output`
- `README.md`, `docs/CLI-SPEC.md` references, and examples
- Release notes labeling and PR description per `.github/copilot-instructions.md`

## Acceptance Criteria
- [ ] Migration documented
- [ ] Optional flag decision recorded

## References
- `docs/subcommand-specs/read-configuration/GAP.md` (§19)
