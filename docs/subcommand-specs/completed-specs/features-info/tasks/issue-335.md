# [features info] Implement OCI manifest fetch and canonical identifier

https://github.com/get2knowio/deacon/issues/335

## Issue Type
- [x] Core Logic Implementation
- [ ] Missing CLI Flags
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Implement manifest fetching for a feature reference and compute a canonical identifier (e.g., digest-based). Support both OCI image manifest retrieval and local cache hits. Expose `canonicalId` in JSON output and human-readable in text.

## Specification Reference
- From SPEC.md Section: §6. Manifest Resolution
- From GAP.md Section: 4.1 Manifest Canonicalization

### Expected Behavior
- Retrieve manifest using OCI client; compute canonical identifier deterministically (manifest digest or similar canonical form).
- In JSON mode include `canonicalId` alongside `manifest` object.

### Current Behavior
- Partial or no manifest querying; canonical ID not present.

## Implementation Requirements

### Files to Modify
- `crates/core/src/oci.rs` (ensure manifest fetch API)
- `crates/deacon/src/commands/features.rs` (use OCI client and expose canonicalId)

### Specific Tasks
- [ ] Implement or reuse a `get_manifest` function returning manifest bytes and digest
- [ ] Compute canonicalId (e.g., sha256:<digest>) and surface it in outputs
- [ ] Graceful error handling: in JSON mode emit `{}` and exit 1 on fetch failures

## Testing Requirements
- Unit Tests:
  - [ ] Manifest parsing and canonicalId computation
  - [ ] Cache-hit behavior
- Integration Tests:
  - [ ] End-to-end `features info --output-format json` returns `manifest` and `canonicalId`

## Acceptance Criteria
- [ ] `canonicalId` present in JSON output and correct
- [ ] Manifest fetch uses OCI client and respects auth
- [ ] CI checks pass

## Dependencies
Blocked By: #324 (OCI client list/manifests improvements)
