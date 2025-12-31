# [Infrastructure] OCI Authentication Wiring for Tags and Manifests

Labels:
- subcommand: outdated
- type: infrastructure
- priority: medium
- scope: small

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Verify and, if needed, implement authentication flow for tag listing and manifest/metadata fetches used by `outdated`. Ensure that registry requests carry appropriate auth headers and integrate with existing credential helpers.

## Specification Reference

- From SPEC.md Section: §7 External System Interactions — OCI Registries (Authentication)
- From GAP.md Section: 4.3 Authentication (Partial)

### Expected Behavior
- Tag and manifest requests succeed against authenticated registries when credentials are available (e.g., via `docker login`).
- Errors from 401/403 are handled upstream by graceful degradation logic.

### Current Behavior
- Base OCI client exists; integration for tag listing path may be incomplete.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/oci.rs`
  - Ensure new tag listing and manifest helpers use the existing auth/token flow.
- Tests under `crates/core/tests/`
  - Add tests simulating 401/403 and token acquisition if applicable.

#### Specific Tasks
- [ ] Confirm token exchange for `scope=repository:<path>:pull` where required.
- [ ] Ensure headers propagate through helper calls.

### 2. Data Structures
- Reuse existing HTTP client and auth token structures.

### 3. Validation Rules
- [ ] Do not log secrets; redaction in place.

### 4. Cross-Cutting Concerns
- Theme 6 - Error Messages and redaction: no sensitive logs.

## Testing Requirements

### Unit/Integration Tests
- [ ] Mock auth failures and verify behavior.
- [ ] Success path with simulated token.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] Authenticated requests work for tag list and manifest fetch.
- [ ] Tests cover 401/403 and success.
- [ ] CI passes.

## Implementation Notes
- Follow existing OCI client trait and avoid duplicating logic.

### Edge Cases to Handle
- Anonymous registries where auth not required.
- Registries requiring bearer tokens with specific scopes.

### References
- GAP: §4.3
- SPEC: §7