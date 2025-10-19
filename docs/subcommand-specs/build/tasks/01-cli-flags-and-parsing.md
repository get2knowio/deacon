---
subcommand: build
type: enhancement
priority: high
scope: medium
---

# [build] Implement core CLI flags and parsing

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Parent Issue
Tracks: #0 (tracking issue)

## Description
Add missing core CLI flags for the build subcommand and plumb them into the internal argument structures. This enables tagging, pushing, custom outputs, and label injection which are prerequisites for several downstream behaviors.

## Specification Reference

**From SPEC.md Section:** §2 Command-Line Interface; §3 Input Processing Pipeline

**From GAP.md Section:** 1.1 Missing Required Flags; 8.1 ParsedInput Structure

### Expected Behavior
- CLI supports and parses:
  - `--image-name <name[:tag]>` (repeatable)
  - `--push` (boolean)
  - `--output <spec>` (string)
  - `--label <name=value>` (repeatable)
- Values are preserved in order and exposed to execution logic.

### Current Behavior
- Flags are missing; `BuildArgs` and CLI have no fields/flags for these options.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` – add clap flags for: `--image-name`, `--push`, `--output`, `--label`; wire to `BuildArgs`.
- `crates/deacon/src/commands/build.rs` – extend `BuildArgs` with fields for these flags and ensure they are populated; thread through to execution as needed in later issues.
- `crates/deacon/src/commands/build.rs` – prepare storage for image names and labels even if not yet acted on.

#### Specific Tasks
- [ ] Add CLI flags: `--image-name <name[:tag]>...`, `--push`, `--output <spec>`, `--label <k=v>...`.
- [ ] Add fields to `BuildArgs`: `image_names: Vec<String>`, `push: bool`, `output: Option<String>`, `labels: Vec<String>`.
- [ ] Ensure flags are parsed with correct types and support repeatable semantics where required.
- [ ] Preserve insertion order for repeatables per spec.

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
// Parsed input (subset relevant to this issue)
pub struct ParsedInput {
    pub image_names: Vec<String>,   // repeatable --image-name
    pub push: bool,                 // buildx only
    pub output: Option<String>,     // buildx only; mutually exclusive with push
    pub labels: Vec<String>,        // repeatable --label
    // ...
}
```

### 3. Validation Rules
- No new validations are enforced in this issue; parsing only. Mutual exclusion and gating are handled in a follow-up issue.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 2 - CLI Validation: wire-up flags; validations implemented in a later issue.
- [ ] Theme 6 - Error Messages: defer to validation issue for exact messages.

## Testing Requirements

### Unit Tests
- [ ] Add tests in `crates/deacon` verifying clap parses repeatable flags and types correctly.
- [ ] Confirm ordering of `--image-name` and `--label` is preserved.

### Integration Tests
- [ ] Basic CLI parse roundtrip using `assert_cmd` to confirm the flags reach `BuildArgs`.

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` only if it asserts flags presence; functional use comes later.

### Examples
- [ ] Add a minimal invocation example to `examples/build/basic-dockerfile/README.md` documenting new flags (no behavior guarantees yet).

## Acceptance Criteria

- [ ] Flags available and documented in `--help` for `deacon build`.
- [ ] `BuildArgs` includes `image_names`, `push`, `output`, `labels` and they are populated from CLI.
- [ ] CI checks pass:
  ```bash
  cargo build --verbose
  cargo test --verbose -- --test-threads=1
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  ```
- [ ] No `unwrap()`/`expect()` in production paths; add error context where helpful.

## Implementation Notes
- Keep this issue strictly to CLI surface and parsing. Actual effects (tagging, pushing, output control, label injection) are split into subsequent issues.

### Edge Cases to Handle
- Duplicate `--label` keys are allowed; Docker resolves last-wins at build time. We simply pass through.

## Definition of Done
- [ ] New flags are available and wired into `BuildArgs`.
- [ ] Tests cover parsing and ordering.
- [ ] No behavior changes beyond parsing.

## References
- Specification: `docs/subcommand-specs/build/SPEC.md` (§2, §3)
- Gap Analysis: `docs/subcommand-specs/build/GAP.md` (§1.1, §8.1)
- Data Structures: `docs/subcommand-specs/build/DATA-STRUCTURES.md`
- Diagrams: `docs/subcommand-specs/build/DIAGRAMS.md`
- Parity Approach: `docs/PARITY_APPROACH.md`
