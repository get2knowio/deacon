# [features publish] Publish collection metadata (devcontainer-collection.json)

https://github.com/get2knowio/deacon/issues/326

<!-- Labels: subcommand:features-publish, type:enhancement, priority:high, scope:medium -->

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Implement publishing of collection metadata to the registry, using `devcontainer-collection.json` produced by `features package` (or generated when packaging within publish flow). This enables discovery of all features within a namespace.

## Specification Reference
**From SPEC.md Section:** §5. Core Execution Logic – Publish collection metadata

**From GAP.md Section:** 2. Core Execution Logic Gaps – Collection Metadata Publishing

### Expected Behavior
```pseudocode
collection_ref = make_collection_ref(registry, namespace)
push_collection_metadata(collection_ref, join(out_dir, 'devcontainer-collection.json'))
```

### Current Behavior
- Not implemented

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Add call to publish collection metadata after feature tags
- `crates/core/src/oci.rs` — Add `publish_collection_metadata` helper (or reuse template publishing pattern) to upload a blob/manifest representing the metadata

#### Specific Tasks
- [ ] Ensure `devcontainer-collection.json` exists (from packaging)
- [ ] Construct collection OCI reference `<registry>/<namespace>`
- [ ] Upload metadata with appropriate media type
- [ ] Update JSON/text outputs to indicate success

### 2. Data Structures
- `collection_ref`: `{ registry, namespace }` (conceptual)

### 3. Validation Rules
- [ ] Validate file exists; error if missing

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output (may extend to include collection publish result)
- [ ] Theme 6 - Error Messages

## Testing Requirements

### Unit Tests
- [ ] Publishing succeeds with valid file
- [ ] Error when metadata file missing

### Integration Tests
- [ ] Registry mock accepts metadata upload

## Acceptance Criteria
- [ ] Collection metadata published after features
- [ ] CI checks pass

## Dependencies

**Blocked By:** #324 (if using common OCI client changes)
