# [up] Implement mounts/env/cache/build flags with validation

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: medium -->

## Issue Type
- [x] Missing CLI Flags
- [x] Testing & Validation
- [x] Core Logic Implementation

## Description
Implement additional flags: `--mount` (repeatable, regex-validated), `--remote-env` (repeatable name=value), `--cache-from`, `--cache-to`, `--buildkit` (auto|never). Normalize arrays and feed into build/run pipelines.

## Specification Reference
- From SPEC.md Section: §2. Command-Line Interface (Additional mounts/env/cache/build)
- From GAP.md Section: §1 Missing Flags; §2 Input Processing Pipeline – Missing normalization and JSON parsing

### Expected Behavior
- `--mount` validated against `type=(bind|volume),source=([^,]+),target=([^,]+)(,external=(true|false))?`.
- `--remote-env` validated as `<name>=<value>`.
- `--cache-from` repeatable, `--cache-to` string; `--buildkit` controls build strategy.
- Arrays normalized; passed to ProvisionOptions/additional structures.

### Current Behavior
- Flags missing; no normalization helpers.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/cli.rs` - add flags with validators.
- `crates/deacon/src/commands/up.rs` - parse/normalize into `ParsedInput`-like and map to runtime/build.
- `crates/deacon/src/commands/build.rs` - if shared build path, align BuildKit and cache options handling.

### 2. Data Structures
```rust
// Mount { type, source, target, external? }
// ProvisionOptions.additionalMounts: Mount[]
// ProvisionOptions.remoteEnv: map<string,string>
// ProvisionOptions.additionalCacheFroms: string[]
// ProvisionOptions.useBuildKit: 'auto'|'never'
```

### 3. Validation Rules
- [ ] Enforce regex for `--mount`.
- [ ] Enforce `name=value` for `--remote-env`.
- [ ] Exact error messages per SPEC.

### 4. Cross-Cutting Concerns
- [x] Theme 2 - CLI Validation.
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: mount parser, remote-env parser, cache flags.
- Integration: up with extra mount/env; ensure propagated to container run/build.

## Acceptance Criteria
- Flags implemented and validated.
- Options flow into container run/build.
- CI checks pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§2)
- `docs/subcommand-specs/up/GAP.md` (§1, §2)
