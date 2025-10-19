---
subcommand: templates
type: enhancement
priority: high
scope: medium
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: medium"]
---

# [templates] Implement Publish CLI, Collection Mode, and Output Map

## Issue Type
- [x] Missing CLI Flags
- [x] Core Logic Implementation
- [x] Testing & Validation

## Description
Align `templates publish` with the spec by adding `--namespace` (required) and defaulting `--registry` to `ghcr.io`, supporting positional `target` defaulting to `.`. Implement collection mode: package and publish each template under `src/` and generate/publish `devcontainer-collection.json`. Emit a JSON map `{ [id]: { publishedTags?, digest?, version? } }` on stdout.

## Specification Reference

**From SPEC.md Section:** §2 CLI (publish), §5 Core Execution Logic (publish), §10 Output Specifications, §6 State Management

**From GAP.md Section:** 1.2 Publish Command gaps; 3.2 Collection Metadata

### Expected Behavior
- CLI:
  - `templates publish [target] --namespace <owner/repo> [--registry ghcr.io]`.
- Collection detection: if `target/src/` exists, treat as collection; otherwise, single template at `target`.
- For collections: publish per-template artifacts and then publish collection metadata tag at `<registry>/<namespace>:latest`.
- Stdout prints a map keyed by template id with publish results.

### Current Behavior
- Single-template only, wrong flags, wrong output shape.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/templates.rs` — Update `publish` flags and positional; wire to core.
- `crates/core/src/templates.rs` or new `crates/core/src/templates_publish.rs` — Implement packaging (tgz) per template and collection metadata generation.
- `crates/core/src/oci.rs` — Ensure helper functions support multi-tag push and annotation injection (see Task 05).

#### Specific Tasks
- [ ] Add `--namespace, -n <owner/repo>` required; default `--registry` to `ghcr.io`.
- [ ] Implement collection detection and iterate `src/*` subdirectories.
- [ ] Package templates and return metadata (id, version, archive path).
- [ ] Build stdout map keyed by template id.

### 2. Data Structures
```rust
pub struct PublishArgs { pub target: String, pub registry: String, pub namespace: String, pub log_level: String }

pub struct PublishEntry { pub published_tags: Option<Vec<String>>, pub digest: Option<String>, pub version: Option<String> }
pub type PublishOutput = std::collections::HashMap<String, PublishEntry>;
```

### 3. Validation Rules
- [ ] Fail if `--namespace` missing.
- [ ] Skip templates missing `version` with a warning; do not fail entire publish.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract.
- [ ] Theme 2 - CLI Validation rules.

## Testing Requirements

### Unit Tests
- [ ] Collection detection logic, packaging metadata extraction (id, version, files).

### Integration Tests
- [ ] Publish in single-template and collection modes; verify stdout map shape.

### Examples
- [ ] Add example under `examples/template-management/minimal-template/` for single and collection.

## Acceptance Criteria
- [ ] CLI flags correct; collection mode supported; stdout map emitted.
- [ ] CI checks pass.

## Definition of Done
- [ ] Implementation and tests complete; docs updated.
