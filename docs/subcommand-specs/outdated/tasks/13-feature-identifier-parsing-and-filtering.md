# [outdated] Feature Identifier Parsing and Filtering

Labels:
- subcommand: outdated
- type: enhancement
- priority: medium
- scope: small

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Description
Implement or reuse robust parsing of feature identifiers into `OCIRef` and filter out non-versionable identifiers (local paths `./feature`, direct tarballs `https://...`, legacy/no-registry forms) per SPEC §14. This ensures only versionable features are processed.

## Specification Reference

- From SPEC.md Section: §5 Core Execution Logic; §14 Edge Cases and Corner Cases
- From GAP.md Section: 2.3 Version Resolution (parse feature ref), 9 Error Handling (invalid identifiers skipped)

### Expected Behavior
- `parse_feature_ref` returns an `OCIRef` with `tag` or `digest` when the identifier is an OCI registry path.
- Non-OCI identifiers return `None` and are skipped by the command pipeline.

### Current Behavior
- Parser status unknown; implement if missing or extend for this use case.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/core/src/version.rs` or `crates/core/src/oci.rs`
  - Implement `parse_feature_ref(id: &str) -> Option<OCIRef>` if not present.
  - Unit tests for diverse identifier forms.
- `crates/deacon/src/commands/outdated.rs`
  - Integrate filter to omit non-OCI IDs before parallel resolution.

#### Specific Tasks
- [ ] Recognize `registry/namespace/name[:tag|@sha256:...]` patterns.
- [ ] Skip `./local/path`, absolute file paths, and `https://...` tarballs.

### 2. Data Structures
- `OCIRef` as defined in DATA-STRUCTURES.

### 3. Validation Rules
- [ ] No hard-fail for invalid IDs; skip and continue.

### 4. Cross-Cutting Concerns
- Theme 6 - Error Messages: log at debug/trace rather than error for skipped items.

## Testing Requirements

### Unit Tests
- [ ] Positive: valid OCI identifiers parsed correctly.
- [ ] Negative: local paths/URLs return `None` and are skipped.

### Integration Tests
- [ ] Mixed identifiers only process versionable ones.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] Parser implemented or reused; filtering integrated.
- [ ] Tests cover positive/negative cases.
- [ ] CI passes.

## Implementation Notes
- Keep the parser conservative; defer advanced forms if not required by current spec.

### Edge Cases to Handle
- Missing registry host but with namespace/name → treat as invalid for version listing.

### References
- SPEC: §14
- GAP: §2.3, §6