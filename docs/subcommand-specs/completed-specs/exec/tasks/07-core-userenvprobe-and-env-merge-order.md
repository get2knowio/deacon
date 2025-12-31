---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Core: userEnvProbe Implementation and Environment Merge Order

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Description
Implement the user environment probing system supporting modes `none`, `loginInteractiveShell`, `interactiveShell`, and `loginShell`. Merge environments in order: shell probe → CLI `--remote-env` → config `remoteEnv`. Integrate optional caching using container data folders when available.

## Specification Reference
- From SPEC.md Section: §5 Core Execution Logic (probe_remote_env), §6 State Management
- From GAP.md Section: 5. Environment Handling Gaps, 8. Data Folder and Caching Gaps
- From PARITY_APPROACH.md: Infrastructure Item 5 (Environment Probing System with Caching)

### Expected Behavior
- Determine probe mode: config `userEnvProbe` or CLI `--default-user-env-probe` or default `loginInteractiveShell`.
- Collect environment via shell execution or `printenv` fallback; do not log secret values.
- Merge env: shell → CLI → config.
- Cache probe results when `container_data_folder`/`container_system_data_folder` are provided; cache key includes container ID and user.

### Current Behavior
- No env probe; no merge beyond CLI env; no caching.

## Implementation Requirements

### 1. Code Changes Required
- `crates/core/src/env_probe.rs` — New module implementing probe logic and optional caching.
- `crates/deacon/src/commands/exec.rs` — Integrate probe and merge logic before invoking docker exec.
- `crates/core/src/logging/redaction.rs` (if exists) — Ensure no secret values are logged; otherwise, add minimal masking for known keys.

### 2. Data Structures
```rust
pub struct EnvMerge {
    pub shell_env: std::collections::HashMap<String, String>,
    pub cli_env: std::collections::HashMap<String, String>,
    pub config_env: std::collections::HashMap<String, String>,
}
```

### 3. Validation Rules
- [ ] Respect enum values for probe mode; default as specified.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract (if emitting logs during probe)
- [ ] Theme 6 - Error Messages
- [ ] Infrastructure Item 5 - Environment Probing System with Caching

## Testing Requirements

### Unit Tests
- [ ] Probe mode selection precedence.
- [ ] Env merge order correctness (later overrides earlier).

### Integration Tests
- [ ] Verify PATH and other variables from the shell are present in the final exec environment.
- [ ] Verify caching is used on repeat invocations when folders are set (can be simulated).

## Acceptance Criteria
- [ ] Probe implemented for all modes; merge order enforced; optional caching; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§5–§6)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§5, §8)
- PARITY_APPROACH: `docs/PARITY_APPROACH.md` (Item 5)
