---
subcommand: upgrade
type: enhancement
priority: high
scope: medium
---

# [upgrade] Generate Lockfile Wiring (Core Path)

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Wire feature resolution to lockfile generation using existing core modules. Produce an in-memory `Lockfile` from resolved `FeaturesConfig` and prepare for either stdout (dry-run) or file write. Leverage `deacon_core::lockfile` structures and avoid re-implementing them.

## Specification Reference

**From SPEC.md Section:** §5. Core Execution Logic (Phase 3)

**From GAP.md Section:** 2.4 Feature Resolution, 2.5 Lockfile Generation

### Expected Behavior
- Resolve features via existing core resolver, including dependency processing and digest computation.
- Translate resolved sets into `Lockfile` entries with deterministic ordering.

### Current Behavior
- No upgrade-specific wiring exists; core lockfile module exists for I/O and validation.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Add a function `generate_lockfile_from_config(config) -> Lockfile`
  - Call core feature resolver to obtain `FeaturesConfig`
  - Map resolved features to `{ version, resolved, integrity }`

#### Specific Tasks
- [ ] Integrate resolver and capture digests/versions
- [ ] Build `Lockfile { features: HashMap<_,_> }` in deterministic key order
- [ ] Add tracing spans: `feature.resolve`, `lockfile.generate`

### 2. Data Structures

Required from DATA-STRUCTURES.md:
```rust
struct Lockfile {
    features: std::collections::HashMap<String, LockfileFeature>,
}

struct LockfileFeature {
    version: String,
    resolved: String,
    integrity: String,
}
```

### 3. Validation Rules
- [ ] Pass generated lockfile through `deacon_core::lockfile::write_lockfile` later; here ensure data completeness

### 4. Cross-Cutting Concerns

Applies from PARITY_APPROACH.md:
- [ ] Theme 4 - Semantic Versioning Operations (ensure tag filtering/selection as resolver provides)
- [ ] Theme 6 - Error Messages for resolver failures

## Testing Requirements

### Unit Tests
- [ ] Mapping test: from mock resolved features to lockfile entries

### Integration Tests
- [ ] End-to-end resolution in a fixture with simple features

### Smoke Tests
- [ ] None yet

### Examples
- [ ] None yet

## Acceptance Criteria
- [ ] Function exists and returns deterministic lockfile structures
- [ ] Uses tracing spans, errors bubble with context
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§5)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§2.4–2.5)
