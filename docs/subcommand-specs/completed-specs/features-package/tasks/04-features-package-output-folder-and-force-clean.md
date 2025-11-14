---
subcommand: features-package
type: enhancement
priority: high
scope: small
labels: ["subcommand: features-package", "type: enhancement", "priority: high", "scope: small"]
---

# [features-package] Implement Output Folder Handling and --force-clean-output-folder

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement the `--force-clean-output-folder/-f` behavior and robust output folder handling: optionally remove pre-existing output content, then ensure directory exists prior to packaging.

## Specification Reference

**From SPEC.md Section:** “§3. Input Processing Pipeline” and “§6. State Management”

**From GAP.md Section:** “3. MISSING: `--force-clean-output-folder` Flag”

### Expected Behavior
```
if force_clean: rm -rf <output>
ensure_dir(<output>)
```

### Current Behavior
Output folder is created if missing; no clean option exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs`
  - In `execute_features_package`, before packaging:
    - If `force_clean`, remove the output directory if it exists (use std::fs::remove_dir_all with context; ignore if absent).
    - Create the directory with `create_dir_all` and context.

#### Specific Tasks
- [ ] Implement clean-then-create semantics with clear logs.
- [ ] Error messages:
  - On remove failure: "Failed to clean output folder." (with context)
  - On create failure: "Failed to create output folder." (with context)

### 2. Data Structures
N/A.

### 3. Validation Rules
- [ ] If output path exists but is a file, error: "Output path exists and is not a directory."

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 6 - Error Messages: standardized wording, add context via anyhow::Context.

## Testing Requirements

### Unit Tests
- [ ] Clean behavior: pre-populate with files; after run with `-f`, only new artifacts remain.
- [ ] Error when output path is a file.

### Integration Tests
- [ ] End-to-end run with `-f` applied.

### Smoke Tests
- [ ] Optional: add minimal smoke to assert directory recreated.

## Acceptance Criteria
- [ ] Force-clean implemented with robust error handling.
- [ ] Directory creation covers non-existent and existing cases.
- [ ] CI checks pass.

## References
- `docs/subcommand-specs/features-package/SPEC.md` (§3, §6)
- `docs/subcommand-specs/features-package/GAP.md` (Section 3)
- `docs/PARITY_APPROACH.md` (Theme 6)
