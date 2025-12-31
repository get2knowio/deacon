---
subcommand: set-up
type: enhancement
priority: medium
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: medium", "area: variables"]
---

# [set-up] Implement container-side variable substitution (${containerEnv:VAR})

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Extend the variable substitution system to perform a second pass using the live container environment, resolving `${containerEnv:VAR}` placeholders in both base and merged configurations. Cache probed environment under `--container-session-data-folder`.

## Specification Reference

**From SPEC.md Section:** §4 Configuration Resolution (Variable Substitution)

**From GAP.md Section:** 1.6 Variable Substitution (Partial)

### Expected Behavior
- Probe container env via shell server using configured `userEnvProbe` (from merged config or default).
- Apply substitution to config and merged config with `${containerEnv:VAR}` values.
- Respect case-insensitive lookup on Windows-like contexts (note: inside container POSIX assumed).
- Errors for malformed placeholders; missing variable name is an error pointing to the config file if available.

### Current Behavior
- Only `${localEnv:VAR}` supported; no container-side substitution.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/variable.rs` – Add `container_env_substitute(...)` and `${containerEnv:...}` support.
- `crates/core/src/container_env_probe.rs` – Ensure probe results can be cached/read via `--container-session-data-folder`.
- `crates/core/src/setup/substitution.rs` – New: orchestrate two-pass substitution for set-up.

#### Specific Tasks
- [ ] Implement parser support for `${containerEnv:VAR}`.
- [ ] Implement substitution pass that takes a map of env from the container.
- [ ] Integrate with probe logic; write/read cache when folder provided.
- [ ] Unit tests for nested substitution and error on missing variable name.

### 2. Data Structures
- Reuse existing variable engine data types; no new public types required.

### 3. Validation Rules
- [ ] Detect and error on `${containerEnv:}` (empty var name) with a message referencing config file path when known.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 8 - Two-Phase Variable Substitution
- [x] Theme 1 - JSON Output Contract (results printed later)

## Testing Requirements

### Unit Tests
- [ ] Substitutes `${containerEnv:TEST_CE}` correctly when present.
- [ ] Error on empty name.
- [ ] Cache read/write round-trip via session folder.

### Integration Tests
- [ ] End-to-end with container providing TEST_CE variable.

### Smoke Tests
- [ ] None for this issue.

### Examples
- [ ] None for this issue; covered in orchestration task.

## Acceptance Criteria
- [ ] Substitution pass works and is covered by tests.
- [ ] No clippy warnings.

## Implementation Notes
- Avoid assuming bash; use POSIX sh for env dumps.

### Edge Cases to Handle
- Very large environment sizes; ensure memory-safe handling.

## Definition of Done
- [ ] `${containerEnv:...}` supported end-to-end for set-up substitution phase.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§4)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.6)
