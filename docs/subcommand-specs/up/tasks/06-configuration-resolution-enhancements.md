# [up] Enhance configuration resolution per spec

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Core Logic Implementation
- [x] Error Handling

## Description
Align configuration resolution with SPEC: validate devcontainer filename, implement `findContainerAndIdLabels`, ensure pre-substitution timing, disallowed Features validation, and complete merging with image metadata.

## Specification Reference
- From SPEC.md Section: §4. Configuration Resolution
- From GAP.md Section: §3 Missing (filename validation, id labels, substitution timing, disallowed features, metadata merge)

### Expected Behavior
- Error if config filename not `devcontainer.json` or `.devcontainer/devcontainer.json` when passed explicitly.
- Fallback discovery when only `override-config` present.
- `findContainerAndIdLabels` returns effective id labels when not provided.
- Variable substitution applied with correct ordering.
- Disallowed feature ids cause a user error including offending id.
- Merge image labels metadata into runtime config.

### Current Behavior
- Partial resolution; missing validations and metadata merge.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/read_configuration.rs` - add filename validation; refine substitution timing.
- `crates/deacon/src/commands/up.rs` - integrate `findContainerAndIdLabels` and metadata merge; surface clear errors.
- `crates/deacon/src/commands/features.rs` - add disallowed features check if features logic resides there.

### 2. Data Structures
```rust
// ResolverParameters, DockerResolverParameters (see DATA-STRUCTURES.md)
// ensure these are respected or mapped if already present in code
```

### 3. Validation Rules
- [ ] Exact filename validity error message.
- [ ] Disallowed feature error includes offending feature id.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [ ] Theme 1 - JSON Output: map errors accordingly once task 01 is done.

## Testing Requirements
- Unit: filename validation, disallowed features check.
- Integration: metadata merge end-to-end; id labels discovery vs provided.

## Acceptance Criteria
- Resolution matches SPEC; tests pass; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§4)
- `docs/subcommand-specs/up/GAP.md` (§3)
