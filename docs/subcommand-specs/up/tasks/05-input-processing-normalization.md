# [up] Implement input processing normalization pipeline

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: small -->

## Issue Type
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Implement the `parse_command_arguments(args) -> ParsedInput` normalization pipeline: array normalization (`toArray`), JSON parsing for `--additional-features`, path resolution for `--workspace-folder`, and consolidation of id labels and override/config paths.

## Specification Reference
- From SPEC.md Section: §3. Input Processing Pipeline
- From GAP.md Section: §2 Input Processing Pipeline – Missing

### Expected Behavior
As per pseudocode in SPEC:
```pseudocode
REQUIRE args.workspace_folder OR args.id_label
REQUIRE args.workspace_folder OR args.override_config
VALIDATE mount and remote-env formats
NORMALIZE arrays and JSON fields
RETURN ParsedInput with normalized arrays, booleans, resolved paths
```

### Current Behavior
- Basic clap parsing only; no normalization or JSON parsing for additional features; no providedIdLabels.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - add a normalization function returning a `ParsedInput` internal struct; call it early.
- `crates/deacon/src/cli.rs` - ensure flags provide raw values needed for normalization.

### 2. Data Structures
```rust
// ParsedInput { workspaceFolder?: String, providedIdLabels?: Vec<String>, addRemoteEnvs: Vec<String>, addCacheFroms: Vec<String>, additionalFeatures: Map<...>, overrideConfigFile?: URI, configFile?: URI }
```

### 3. Validation Rules
- [ ] Enforce requirements before heavy work.
- [ ] Exact error messages per SPEC.

### 4. Cross-Cutting Concerns
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: JSON parsing errors for additional-features; arrays normalization; path resolution.
- Integration: success path with multiple mounts/envs; failure path with invalid JSON.

## Acceptance Criteria
- Normalization implemented and covered by tests.
- Errors fail fast and are mapped to JSON when task 01 is completed.

## References
- `docs/subcommand-specs/up/SPEC.md` (§3)
- `docs/subcommand-specs/up/GAP.md` (§2)
