---
subcommand: build
type: enhancement
priority: medium
scope: small
---

# [build] Implement hidden and experimental flags plumbing

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Add the hidden/experimental flags to the CLI and internal structures with basic wiring so follow-up tasks (features, lockfile, metadata) can rely on them: `--skip-feature-auto-mapping`, `--skip-persisting-customizations-from-features`, `--experimental-lockfile`, `--experimental-frozen-lockfile`, `--omit-syntax-directive`.

## Specification Reference

**From SPEC.md Section:** §2 Command-Line Interface – Hidden/Experimental Flags; §6 State Management

**From GAP.md Section:** 1.3 Hidden/Experimental Flags

### Expected Behavior
- Flags are available (hidden in help as appropriate) and populate corresponding fields in `BuildArgs` / resolver parameters.

### Current Behavior
- Flags absent.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` – define hidden flags and map to args.
- `crates/deacon/src/commands/build.rs` – add fields to `BuildArgs` and thread values to where they’ll be consumed in other tasks.

#### Specific Tasks
- [ ] Add flags and fields: `skip_feature_auto_mapping`, `skip_persist_customizations`, `experimental_lockfile`, `experimental_frozen_lockfile`, `omit_syntax_directive`.
- [ ] Ensure they default to false and are not shown in help unless we adopt a pattern for hidden flags in clap.

### 2. Data Structures
See DATA-STRUCTURES ParsedInput fields for names and types.

### 3. Validation Rules
- Basic boolean flags; no extra validation beyond future consumers.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 2 - CLI Validation (consistency of flag names and defaults)

## Testing Requirements

### Unit Tests
- [ ] CLI parsing sets the fields correctly with defaults and when provided.

### Integration Tests
- [ ] Minimal ensures flag values reach `BuildArgs` in a run.

## Acceptance Criteria
- [ ] Flags exist and are wired; help remains stable (hidden where intended).
- [ ] CI checks pass.

## Definition of Done
- [ ] All flags are available and parsed.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§2, §6)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§1.3)
