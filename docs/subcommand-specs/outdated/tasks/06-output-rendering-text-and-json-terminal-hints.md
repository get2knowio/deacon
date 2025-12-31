# [outdated] Output Rendering (Text and JSON) with Terminal Hints

Labels:
- subcommand: outdated
- type: enhancement
- priority: medium
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Implement the rendering layer for `outdated`: human-friendly text table with columns `Feature | Current | Wanted | Latest`, and JSON mode that matches `OutdatedResult` exactly. Respect terminal size hints when provided for table formatting.

## Specification Reference

- From SPEC.md Section: §10. Output Specifications
- From GAP.md Section: 2.4 Phase 4: Output Formatting

### Expected Behavior
- Text mode: header + one row per versionable feature; undefined fields render as `-`.
- JSON mode: print `OutdatedResult` to stdout; indentation 2 spaces when TTY, compact otherwise.
- Feature column shows identifier without a version suffix (`:x.y.z` or `@sha256:...`).

### Current Behavior
- No rendering exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/deacon/src/commands/outdated.rs`
  - Add `render_outdated_table(result: &OutdatedResult, term: Option<(u32,u32)>) -> String`.
  - Add `render_outdated_json(result: &OutdatedResult, pretty: bool) -> String` or write directly to stdout.
- Optional helpers in a dedicated module/file if preferred (keep scope small).

#### Specific Tasks
- [ ] Strip version suffix from feature ID in the table’s first column.
- [ ] Replace None fields with `-` in text output.
- [ ] Pretty-print JSON when stderr is a TTY and/or when `log_format == text` (match repo conventions), otherwise compact.

### 2. Data Structures
- Input: `OutdatedResult` and terminal hints from CLI `--terminal-columns/rows`.

### 3. Validation Rules
- [ ] None beyond ensuring correct shapes and field names.

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: exact schema, stdout only.
- Theme 6 - Error Messages: not applicable here.

## Testing Requirements

### Unit Tests
- [ ] Text rendering replaces missing values with `-` and strips version suffix.
- [ ] JSON rendering matches expected schema and indentation rules.

### Integration Tests
- [ ] End-to-end with small fixture; compare stdout for both modes.

### Smoke Tests
- [ ] Add/update smoke to cover `--output-format text` and `--output-format json` with an empty config (header/empty map).

### Examples
- [ ] Add example output snippets in `examples/observability/json-logs/` if helpful; otherwise covered in examples task.

## Acceptance Criteria
- [ ] Text table and JSON output implemented and tested.
- [ ] Terminal hints are accepted; reasonable column widths are used (do not over-engineer).
- [ ] CI passes.

## Implementation Notes
- Use simple spacing or a minimal table helper; avoid heavy deps.
- Avoid logging to stdout; all logs go to stderr.

### Edge Cases to Handle
- No versionable features → header-only table or `{ features: {} }`.
- Very long feature IDs → truncate/clip sensibly if terminal hints provided.

### References
- SPEC: §10
- GAP: §2.4