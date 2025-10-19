---
subcommand: templates
type: enhancement
priority: high
scope: large
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: large"]
---

# [templates] Implement Apply OCI Fetch, Blob Extraction, and File Listing

## Issue Type
- [x] Core Logic Implementation
- [x] External System Interactions
- [x] Testing & Validation

## Description
Implement the end-to-end OCI workflow for `templates apply`: parse the OCI ref, fetch the manifest, resolve the first layer digest, download the blob to a temp directory, and extract into the `--workspace-folder`, honoring `--omit-paths` and reserved file exclusions. Collect and return the list of written relative file paths in stdout JSON.

## Specification Reference

**From SPEC.md Section:** §5 Core Execution Logic (apply), §7 External System Interactions (OCI), §10 Output Specifications

**From DIAGRAMS.md:** Sequence — templates apply

**From GAP.md Section:** Executive Summary (OCI behavior missing implied by CLI move to `--template-id`); 2.4 Output format (list of files)

### Expected Behavior
- Resolve and GET manifest for `<registry>/<namespace>/<name>:<tag|@digest>`; select platform if index.
- Resolve first layer digest; GET blob; extract into workspace.
- Apply omit rules and reserved files: `devcontainer-template.json`, `README.md`, `NOTES.md`.
- Return `{ files: string[] }` listing relative paths actually written.

### Current Behavior
- Local filesystem copy; no OCI registry interactions.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify / Add
- `crates/core/src/oci.rs` — Ensure helpers for `fetch_manifest`, `download_blob`, with auth and media types.
- `crates/core/src/templates_apply.rs` (new) or extend `templates.rs` — Implement `apply_from_oci_ref(args, opts)` orchestrating fetch/extract/substitute.
- `crates/deacon/src/commands/templates.rs` — Route `apply` to OCI path when `--template-id` is an OCI ref (always per spec).

#### Specific Tasks
- [ ] Parse OCI ref and fetch manifest; error if parse/fetch fails.
- [ ] Handle index by selecting current OS/arch when present (shared util if available).
- [ ] Download layer blob to `--tmp-dir` or system temp.
- [ ] Secure extraction (no path traversal); write to workspace; collect written paths.
- [ ] Integrate with substitution and (later) feature injection; apply after extraction.

### 2. Data Structures
```rust
pub struct ApplyOutput { pub files: Vec<String> }
```

### 3. Validation Rules
- [ ] Fail fast on parse errors or missing manifest/layer.
- [ ] Respect omit rules including directory-wide `/*`.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract.
- [ ] Theme 6 - Error Messages.

## Testing Requirements

### Unit/Integration Tests
- [ ] Mock registry: manifest -> blob -> extraction; verify file list and omissions.
- [ ] Error paths: 404 manifest, missing layer, auth failure.

## Acceptance Criteria
- [ ] Apply works end-to-end with OCI refs; tests pass; CI green.

## Definition of Done
- [ ] Implementation complete with robust error handling and logs.
