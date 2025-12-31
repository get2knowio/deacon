# [up] Implement Features-driven image extension during up

<!-- Suggested labels: subcommand: up, type: enhancement, priority: high, scope: large -->

## Issue Type
- [x] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern

## Description
Extend base images with Features during `up` when required by the configuration or CLI. Build extended images using BuildKit, construct the feature contexts, and merge resulting metadata and provenance. Respect `--skip-feature-auto-mapping` and experimental lockfile flags if feasible or emit clear "Not implemented yet" errors without silent fallback.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (Dockerfile/Image Flow step 1)
- From GAP.md Section: §4 Missing (4, 6); §17 Features System Integration – Missing

### Expected Behavior
- When Features present, extend image before container run; cache contexts; merge labels; track provenance.
- Respect CLI-provided features and order overrides.
- If lockfile flags provided but infra not ready, error explicitly per Prime Directives.

### Current Behavior
- No image extension with Features.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - integrate feature planning and build path into Dockerfile/Image flow.
- `crates/deacon/src/commands/features.rs` - reuse planning, install order, and metadata helpers.
- `crates/deacon/src/commands/build.rs` - align BuildKit/cache options if shared.

### 2. Data Structures
```rust
// ProvisionOptions.additionalFeatures: map<string, string|bool|map<...>>
// LifecycleHooksInstallMap accumulation from feature metadata
```

### 3. Validation Rules
- [ ] Error on disallowed Features; map error to JSON.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.
- [ ] Infrastructure Item 8 - Two-Phase Variable Substitution (may be needed for some feature options).

## Testing Requirements
- Unit: feature context construction from metadata.
- Integration: config with a minimal registry Feature extends image; verify labels merged and container boots.

## Acceptance Criteria
- Feature extension path works; errors explicit when infra missing; CI green.

## References
- `docs/subcommand-specs/up/SPEC.md` (§5)
- `docs/subcommand-specs/up/GAP.md` (§4, §17)
