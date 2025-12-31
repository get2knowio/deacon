# [up] Implement runtime behavior flags

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation

## Description
Add and wire runtime behavior flags that influence container creation and lifecycle: `--build-no-cache`, `--expect-existing-container`, `--workspace-mount-consistency`, `--gpu-availability`, `--default-user-env-probe`, `--update-remote-user-uid-default`.

## Specification Reference
**From SPEC.md Section:** §2. Command-Line Interface (Runtime behavior)

**From GAP.md Section:** §1 Missing Flags (runtime behavior), §5 State Management (probe caching)

### Expected Behavior
- Flags parsed and available in options; behavior adjusted accordingly in setup and execution.
- `--expect-existing-container` errors if container absent.
- `--build-no-cache` passed through to build pipeline.
- Mount consistency applied to workspace mount defaults.
- GPU availability gating (warn vs error per spec note).
- Default user env probe mode influences environment probing (infra may be separate; stub with clear error if not implemented, no silent noop).
- UID update default applied to user mapping logic.

### Current Behavior
- Missing all of the above flags; partial UID update logic exists but not fully controlled.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add flags with enums.
- `crates/deacon/src/commands/up.rs` - carry into `ProvisionOptions` equivalent; enforce `expect-existing`.
- `crates/core/src/container.rs` - apply mount consistency, uid mapping policy.

### 2. Data Structures
```rust
// ProvisionOptions.workspaceMountConsistency: 'consistent'|'cached'|'delegated'
// ProvisionOptions.gpuAvailability: 'all'|'detect'|'none'
// ProvisionOptions.defaultUserEnvProbe: enum
// ProvisionOptions.updateRemoteUserUIDDefault: 'never'|'on'|'off'
```

### 3. Validation Rules
- [ ] `--expect-existing-container` must fail fast when not found.
- [ ] Emit warnings when GPU requested but unsupported per SPEC §9.

### 4. Cross-Cutting Concerns
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.
- [ ] Infrastructure Item 5 - Environment Probing System (note: may be separate; fail with clear error if mode requires unimplemented probe).

## Testing Requirements
- Unit: enum parsing and policy mapping.
- Integration: expect-existing path success/fail; build-no-cache propagation.

## Acceptance Criteria
- Flags available and effective.
- Clear errors/warnings per spec.
- CI checks pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2)
- `docs/subcommand-specs/up/GAP.md` (§1, §9)
