# [up] Implement UID update and security options in container run

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] Core Logic Implementation

## Description
Complete `updateRemoteUserUID` flow and apply security-related options when running containers: `init`, `privileged`, `capAdd`, `securityOpt`, and proper entrypoint handling. Ensure host user mapping for supported platforms.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (Dockerfile/Image Flow step 2-3)
- From GAP.md Section: §4 Missing (1, 2)

### Expected Behavior
- Detect container user and host user; rebuild or patch image when UID update requested/required.
- Apply security options from merged configuration during `docker run`.
- EntryPoint respected/overridden per config.

### Current Behavior
- Partial UID update logic; security options and entrypoint incomplete.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - implement UID update build path; apply security opts in run.
- `crates/core/src/container.rs` - helpers for building temp Dockerfile for UID updates (if not present already).

### 2. Data Structures
```rust
// ContainerProperties.user, gid, shell, homeFolder
```

### 3. Validation Rules
- [ ] Fail clearly on unsupported platforms for UID remap.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: temp Dockerfile creation logic for UID update.
- Integration: run path applies security opts; UID update path chosen based on config.

## Acceptance Criteria
- UID update and security options implemented; tests green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§5)
- `docs/subcommand-specs/up/GAP.md` (§4)
