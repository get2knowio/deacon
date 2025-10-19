---
subcommand: upgrade
type: enhancement
priority: medium
scope: medium
---

# [upgrade] Feature Pinning via Hidden Flags (Config Edit)

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Implement the optional pre-lockfile config edit triggered by `--feature` and `--target-version`. Replace the first matching feature key by base ID (strip tag/digest) with a version-pinned key, write back to file, then re-load the config for subsequent resolution.

## Specification Reference

**From SPEC.md Section:** §5. Core Execution Logic (Phase 2: Optional config edit)

**From GAP.md Section:** 2.3 Feature Version Pinning, 10.1–10.3 Design Decisions

### Expected Behavior
- When both flags present: log "Updating '<feature>' to '<target_version>' in devcontainer.json".
- Perform text-based key replacement matching by base identifier (up to last `:` or `@`).
- Replace only the first occurrence; if not found, log "No Features found in '<path>'" and continue.
- Re-read config after edit.
- Edits persist even in `--dry-run` mode.

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Add `fn get_feature_id_without_version(id: &str) -> String`
  - Add `async fn update_feature_version_in_config(config_path: &Path, feature_id: &str, target_version: &str) -> anyhow::Result<bool>`
  - Implement text replacement and write file back atomically
  - Add logs and re-load config

#### Specific Tasks
- [ ] Implement base ID extraction by removing last `:[^/]+` or `@[^/]+`
- [ ] Replace the first matching key string only
- [ ] Log warnings if replacement count != 1
- [ ] Re-read config on success path

### 2. Data Structures

From DATA-STRUCTURES.md:
```rust
fn get_feature_id_without_version(id: &str) -> &str;
```

### 3. Validation Rules
- [ ] Pairing validated in CLI; assume both flags present here

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Base ID extraction cases (tag, digest, none)
- [ ] Replacement only once; multiple matching keys → only first replaced
- [ ] No match → returns false and logs

### Integration Tests
- [ ] End-to-end pin + dry-run prints lockfile for pinned ID

### Smoke Tests
- [ ] None

### Examples
- [ ] To be added in examples task

## Acceptance Criteria
- [ ] Hidden flags perform config edit as specified
- [ ] Re-load config post-edit
- [ ] Dry-run still edits config, only prints lockfile JSON (no write)
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§5, Design Decisions)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§2.3, §10)
