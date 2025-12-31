---
subcommand: templates
type: enhancement
priority: medium
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: medium", "scope: small"]
---

# [templates] Implement `--omit-paths` and Reserved File Exclusions in Apply

## Issue Type
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Add support for `--omit-paths <json>` to exclude specific files or directories when applying a template. Always omit reserved files `devcontainer-template.json`, `README.md`, and `NOTES.md` per spec. Support `/*` suffix to exclude entire directories.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (apply); §7 External System Interactions (extraction omit set)

**From GAP.md Section:** 2.3 Omit Paths; 2.4 Output Format mentions collecting file list

### Expected Behavior
- `--omit-paths` accepts JSON array of strings. Paths are relative to the template root.
- Paths ending with `/*` omit all contents of the directory. Exact filenames omit individual files.
- The reserved files are always omitted and do not require explicit configuration.

### Current Behavior
- Only `devcontainer-template.json` is omitted by core; no user-configurable omit support.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/templates.rs` — Parse and pass omit paths array to core via `ApplyOptions`.
- `crates/core/src/templates.rs` — Extend planning to skip files matching omit rules; include reserved paths by default.

#### Specific Tasks
- [ ] Add `omit_paths: Vec<String>` to `ApplyOptions` (already planned in Task 01).
- [ ] Implement matcher:
  - `path` equals relative path -> omit file.
  - `dir/*` prefix rule -> omit any file under `dir/`.
- [ ] Ensure reserved files are always excluded.
- [ ] Build the final list of applied files to feed output JSON `{ files: [...] }`.

### 2. Data Structures
```rust
pub struct ApplyOptions {
    pub omit_paths: Vec<String>,
    // ...existing fields
}
```

### 3. Validation Rules
- [ ] If `--omit-paths` is not an array of strings, validation fails in CLI with: "Invalid --omit-paths argument provided".

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract: ensure the `files` list excludes omitted entries.

## Testing Requirements

### Unit Tests (core)
- [ ] Rule matching for exact files and `/*` directories.
- [ ] Reserved files omitted regardless of user input.

### Integration Tests
- [ ] Apply with omit paths; verify omitted entries are not written and not listed in stdout JSON.

## Acceptance Criteria
- [ ] Omit paths behavior matches spec including reserved files.
- [ ] Tests pass and CI green.

## Definition of Done
- [ ] Implementation and tests committed; behavior documented in examples.
