---
subcommand: exec
type: enhancement
priority: high
scope: medium
---

# [exec] Core: Image Metadata Merge and Container-Aware Variable Substitution

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Testing & Validation
- [x] Error Handling

## Description
Implement reading devcontainer image metadata labels from the running container, merge them with the resolved config, and apply container-aware variable substitution using the container environment. This unlocks `remoteUser`, `remoteEnv`, and `${containerEnv:VAR}` semantics.

## Specification Reference
- From SPEC.md Section: §4 Configuration Resolution (Merge Algorithm, Variable Substitution)
- From GAP.md Section: 3. Configuration Resolution Gaps, 4. Core Execution Logic Gaps

### Expected Behavior
- Inspect container for devcontainer-related labels to produce `imageMetadata`.
- Merge `config` with `imageMetadata` → `mergedConfig`.
- Apply `containerSubstitute(platform, configPath, containerEnv, mergedConfig)` to produce updated config used by `exec`.

### Current Behavior
- No metadata reading; no merge; no container-aware substitution.

## Implementation Requirements

### 1. Code Changes Required
- `crates/core/src/config/merge.rs` — Add `merge_configuration(config, image_metadata)`.
- `crates/core/src/docker/inspect.rs` — Read labels and environment from container; map into structured metadata.
- `crates/core/src/substitute.rs` — Implement `container_substitute(...)` supporting `${env:VAR}`, `${localEnv:VAR}`, `${workspaceFolder}`.
- `crates/deacon/src/commands/exec.rs` — Integrate flow before building env and CWD.

### 2. Data Structures
```rust
pub struct ContainerProperties {
    pub env: std::collections::HashMap<String, String>,
    pub user: String,
    pub remoteWorkspaceFolder: Option<String>,
    pub homeFolder: String,
}
```

### 3. Validation Rules
- [ ] If metadata parsing fails, surface a clear error with context; do not silently ignore per Prime Directives (no silent fallbacks).

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages
- [ ] Theme 2 - Validation before exec

## Testing Requirements

### Unit Tests
- [ ] Merge behavior: config values overridden by image metadata where specified.
- [ ] Substitution correctness for `${env:VAR}`, `${localEnv:VAR}`, `${workspaceFolder}`.

### Integration Tests
- [ ] End-to-end: with container labels present, verify `remoteUser` and `remoteEnv` are applied after merge and substitution.

## Acceptance Criteria
- [ ] Metadata merge and substitution implemented and covered by tests; CI green.

## References
- SPEC: `docs/subcommand-specs/exec/SPEC.md` (§4)
- GAP: `docs/subcommand-specs/exec/GAP.md` (§3–§4)
