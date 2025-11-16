---
subcommand: features-package
type: enhancement
priority: medium
scope: small
labels: ["subcommand: features-package", "type: enhancement", "priority: medium", "scope: small"]
---

# [features-package] JSON Output Contract and Text Separation

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Ensure compliance with Theme 1 for output separation: JSON (when `--json` is set) must be the only content on stdout; human-readable logs should go to stderr. Define minimal JSON result structure for packaging summary.

## Specification Reference

**From SPEC.md Section:** “§10. Output Specifications”

**From GAP.md Section:** Noted as missing clarity around text vs JSON and collection summary.

### Expected Behavior
- When `--json`: print only JSON summary to stdout, nothing else.
- Logs like "Created package: ..." go to stderr.
- Suggested JSON summary shape (from DATA-STRUCTURES.md Packaging Summary concept):
  ```json
  { "mode": "single|collection", "outputDir": "./output", "artifacts": ["<id>.tgz", "devcontainer-collection.json"] }
  ```

### Current Behavior
- Text printed to stdout via `println!`; JSON also printed via println in current implementation.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs`
  - Update output paths to use `eprintln!` for human logs when `--json` is set.
  - Build and print a JSON summary object only once on stdout.

#### Specific Tasks
- [ ] Add helper `fn print_json<T: Serialize>(t: &T)` and route logs to stderr when json=true.
- [ ] Ensure no extra newlines/whitespace beyond pretty JSON.

### 2. Data Structures
Use the conceptual Packaging Summary from DATA-STRUCTURES.md.

### 3. Validation Rules
N/A.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Verify stdout contains only JSON in `--json` runs (capture stdout/stderr in tests).

### Integration Tests
- [ ] End-to-end run with `--json` asserts separation.

### Smoke Tests
- [ ] Ensure smoke is compatible and captures logs appropriately.

## Acceptance Criteria
- [ ] Clean separation between JSON and text outputs.
- [ ] Tests passing; CI green.

## References
- `docs/PARITY_APPROACH.md` (Theme 1)
- `docs/subcommand-specs/features-package/DATA-STRUCTURES.md` (Packaging Summary)
