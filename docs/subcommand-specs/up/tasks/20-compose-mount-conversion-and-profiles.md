# [up] Implement compose-specific mount conversion and profiles support

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] External System Interactions
- [x] Core Logic Implementation

## Description
For compose-based configurations, implement conversion of additional `--mount` flags into compose volumes entries and support profiles selection when calling `docker compose up -d`. Ensure `.env`-driven profiles are respected.

## Specification Reference
- From SPEC.md Section: §7. External System Interactions (Compose)
- From GAP.md Section: §18 Compose-Specific Functionality – Missing (mount conversion, profiles)

### Expected Behavior
- Additional mounts mapped to compose volumes via `convertMountToVolume`.
- Profiles applied when provided in config or env, propagated to compose commands.

### Current Behavior
- Basic compose support exists; conversion and profiles incomplete.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - when compose flow chosen, convert mounts and pass profiles to compose up.
- `crates/core/src/container.rs` or compose utils - implement `convertMountToVolume` per DATA-STRUCTURES.md.

### 2. Data Structures
```rust
// ComposeContext { composeFiles, envFile, projectName, serviceName }
```

### 3. Validation Rules
- [ ] Validate mount types supported for compose.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: mount conversion function.
- Integration: compose project receives extra volumes and honors profiles.

## Acceptance Criteria
- Compose mount conversion and profiles supported; tests pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§7)
- `docs/subcommand-specs/up/GAP.md` (§18)
