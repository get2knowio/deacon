# [features publish] Complete authentication: username/password-stdin, DOCKER_CONFIG, DEVCONTAINERS_OCI_AUTH

https://github.com/get2knowio/deacon/issues/327

<!-- Labels: subcommand:features-publish, type:enhancement, priority:high, scope:medium, security -->

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Wire authentication mechanisms for publishing to private registries: `--username`, `--password-stdin`, `DOCKER_CONFIG` credential store, and `DEVCONTAINERS_OCI_AUTH` env for tests.

## Specification Reference
**From SPEC.md Section:** §7. External System Interactions – Authentication

**From GAP.md Section:** 3. OCI Client Gaps – Incomplete authentication implementation

### Expected Behavior
- If `--username` present and `--password-stdin` true: read password from stdin and set credentials
- If `DEVCONTAINERS_OCI_AUTH` present: parse `host|user|pass` and use for matching host
- Else if `DOCKER_CONFIG` set: use credential helpers/auth.json
- Credentials applied to all HTTP requests to registry

### Current Behavior
- Flags exist but not wired; debug logs only

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Capture and pass auth config to core/OCI client
- `crates/core/src/oci.rs` — Add/set auth on HTTP client; store per-host creds
- Update mock clients per `.github/copilot-instructions.md` OCI guidelines

#### Specific Tasks
- [ ] Implement `--password-stdin` reading with secret redaction
- [ ] Parse `DEVCONTAINERS_OCI_AUTH`
- [ ] Integrate Docker config auth when available
- [ ] Ensure headers/tokens added to registry requests

### 2. Data Structures
- Potential `AuthConfig { host, username, password }`

### 3. Validation Rules
- [ ] If `--password-stdin` set without `--username`: error "--password-stdin requires --username."

### 4. Cross-Cutting Concerns
- [ ] Theme 7 (Secrets Management & Log Redaction) from PARITY_APPROACH.md
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Parsing `DEVCONTAINERS_OCI_AUTH`

### Integration Tests
- [ ] Auth with stdin
- [ ] Auth with Docker config (mocked)

## Acceptance Criteria
- [ ] Private registry publish works
- [ ] Secrets redacted
- [ ] CI checks pass

## Dependencies

**Blocks:** #324, #323

**Related to Infrastructure:** Phase 0 OCI Registry Infrastructure Enhancement
