# [up] Implement Docker/Compose path flags and container data folders

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: small -->

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation

## Description
Add support for `--docker-path`, `--docker-compose-path`, `--container-data-folder`, and `--container-system-data-folder` flags. These enable using non-default runtimes and customizing data directories. Ensure the values flow into runtime resolution and relevant filesystem operations.

## Specification Reference
- From SPEC.md Section: §2. Command-Line Interface (Docker/Compose paths and data)
- From GAP.md Section: §1 Missing Flags – Docker/Compose paths and data

### Expected Behavior
- CLI accepts the four flags.
- Path flags override runtime executable locations when spawning docker/compose commands.
- Data folder flags affect location for container-related data as specified (e.g., temp build artifacts, caches) if used by `up` path; otherwise stored for future steps.

### Current Behavior
- Missing; runtime path selection is fixed to defaults.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add flags and help text.
- `crates/deacon/src/commands/up.rs` - propagate path overrides into runtime abstraction and provision options; wire container data folder paths where applicable.
- `crates/core/src/container.rs` or runtime module - add optional overrides for docker and compose command paths.

### 2. Data Structures
```rust
// ProvisionOptions.dockerPath?: string
// ProvisionOptions.dockerComposePath?: string
// ProvisionOptions.containerDataFolder?: string
// ProvisionOptions.containerSystemDataFolder?: string
```

### 3. Validation Rules
- [ ] Validate path existence/executability for docker/compose when provided; error otherwise with exact message.
- [ ] Validate folder paths exist or create as needed; error on failure.

### 4. Cross-Cutting Concerns
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: path validation and normalization.
- Integration: override docker path in a test harness (mock) and assert it is used; data folder paths stored and referenced.

## Acceptance Criteria
- Flags added and used where relevant; tests pass; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2)
- `docs/subcommand-specs/up/GAP.md` (§1)
