# [up] Complete Docker buildx and Compose interactions

<!-- Suggested labels: subcommand: up, type: enhancement, priority: medium, scope: medium -->

## Issue Type
- [x] External System Interactions
- [x] Core Logic Implementation

## Description
Implement missing Docker BuildKit/buildx capabilities used during `up` and compose-specific behaviors: handle `--cache-from`, `--cache-to`, `--platform`, optional `--push` for buildx, compose project name inference from `.env`, and TTY handling differences; add Windows Compose v1 PTY fallback hooks (no-op on Linux).

## Specification Reference
- From SPEC.md Section: §7. External System Interactions
- From GAP.md Section: §6 Missing buildx options; §18 Compose-specific behaviors

### Expected Behavior
- Buildx invocation supports cache-from/to and platform options propagated from flags.
- Compose project name inferred from `.env` when not provided.
- Consider TTY vs non-TTY output; PTY fallback wired (guarded by platform checks).

### Current Behavior
- Partial docker run/exec/compose; buildx incomplete; compose project name inference missing; PTY fallback missing.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/src/commands/up.rs` - extend build path for buildx options if used during `up`.
- `crates/deacon/src/commands/build.rs` - share helpers for buildx flags if appropriate.
- `crates/core/src/container.rs` or `crates/core/src/runtime/*` - add compose project name inference utilities and TTY detection helpers.

### 2. Data Structures
```rust
// DockerResolverParameters.buildxPlatform, buildxPush, buildxCacheTo, buildxOutput
```

### 3. Validation Rules
- [ ] Validate platform strings and cache-to formats, error with exact messages.

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages.

## Testing Requirements
- Unit: compose project name inference from env file.
- Integration: buildx options presence on build call; `.env` project name applied.

## Acceptance Criteria
- External interactions behave per spec; tests pass.

## References
- `docs/subcommand-specs/up/SPEC.md` (§7)
- `docs/subcommand-specs/up/GAP.md` (§6, §18)
