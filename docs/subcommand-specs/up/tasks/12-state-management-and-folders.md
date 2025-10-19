# [up] Add user/session data folders and caching hooks

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: small -->

## Issue Type
- [x] State Management
- [x] Missing CLI Flags

## Description
Implement `--user-data-folder` and `--container-session-data-folder` semantics for host-side and container session caching, including wiring for storing environment probe results and temporary artifacts as specified.

## Specification Reference
- From SPEC.md Section: §6. State Management
- From GAP.md Section: §5 State Management – Missing user/session data folders

### Expected Behavior
- Flags parsed and passed into options; used as base locations for caches and temp artifacts.
- If not provided, use sensible defaults per platform.

### Current Behavior
- No support for these flags.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add flags.
- `crates/deacon/src/commands/up.rs` - propagate into provision options and to subsystems that use caching.

### 2. Data Structures
```rust
// ProvisionOptions.persistedFolder, containerSessionDataFolder
```

### 3. Validation Rules
- [ ] Verify paths exist or create with proper permissions; error on failure.

### 4. Cross-Cutting Concerns
- [ ] Infrastructure Item 5 - Environment Probing System with Caching.

## Testing Requirements
- Unit: path resolution and creation.
- Integration: probe cache writes under session folder when feature lands.

## Acceptance Criteria
- Flags supported and paths used; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§6)
- `docs/subcommand-specs/up/GAP.md` (§5)
