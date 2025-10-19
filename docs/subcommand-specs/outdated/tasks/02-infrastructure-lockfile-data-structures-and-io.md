# [Infrastructure] Lockfile Data Structures and I/O

Labels:
- subcommand: outdated
- type: infrastructure
- priority: high
- scope: medium

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: ___________

## Description
Introduce lockfile data structures and read helpers required by `outdated` (and future `upgrade`). The command must read an adjacent lockfile in order to compute the “current” version. This creates a reusable module in `core` with clean types and JSON (de)serialization.

## Specification Reference

- From SPEC.md Section: §4. Configuration Resolution; §6. State Management
- From GAP.md Section: 3. Missing Data Structures; 5.1 Lockfile I/O

### Expected Behavior
- Define `Lockfile` and `LockFeature` per DATA-STRUCTURES.
- Provide helpers to compute the adjacent lockfile path for a given config and read it when present.
- No writes performed by `outdated`.

### Current Behavior
- No lockfile types exist; no read helpers.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/core/src/lockfile.rs` (NEW)
  - Define `Lockfile` and `LockFeature` structs with `serde::{Serialize, Deserialize}`.
  - Implement `fn lockfile_path(config_path: &Path) -> PathBuf`.
  - Implement `async fn read_lockfile_adjacent_to(config_path: &Path) -> anyhow::Result<Option<Lockfile>>`.
- `crates/core/Cargo.toml`
  - Ensure `serde` and `serde_json` present with derive features.
- `crates/core/src/lib.rs`
  - `pub mod lockfile;`

#### Specific Tasks
- [ ] Implement data structures exactly matching DATA-STRUCTURES.md fields and names.
- [ ] Support both `.devcontainer-lock.json` and `devcontainer-lock.json` adjacent to the config file (prefer dotfile first).
- [ ] Robust JSON parsing with helpful context on errors.
- [ ] Unit tests for path resolution and JSON roundtrip.

### 2. Data Structures
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Lockfile {
    pub features: std::collections::HashMap<String, LockFeature>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct LockFeature {
    pub version: String,
    pub resolved: String,
    pub integrity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}
```

### 3. Validation Rules
- [ ] If file exists but is invalid JSON, return an error with context.
- [ ] If file does not exist, return Ok(None).

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: type shapes must match future `outdated` JSON expectations.
- Theme 6 - Error Messages: context strings should be actionable.

## Testing Requirements

### Unit Tests (core)
- [ ] `lockfile_path` chooses dotfile when both exist.
- [ ] `read_lockfile_adjacent_to` returns Some/None appropriately.
- [ ] Deserialize example JSON into `Lockfile` and re-serialize (roundtrip).

### Integration Tests
- [ ] Not required here; exercised via `outdated` command tests later.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] Public `lockfile` module available from core with types and I/O helpers.
- [ ] Unit tests pass and cover happy/error paths.
- [ ] CI passes: build, test, fmt, clippy.

## Implementation Notes
- Place helpers in `core` for reuse by `upgrade`.
- Do not write files in this task; `outdated` is read-only.

### Edge Cases to Handle
- Both lockfile names present → prefer dotfile.
- Broken JSON → error with file path context.

### References
- DATA-STRUCTURES: `docs/subcommand-specs/outdated/DATA-STRUCTURES.md`
- GAP: §3, §5.1
- SPEC: §4, §6