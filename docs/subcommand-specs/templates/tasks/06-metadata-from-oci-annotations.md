---
subcommand: templates
type: enhancement
priority: high
scope: small
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: small"]
---

# [templates] Implement Metadata Retrieval from OCI Manifest Annotations

## Issue Type
- [x] Core Logic Implementation
- [x] External System Interactions
- [x] Error Handling

## Description
Refactor `templates metadata` to accept an OCI `templateId` reference, fetch the manifest, and print the `dev.containers.metadata` annotation JSON to stdout. If the manifest or annotation is missing, print `{}` to stdout and exit non-zero, per spec.

## Specification Reference

**From SPEC.md Section:** §2 CLI (metadata), §5 Core Execution Logic (metadata), §10 Output Specifications

**From GAP.md Section:** 1.3 Metadata Command gaps

### Expected Behavior
- Input is an OCI ref (`registry/namespace/name[:tag|@sha256:digest]`).
- Fetch manifest; if present and contains `dev.containers.metadata`, print parsed JSON. Otherwise, print `{}` and exit non-zero.

### Current Behavior
- Operates on local path; no registry interaction.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/templates.rs` — Update `metadata` positional arg semantics and error messages.
- `crates/core/src/oci.rs` — Ensure `fetch_manifest(ref)` returns annotations; add `fetch_manifest_if_exists` helper.

#### Specific Tasks
- [ ] Validate OCI ref format; on failure, exit with parse error.
- [ ] On 404 or missing annotation, stdout `{}` and non-zero exit.
- [ ] Ensure logs describe the failure without dumping secrets.

### 2. Data Structures
```rust
// MetadataOutput: JSON object or {}
```

### 3. Validation Rules
- [ ] Require positional `templateId`.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract (stdout only).
- [ ] Theme 6 - Error Messages formatting.

## Testing Requirements

### Unit/Integration Tests
- [ ] With annotation present -> outputs JSON.
- [ ] Without annotation -> outputs `{}` and non-zero exit.

## Acceptance Criteria
- [ ] Behavior matches spec; tests pass and CI green.

## Definition of Done
- [ ] Implementation complete; CLI help and docs updated.
