# [features publish] Implement CLI flags parity: --namespace, default --registry, validation

https://github.com/get2knowio/deacon/issues/322

<!-- Labels: subcommand:features-publish, type:enhancement, priority:high, scope:medium -->

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: 

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Add spec-compliant CLI flags for `features publish`: introduce required `--namespace` (owner/repo) and make `--registry` optional with default `ghcr.io`. Separate hostname from namespace to align with the spec and TypeScript CLI.

## Specification Reference

**From SPEC.md Section:** §2. Command-Line Interface

**From GAP.md Section:** 1. Command-Line Interface Gaps – Missing `--namespace` flag; `--registry` conflates host+path currently

### Expected Behavior
- CLI signature: `devcontainer features publish [target] --registry <host> --namespace <owner/repo>`
- `--registry` defaults to `ghcr.io` when not provided
- `--namespace` is required and validated (format: `<owner>/<repo>`, no leading/trailing slashes)

### Current Behavior
- Only `--registry` flag exists and is used for full reference
- No `--namespace` flag; no validation for namespace

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/cli.rs` — Extend `FeatureCommands::Publish` with `namespace: String`, make `registry: Option<String>` with default
- `crates/deacon/src/commands/features.rs` — Adjust argument extraction and downstream calls to pass registry host and namespace separately

#### Specific Tasks
- [ ] Add `#[arg(long, short = 'n')] namespace: String` (required)
- [ ] Set `#[arg(long, short = 'r', default_value = "ghcr.io")] registry: String`
- [ ] Validate `namespace` matches regex `^[^/\s]+/[^/\s]+$`
- [ ] Update help text and docs

### 2. Data Structures
N/A

### 3. Validation Rules
- [ ] Required pairing: `--namespace` required
- [ ] Format: `namespace` must be `<owner>/<repo>`
- [ ] Error message: "Invalid namespace format. Expected '<owner>/<repo>'."

### 4. Cross-Cutting Concerns
- [ ] Theme 2 - CLI Validation: enforce format and required flags
- [ ] Theme 6 - Error Messages: use exact phrasing

## Testing Requirements

### Unit Tests
- [ ] Parse success with defaults: `--registry` omitted
- [ ] Parse error when `--namespace` missing
- [ ] Parse error when `namespace` invalid

### Integration Tests
- [ ] End-to-end invocation uses default `ghcr.io`

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` help/usage if necessary

### Examples
- [ ] Update examples/readme usage snippets for `features publish`

## Acceptance Criteria
- [ ] Flags implemented and validated
- [ ] CI checks pass
- [ ] Docs updated

## Implementation Notes
Breaking change from current combined registry reference; migration documented in tracking issue.

## Dependencies

**Blocked By:** None

**Related to Infrastructure (PARITY_APPROACH.md):** Theme 2, Theme 6
