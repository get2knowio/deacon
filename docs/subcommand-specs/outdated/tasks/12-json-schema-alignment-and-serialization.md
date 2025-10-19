# [outdated] JSON Schema Alignment and Serialization

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
Ensure the JSON output for `outdated` exactly matches the schema defined in DATA-STRUCTURES, including field names and optionality. Decide and implement serialization strategy for preserving feature order if required by examples/tests.

## Specification Reference

- From SPEC.md Section: §10 Output Specifications
- From GAP.md Section: 8.1 Deterministic Ordering; 12. Output Formats checklist

### Expected Behavior
- JSON emitted on stdout conforms exactly to:
  - `{ "features": { "<feature-id>": { "current"?: string, "wanted"?: string, "wantedMajor"?: string, "latest"?: string, "latestMajor"?: string } } }`.
- Optional fields omitted when undefined; not set to `null`.
- Stable key order if tests require (use `indexmap` or serialize from ordered vector).

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/outdated.rs`
  - Define serde field renames if struct fields in Rust use snake_case but JSON uses camelCase (e.g., `wantedMajor`).
  - Consider `indexmap::IndexMap` for `features` map to preserve insertion order during serialization.
- `crates/deacon/Cargo.toml`
  - Add `indexmap` if chosen.

#### Specific Tasks
- [ ] Add `#[serde(skip_serializing_if = "Option::is_none")]` to optional fields.
- [ ] Ensure camelCase JSON field names where specified.

### 2. Data Structures
```rust
#[derive(serde::Serialize)]
pub struct FeatureVersionInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wanted: Option<String>,
    #[serde(rename = "wantedMajor", skip_serializing_if = "Option::is_none")]
    pub wanted_major: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    #[serde(rename = "latestMajor", skip_serializing_if = "Option::is_none")]
    pub latest_major: Option<String>,
}
```

### 3. Validation Rules
- [ ] No `null` values in JSON; omit instead.

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract.

## Testing Requirements

### Unit Tests
- [ ] Serialization matches expected JSON (including camelCase and omission of missing fields).

### Integration Tests
- [ ] Compare entire JSON against expected fixture.

### Smoke Tests
- [ ] `--output-format json` on empty features prints `{ "features": {} }`.

### Examples
- [ ] Include example JSON in examples directory README.

## Acceptance Criteria
- [ ] JSON schema compliance ensured and tested.
- [ ] CI passes.

## Implementation Notes
- Prefer using `serde` attributes over manual JSON construction.

### Edge Cases to Handle
- Empty map, missing fields.

### References
- SPEC: §10
- GAP: §8.1, §12