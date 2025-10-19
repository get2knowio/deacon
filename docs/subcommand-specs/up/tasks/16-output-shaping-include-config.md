# [up] Implement include-configuration and include-merged-configuration flags

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: small -->

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation

## Description
Support output shaping flags `--include-configuration` and `--include-merged-configuration` and embed the corresponding JSON blobs into the success result when requested, matching the exact schema in Data Structures.

## Specification Reference
- From SPEC.md Section: §10. Output Specifications
- From GAP.md Section: §9 Output Specifications – Missing (Include configuration output)

### Expected Behavior
- When flags are set, the success JSON includes `configuration` and/or `mergedConfiguration` objects populated from the resolved config structures.

### Current Behavior
- Flags missing; output does not include these fields.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add flags with help text.
- `crates/deacon/src/commands/up.rs` - conditionally serialize the configuration blobs in success output.

### 2. Data Structures
```rust
// UpSuccessResult includes optional configuration and mergedConfiguration
```

### 3. Validation Rules
- [ ] None beyond JSON contract and flags parsing.

### 4. Cross-Cutting Concerns
- [x] Theme 1 - JSON Output Contract (stdout only).

## Testing Requirements
- Unit: serialization when flags set vs unset.
- Integration: run with flags and assert presence of blobs.

## Acceptance Criteria
- Flags supported; output includes blobs when requested; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§10)
- `docs/subcommand-specs/up/GAP.md` (§9)
