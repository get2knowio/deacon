---
subcommand: build
type: enhancement
priority: high
scope: medium
---

# [build] BuildKit output control and cache options

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Implement BuildKit-specific behavior for `--push` and `--output`, including the implied `--load` when neither is specified. Add support for `--cache-from` and `--cache-to` along with build contexts and security options required by feature installation (to be populated in a later issue).

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic; §7 External System Interactions

**From GAP.md Section:** 3.3 BuildKit Output Control; 5.1 Push to Registry; 5.2 Build Context Management

### Expected Behavior
- When BuildKit is enabled:
  - If `--push` present → include `buildx build --push`.
  - Else if `--output <spec>` present → include `--output <spec>`.
  - Else include `--load` to load image into local daemon.
- Respect `--cache-from` additions and optional `--cache-to`.

### Current Behavior
- BuildKit detection exists but flags are not implemented; no automatic `--load`.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/build.rs` – in `execute_docker_build`, when `use_buildkit` is true, switch to `buildx build` and add the correct combination of `--push`/`--output`/`--load`, as well as cache flags and any build contexts and security opts (placeholders if not yet available).
- `crates/deacon/src/commands/build.rs` – ensure non-BuildKit path still uses classic `docker build`.

#### Specific Tasks
- [ ] Detect and use `docker buildx build` when BuildKit is active.
- [ ] Add `--push` or `--output` or `--load` per spec.
- [ ] Thread `--cache-from` and `--cache-to` correctly.
- [ ] Prepare hooks for adding `--build-context` and `--security-opt` in future feature integration.

### 2. Data Structures
N/A.

### 3. Validation Rules
- Already covered by validation issue; ensure runtime assertions remain consistent.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract: result should still conform.
- [x] Theme 2 - CLI Validation: relies on prior issue for gating.

## Testing Requirements

### Unit Tests
- [ ] Assert argument vector includes the correct buildx flags under various combinations.

### Integration Tests
- [ ] Simulated or mocked runs to verify branch selection and argument assembly.

### Smoke Tests
- [ ] No change unless we expose behavior in smoke; keep tolerant to Docker-unavailable environments.

## Acceptance Criteria
- [ ] BuildKit path correctly assembles arguments; legacy path unaffected.
- [ ] `--load` implied when neither `--push` nor `--output` specified.
- [ ] CI checks pass.

## Definition of Done
- [ ] Behavior implemented and tests added.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§5, §7)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§3.3, §5.1, §5.2)
