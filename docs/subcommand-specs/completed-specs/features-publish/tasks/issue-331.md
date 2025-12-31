# [features publish] Logging polish and --log-level alignment

https://github.com/get2knowio/deacon/issues/331

<!-- Labels: subcommand:features-publish, type:enhancement, priority:low, scope:small -->

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Align logging controls with spec: add `--log-level` for this subcommand if not already covered by global flag semantics, and ensure logs describe semantic tags and idempotency outcomes.

## Specification Reference
**From SPEC.md Section:** §2. Command-Line Interface – log level

**From GAP.md Section:** 1. CLI Gaps – Missing `--log-level` convenience flag

### Expected Behavior
- Users can set log verbosity explicitly via flag
- Logs include which tags will be published vs skipped

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` — Consider local `--log-level` or rely on global; ensure description updates
- `crates/deacon/src/commands/features.rs` — Emit detailed logs during tagging and idempotency checks

#### Specific Tasks
- [ ] Add/confirm `--log-level` handling consistent with global options
- [ ] Log computed tags, existing tags, and final `to_publish`

### 2. Data Structures
N/A

### 3. Validation Rules
N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - Log separation from JSON

## Testing Requirements

### Unit/Integration
- [ ] Verify logs (in tests where feasible) and no interference with JSON stdout

## Acceptance Criteria
- [ ] Log-level behavior consistent with spec
- [ ] Logs clear on publish vs skip decisions

## Dependencies

**Related:** #322, #323
