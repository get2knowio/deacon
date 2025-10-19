---
subcommand: upgrade
type: enhancement
priority: low
scope: small
---

# [upgrade] Progress Logging and User-Facing Messages

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Error Handling
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
Add upgrade-specific progress logs and messages using `tracing`, including informative lines for pinning attempts, missing features, feature resolution progress, and lockfile write confirmation. Logs go to stderr and respect `--log-level`.

## Specification Reference

**From SPEC.md Section:** §10 Output Specifications (stderr logging)

**From GAP.md Section:** 5.2 Progress Logging

### Expected Behavior
- Logs include:
  - "Updating '<feature>' to '<target_version>' in devcontainer.json"
  - "No Features found in '<path>'"
  - Feature resolution start/finish
  - Lockfile write confirmation and path

### Current Behavior
- Minimal/no logs.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/upgrade.rs`
  - Add structured `tracing` spans and info! logs per milestone

#### Specific Tasks
- [ ] Add spans: `upgrade.start`, `config.resolve`, `feature.resolve`, `lockfile.generate`, `lockfile.write`
- [ ] Use structured fields (feature id, version, path)

### 2. Data Structures
- N/A

### 3. Validation Rules
- [ ] Ensure messages use exact wording where specified

### 4. Cross-Cutting Concerns
- [ ] Theme 6 - Error Messages (consistency)

## Testing Requirements

### Unit/Integration
- [ ] Optional: Capture logs with `tracing` test subscriber and assert presence

### Smoke Tests
- [ ] None

### Examples
- [ ] Ensure examples mention expected logs

## Acceptance Criteria
- [ ] Informative logs present in key steps
- [ ] Respect `--log-level`
- [ ] CI passes

## References
- SPEC: `docs/subcommand-specs/upgrade/SPEC.md` (§10)
- GAP: `docs/subcommand-specs/upgrade/GAP.md` (§5.2)
