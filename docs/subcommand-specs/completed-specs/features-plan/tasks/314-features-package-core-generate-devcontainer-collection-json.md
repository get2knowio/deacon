---
number: 314
title: "[Features Package] [Core] Generate `devcontainer-collection.json` (single + collection modes)"
author: pofallon
createdAt: 2025-10-13T23:44:26Z
updatedAt: 2025-10-13T23:44:26Z
labels:
  - type:enhancement
  - priority:high
  - scope:medium
  - subcommand:features-package
---

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: _n/a_

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Write `devcontainer-collection.json` to the output folder in both single and collection modes. Populate with `sourceInformation` and `features` array using metadata from packaged features.

## Specification Reference
**From SPEC.md Section:** §5 Core Execution Logic; §10 Output Specifications

**From GAP.md Section:** 2. CRITICAL: Missing `devcontainer-collection.json` Generation

### Expected Behavior
- After packaging, write `devcontainer-collection.json` with structure required by DATA-STRUCTURES.md.
- The features array includes exactly the features packaged in this run.

### Current Behavior
- No collection metadata is written.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Build metadata vector and write JSON file.
- Consider adding a small struct in a shared module for the collection schema.

#### Specific Tasks
- [ ] Define `CollectionMetadata { sourceInformation, features }`.
- [ ] Serialize to JSON with pretty formatting and write to `output/`.
- [ ] Ensure file is overwritten on re-run.

### 2. Data Structures
Use from DATA-STRUCTURES.md:
```json
{
  "sourceInformation": { "source": "devcontainer-cli" },
  "features": [ { "id": "...", "version": "...", "name": "...", "description": "...", "options": {}, "installsAfter": [], "dependsOn": {} } ]
}
```

### 3. Validation Rules
- [ ] Error if metadata vector is empty.

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract (file content shape).
- [ ] Theme 6 - Error Messages.

## Testing Requirements
- [ ] Tests assert file existence and JSON structure equality.
- [ ] Single mode: features array of length 1; collection mode: N entries.

## Acceptance Criteria
- [ ] File generated correctly in both modes.
- [ ] Tests pass; CI green.

## Dependencies
- Tracks: #310
- Blocked By: #313 (Collection packaging produces metadata list)
