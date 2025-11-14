---
subcommand: features-package
type: enhancement
priority: high
scope: medium
labels: ["subcommand: features-package", "type: enhancement", "priority: high", "scope: medium"]
---

# [features-package] Generate devcontainer-collection.json Metadata

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Create the `devcontainer-collection.json` file in the output directory after packaging. This metadata enables discovery and is required by the spec in both single and collection modes.

## Specification Reference

**From SPEC.md Section:** “§5. Core Execution Logic” and “§10. Output Specifications”

**From GAP.md Section:** “2. CRITICAL: Missing devcontainer-collection.json Generation”

### Expected Behavior
```
collection = {
  sourceInformation: { source: 'devcontainer-cli' },
  features: [ <metadata excerpt per feature> ]
}
write_file(<output>/devcontainer-collection.json, pretty_json(collection))
```

### Current Behavior
No collection metadata is written; only tar and an OCI manifest stub exist per feature.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs`
  - After collecting `Vec<FeatureMetadata>` from single/collection flows, map them into the DATA-STRUCTURES.md layout and write `devcontainer-collection.json`.
  - Ensure deterministic key ordering by using `BTreeMap` where needed before serialization.

#### Specific Tasks
- [ ] Define a local struct or serde_json construction that matches DATA-STRUCTURES.md exactly.
- [ ] Include fields: `id`, `version`, `name`, `description`, `options`, `installsAfter`, `dependsOn` for each feature object.
- [ ] Always write this file in both modes; overwrite if exists.
- [ ] Error on write failure with context: "Failed to write devcontainer-collection.json." (Theme 6)

### 2. Data Structures
From DATA-STRUCTURES.md:
```json
{
  "sourceInformation": { "source": "devcontainer-cli" },
  "features": [
    { "id": "<id>", "version": "<version>", "name": "<name>", "description": "<desc>",
      "options": { }, "installsAfter": [], "dependsOn": {} }
  ]
}
```

### 3. Validation Rules
- [ ] Validate non-empty features array; if empty, return error before writing.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] Theme 1 - JSON Output Contract: exact schema, stable key order.
- [ ] Theme 6 - Error Messages: standardized errors on failure.

## Testing Requirements

### Unit Tests
- [ ] Serialization unit test to snapshot `devcontainer-collection.json` shape for a sample metadata list.

### Integration Tests
- [ ] End-to-end package run produces `devcontainer-collection.json` with correct entries.
- [ ] Verify contents for both single and collection modes.

### Smoke Tests
- [ ] Ensure smoke covers existence of the file.

### Examples
- [ ] Update or add example under `examples/feature-management/` showing the file.

## Acceptance Criteria
- [ ] File written in both modes with exact schema and stable ordering.
- [ ] Tests validate shape; CI green.

## Implementation Notes
- Prefer `serde_json::to_writer_pretty` to avoid stray whitespace.
- Consider normalizing `options` and `dependsOn` maps via `BTreeMap`.

## References
- `docs/subcommand-specs/features-package/DATA-STRUCTURES.md`
- `docs/subcommand-specs/features-package/GAP.md` (Section 2)
- `docs/PARITY_APPROACH.md` (Theme 1, Theme 6)
