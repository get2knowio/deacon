---
subcommand: exec
type: enhancement
priority: medium
scope: medium
---

# [exec] Implement CLI Flags: Docker Tooling & Data Folders

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern

## Description
Add flags to control Docker/Compose paths and data folders used by the CLI and inside the container. These enable diverse setups and optional caching locations.

## Specification Reference
- From SPEC.md Section: §2 Command-Line Interface, §6 State Management
- From GAP.md Section: 1. CLI Interface Gaps, 8. Data Folder and Caching Gaps, 9. Docker/Tooling Configuration Gaps

### Expected Behavior
- Add flags:
  - `--docker-path <PATH>` (default `docker`)
  - `--docker-compose-path <PATH>` (auto-detect when not provided)
  - `--user-data-folder <PATH>`
  - `--container-data-folder <PATH>`
  - `--container-system-data-folder <PATH)`
- Values are passed through to docker configuration and later used by env probe caching (see related task 07).

### Current Behavior
- No custom docker/compose paths; only global `--user-data-folder`; no container data folder flags.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/exec.rs` — Add flags and map to `ParsedInput` fields.
- `crates/core/src/docker/*.rs` (or equivalent) — Accept docker path overrides.

### 2. Data Structures
```rust
pub struct ParsedInput {
    pub docker_path: Option<String>,
    pub docker_compose_path: Option<String>,
    pub user_data_folder: Option<String>,
    pub container_data_folder: Option<String>,
    pub container_system_data_folder: Option<String>,
}
```

### 3. Validation Rules
- [ ] None beyond basic path presence; defer to downstream when launching tools.

### 4. Cross-Cutting Concerns
- [ ] Theme 2 - CLI Validation
- [ ] Theme 6 - Error Messages

## Testing Requirements
- [ ] Unit: parse flags and verify in `ParsedInput`.
- [ ] Integration: verify overridden docker path is used by docker runner (mockable).

## Acceptance Criteria
- [ ] Flags implemented and values wired through to runtime configuration.
- [ ] Tests pass and CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§2, §6)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§1, §8, §9)
