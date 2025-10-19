---
subcommand: run-user-commands
type: enhancement
priority: high
scope: medium
labels: ["subcommand: run-user-commands", "type: enhancement", "priority: high", "area: env"]
---

# [run-user-commands] Implement remote env probe and caching

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Implement spec-compliant remote environment probing using `userEnvProbe` modes (none/loginInteractiveShell/interactiveShell/loginShell), with cache read/write in `--container-session-data-folder`, and merge with `--remote-env` flags and config.remoteEnv.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Probe remote env)

**From GAP.md Section:** 3.4 Remote Environment Probing (partial/missing)

### Expected Behavior
- Choose shell flags based on probe mode and detect env via `/proc/self/environ` or `printenv`.
- Cache result in `env-<probe>.json`; read cache before probing.
- Merge env maps: probed shell env + CLI remote-env + config.remoteEnv.

### Current Behavior
- Basic probing exists but no cache or proper merging.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/container_env_probe.rs` – Extend to support modes and caching.
- `crates/core/src/run_user_commands/env.rs` – New orchestrator for merging env maps.

#### Specific Tasks
- [ ] Add cache read/write (JSON) when `container_session_data_folder` provided.
- [ ] Implement probe mode switching to pick shell flags: `-lic`, `-ic`, `-lc`, `-c`.
- [ ] Merge env maps with precedence: CLI `--remote-env` overrides config.remoteEnv overrides probed shell env.

### 2. Data Structures
- Reuse existing env probe and maps.

### 3. Validation Rules
- [ ] None beyond CLI validation for `--remote-env` entries.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Item 5 - Environment Probing System with Caching
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Cache hit avoids probing.
- [ ] Cache miss writes file and subsequent runs hit cache.
- [ ] Env merge precedence correct.

### Integration Tests
- [ ] Verify probing via a known shell init adding a var.

## Acceptance Criteria
- [ ] Probe and cache work; merged env ready for lifecycle.

## References
- Specification: `docs/subcommand-specs/run-user-commands/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/run-user-commands/GAP.md` (§3.4)
