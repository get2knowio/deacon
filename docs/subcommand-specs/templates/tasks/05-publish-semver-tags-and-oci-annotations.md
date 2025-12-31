---
subcommand: templates
type: enhancement
priority: high
scope: medium
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: medium"]
---

# [templates] Implement Semantic Version Tags and Manifest Annotations in Publish

## Issue Type
- [x] Core Logic Implementation
- [x] External System Interactions
- [x] Testing & Validation

## Description
Extend `templates publish` to compute and push semantic tags `[major, major.minor, major.minor.patch, latest]` when a valid version exists and ensure the OCI manifest includes the `dev.containers.metadata` annotation containing serialized template metadata. Skip tags superseded by existing higher versions.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (publish), §7 External System Interactions (OCI), §10 Output Specifications

**From GAP.md Section:** 3.1 Semantic Version Tags; 3.3 Manifest Annotations

### Expected Behavior
- On publish, fetch existing tags for the template ref; compute missing semantic tags for the new version.
- Push the layer and manifest to each tag; set annotation `dev.containers.metadata` to the template metadata JSON.
- Return `publishedTags` and `digest` per template id in stdout map.

### Current Behavior
- Single tag push with no semver computation; annotation handling uncertain.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/oci.rs` —
  - Add utilities: `list_tags(ref)`, `push_with_annotations(ref, archive, tags, annotations)`.
  - Ensure `Location` header handling and HEAD blob checks follow OCI guidelines (see repo instructions).
- `crates/core/src/templates_publish.rs` —
  - Integrate with OCI utilities; compute tags using a shared semver helper.
- Update test mocks per OCI client trait changes (see instructions section “OCI Registry & HTTP Client Implementation Guidelines”).

#### Specific Tasks
- [ ] Implement semver parse with regex `^\d+(\.\d+(\.\d+)?)?$`.
- [ ] Compute tags `[major, major.minor, version, latest]` and filter out already published or superseded.
- [ ] Ensure manifest annotation includes `dev.containers.metadata`.
- [ ] Emit `publishedTags` and `digest` in output.

### 2. Data Structures
```rust
pub struct PublishEntry { pub published_tags: Option<Vec<String>>, pub digest: Option<String>, pub version: Option<String> }
```

### 3. Validation Rules
- [ ] If version invalid (not semver), fail the specific template with error; continue others where possible, per spec guidance.

### 4. Cross-Cutting Concerns
- [ ] Theme 4 - Semantic Versioning Operations.
- [ ] Theme 6 - Error Messages.

## Testing Requirements

### Unit Tests
- [ ] Tag computation for various versions; filtering against existing tag sets.

### Integration Tests
- [ ] Mock registry to verify annotations and multi-tag push flows.

## Acceptance Criteria
- [ ] Semver tags computed and published; annotations included.
- [ ] Tests pass and CI green.

## Definition of Done
- [ ] Implementation complete; docs updated under templates/publish.
