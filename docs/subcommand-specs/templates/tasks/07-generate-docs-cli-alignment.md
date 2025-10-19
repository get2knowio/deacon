---
subcommand: templates
type: enhancement
priority: medium
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: medium", "scope: small"]
---

# [templates] Align Generate-Docs CLI and Add GitHub Metadata Flags

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation

## Description
Align `templates generate-docs` with the spec: replace positional `path` with `--project-folder, -p <path>` (default `.`), and add `--github-owner` and `--github-repo` flags to enrich generated links. No structured stdout is required.

## Specification Reference

**From SPEC.md Section:** §2 CLI (generate-docs)

**From GAP.md Section:** 1.4 Generate-Docs Command gaps

### Expected Behavior
- Flags: `--project-folder, -p`, `--github-owner`, `--github-repo`, `--log-level`.
- Writes docs at canonical locations in the project; no stdout contract beyond logs.

### Current Behavior
- Positional path and a non-spec `--output` flag.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/templates.rs` — Adjust flags and help text.
- `crates/core/src/templates_docs.rs` (if exists) — Accept new flags and propagate into doc generation logic.

### 2. Validation Rules
- [ ] Defaults to `.` when `--project-folder` not provided.

## Testing Requirements
- [ ] CLI parsing unit tests.
- [ ] Golden-file or snapshot tests optional for doc output locations.

## Acceptance Criteria
- [ ] Flags align with spec; tests pass.

## Definition of Done
- [ ] Updated help strings and README snippets.
