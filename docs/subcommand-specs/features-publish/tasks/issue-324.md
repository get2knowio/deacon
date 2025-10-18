# [Infrastructure] OCI client: list tags and manifest existence APIs

https://github.com/get2knowio/deacon/issues/324

<!-- Labels: infrastructure, cross-cutting, subcommand:features-publish, type:enhancement, priority:high, scope:medium -->

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Add OCI registry helper APIs to list tags and check manifest existence, used by `features publish` for semantic tagging and idempotency.

## Specification Reference
**From SPEC.md Section:** §7. External System Interactions – OCI Registries

**From GAP.md Section:** 3. OCI Client Gaps – Missing Tag Listing API; Idempotency checks

### Expected Behavior
- Implement `list_tags(feature_ref) -> Vec<String>` using `/v2/<name>/tags/list`
- Implement `get_manifest_digest(feature_ref) -> Option<String>` using `HEAD` per OCI v2 (prefer HEAD for existence)

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/oci.rs` — Add methods and wire into existing client
- Update/mock tests as per `.github/copilot-instructions.md` HTTP client guidelines

#### Specific Tasks
- [ ] Add `list_tags()` per distribution spec
- [ ] Add `get_manifest_digest()` using HEAD
- [ ] Update mocks and tests for new trait methods (see Copilot Instructions, OCI Guidelines)
- [ ] Handle auth (401/403), not found (404), and server errors distinctly

### 2. Data Structures
- Potential `HttpResponse` struct as per guidelines if signature changes are needed

### 3. Validation Rules
N/A

### 4. Cross-Cutting Concerns
- [ ] Theme 4 - Semantic Versioning Operations (tag filtering)
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] List tags success/empty
- [ ] Manifest existence via HEAD: 200/404 handling

### Integration Tests
- [ ] Against a fake registry/mock client with realistic Location headers

### Smoke Tests
- [ ] N/A

## Acceptance Criteria
- [ ] Methods implemented with proper error handling
- [ ] Clippy/lints/tests all pass

## Dependencies

**Blocks:** #323, #325

**Related to Infrastructure (PARITY_APPROACH.md):** Phase 0 item #1 OCI Registry Infrastructure Enhancement
