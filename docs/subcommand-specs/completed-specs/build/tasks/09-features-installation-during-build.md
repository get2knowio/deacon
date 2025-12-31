---
subcommand: build
type: enhancement
priority: high
scope: large
---

# [build] Integrate Features installation during build

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Generate feature installation scripts and Dockerfile layers during build so that Features are applied to the resulting image in Dockerfile and image-reference modes. Provide build contexts and security options as needed. Support experimental lockfile flags.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic; §6 State Management (lockfiles)

**From GAP.md Section:** 6.1 Feature Installation Workflow; 5.2 Build Context Management

### Expected Behavior
- Features merged from config/CLI are resolved and converted into generated Dockerfile content with scripts and env files written under a persisted folder.
- Build uses empty context plus `--build-context` entries pointing to generated feature content.
- Optional lockfile is written/checked when experimental flags are set.

### Current Behavior
- Only feature merging occurs; no install scripts or layers generated/applied.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/features/*` – add functions to plan feature install (order, env, scripts), write artifacts to cache folder, and compute build args/contexts.
- `crates/deacon/src/commands/build.rs` – consume feature build info to assemble docker args: `--build-context`, `--security-opt`, `--build-arg`, `-f` pointing to generated Dockerfile, `--target` override, etc.
- `crates/core/src/lockfile/*` – implement basic write/validate for experimental flags if not present.

#### Specific Tasks
- [ ] Implement feature planning to `ImageBuildOptions` per DATA-STRUCTURES.md.
- [ ] Write generated files into persisted folder under workspace `.devcontainer/`.
- [ ] Wire contexts and security opts.
- [ ] Integrate experimental lockfile and frozen lockfile behaviors.

### 2. Data Structures
Use `ImageBuildOptions` from DATA-STRUCTURES.md as the contract between planning and build.

### 3. Validation Rules
- Ensure local/disallowed feature sources trigger errors per spec (see test suite references).

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 2 - CLI Validation
- [x] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Plan-to-artifacts tests for feature resolution.

### Integration Tests
- [ ] Build with a minimal feature (fixtures/features/minimal) and assert artifacts and arg assembly.

### Smoke Tests
- [ ] Ensure smoke still passes in Dockerless environments.

## Acceptance Criteria
- [ ] Features are applied to built images via generated layers.
- [ ] Build contexts and security opts wired.
- [ ] Lockfile flags honored.
- [ ] CI checks pass.

## Definition of Done
- [ ] Feature installation pipeline integrated during build.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§5, §6)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§6.1, §5.2)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
