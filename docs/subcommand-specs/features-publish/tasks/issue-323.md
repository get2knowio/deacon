# [features publish] Implement semantic version tagging and multi-tag publish

https://github.com/get2knowio/deacon/issues/323

<!-- Labels: subcommand:features-publish, type:enhancement, priority:high, scope:large -->

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Add semantic version tagging per spec: for version `X.Y.Z`, publish tags `[X, X.Y, X.Y.Z, latest]` skipping tags already present. Implement the loop that publishes multiple tags for each feature artifact.

## Specification Reference
**From SPEC.md Section:** §5. Core Execution Logic – Semantic Tagging

**From GAP.md Section:** 2. Core Execution Logic Gaps – Missing semantic version tagging

### Expected Behavior
```pseudocode
existing = list_tags(oci_ref)
desired = [X, X.Y, X.Y.Z, latest]
to_publish = desired - existing
for tag in to_publish: publish(tag)
```

### Current Behavior
- Single tag publish only; no semantic derivation

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Implement tag computation and publish loop
- `crates/core/src/oci.rs` — Ensure `publish_feature` can be called repeatedly with different tagged refs
- Add utility in core for semver tag computation (optional module): `crates/core/src/semver_utils.rs`

#### Specific Tasks
- [ ] Parse feature version from `devcontainer-feature.json`
- [ ] Compute `[major, major.minor, full, latest]` using `semver`
- [ ] Filter against existing tags (requires #324 list_tags)
- [ ] Publish missing tags iteratively
- [ ] Log which tags were published

### 2. Data Structures
From DATA-STRUCTURES.md (output): include `publishedTags` when JSON mode is enabled

### 3. Validation Rules
- [ ] Validate version is valid semver (see dedicated issue #328)

### 4. Cross-Cutting Concerns
- [ ] Theme 4 - Semantic Versioning Operations
- [ ] Theme 1 - JSON Output Contract (published tags)

## Testing Requirements

### Unit Tests
- [ ] Tag computation for 1.2.3 => ["1", "1.2", "1.2.3", "latest"]
- [ ] Filtering when some tags exist (e.g., only publish missing)

### Integration Tests
- [ ] Dry-run: prints derived tags
- [ ] With mock/real registry: publishes all derived tags when none exist

### Smoke Tests
- [ ] N/A (covered by integration)

### Examples
- [ ] Update docs to explain semantic tagging

## Acceptance Criteria
- [ ] Multi-tag loop implemented and verified
- [ ] Logs show correct tagging; JSON includes `publishedTags`
- [ ] CI checks pass

## Dependencies

**Blocked By:** #324 (list tags) and #328 (semver validation)

**Related:** #329 (JSON output)
