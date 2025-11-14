# [features info] Verbose mode: combine manifest + tags (+ dependencies text)

https://github.com/get2knowio/deacon/issues/338

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Implement verbose mode as a composition of manifest, tags, and dependencies modes. In JSON, include only `manifest`, `canonicalId`, and `publishedTags`. In text, print all three boxed sections including the dependency Mermaid graph.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (Verbose)
- From GAP.md Section: 2.4 Verbose Mode

### Expected Behavior
- Text: print boxed sections for Manifest, Canonical Identifier, Published Tags, and Dependency Tree.
- JSON: emit union of manifest and tags only (no dependency graph).
- Proper error handling in JSON: `{}` on failure.

### Current Behavior
- Custom non-compliant structure; no manifest or tags querying.

## Implementation Requirements

### 1. Code Changes Required
#### Files to Modify
- `crates/deacon/src/commands/features.rs`

#### Specific Tasks
- [ ] Delegate to manifest and tags mode functions to compute data
- [ ] If text format, also call dependencies text rendering
- [ ] Aggregate JSON structure `{manifest, canonicalId, publishedTags}`
- [ ] Ensure errors from any part are handled consistently

### 2. Data Structures
- Reuse types from manifest and tags implementations

### 3. Validation Rules
- [ ] Exit code 1 on any failure in JSON mode with `{}`

### 4. Cross-Cutting Concerns
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages

## Testing Requirements
- Integration Tests:
  - [ ] Verbose text output contains all sections
  - [ ] Verbose JSON matches schema
- Smoke Tests:
  - [ ] Add minimal verbose invocation

## Acceptance Criteria
- [ ] Verbose behaves as specified
- [ ] CI checks pass

## Dependencies
Blocked By: #335, #336, #337, #339
