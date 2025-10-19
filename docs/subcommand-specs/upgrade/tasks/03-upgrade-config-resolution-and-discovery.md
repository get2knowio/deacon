---
subcommand: upgrade
type: enhancement
priority: high
scope: medium
---

# [upgrade] Config Resolution and Discovery Wiring

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Integrate configuration discovery and load for `upgrade`. Use the existing config loader and auto-discovery rules. Ensure error messages match spec and the effective config path is resolved to an absolute path before downstream steps.

## Specification Reference

**From SPEC.md Section:** §4. Configuration Resolution

**From GAP.md Section:** 2.2 Configuration Resolution

### Expected Behavior
- If `--config` given: resolve to absolute path and load directly.
- Else: probe `<workspace>/.devcontainer/devcontainer.json`, then `<workspace>/.devcontainer.json`.
- On failure: "Dev container config (...) not found." or "... must contain a JSON object literal."
- Apply variable substitution per existing loader.

### Current Behavior
- No upgrade-specific integration exists.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Add `resolve_config_path` and `load_config` steps
  - Use existing core config APIs (follow read-configuration patterns)

#### Specific Tasks
- [ ] Implement discovery order and absolute path derivation
- [ ] Integrate substitution-enabled loader
- [ ] Map errors to exact messages per spec
- [ ] Add tracing spans: `config.resolve`

### 2. Data Structures
- Reuse `DevContainerConfig` from core; only `features` subset is needed downstream.

### 3. Validation Rules
- [ ] Validate path existence and type (file)
- [ ] Error messages per spec

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Discovery preference order test
- [ ] Error when not found
- [ ] Error on non-object JSON

### Integration Tests
- [ ] Load config with variable substitution

### Smoke Tests
- [ ] Add a minimal workspace fixture and assert discovery behavior

### Examples
- [ ] N/A here; examples in later tasks

## Acceptance Criteria
- [ ] Config discovery and load works per spec
- [ ] Exact error messages
- [ ] Tracing spans present
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§4)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§2.2)
