# [features publish] JSON output contract: featureId and publishedTags; logs vs stdout

https://github.com/get2knowio/deacon/issues/329

<!-- Labels: subcommand:features-publish, type:enhancement, priority:medium, scope:small -->

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Testing & Validation

## Parent Issue
Tracks: #321 (tracking issue)

## Description
Align JSON output with DATA-STRUCTURES: include `featureId`, `digest`, and `publishedTags` array. Ensure JSON is printed to stdout and human-readable logs go to stderr.

## Specification Reference
**From DATA-STRUCTURES.md:** Publish Result Schema

**From GAP.md Section:** 4. Output Format Gaps – Incomplete JSON Output Structure

### Expected Behavior
```json
{
  "featureId": "<id>",
  "digest": "sha256:...",
  "publishedTags": ["1", "1.2", "1.2.3", "latest"]
}
```

### Current Behavior
- Partial JSON with `digest` only and extra fields; lacks `featureId` and `publishedTags`

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/features.rs` — Update JSON emit logic
- `crates/deacon/src/cli.rs` — Ensure `--json` wiring honors stdout/stderr separation

#### Specific Tasks
- [ ] Track published tags and feature ID
- [ ] Serialize exactly per schema; avoid extra fields when in JSON mode
- [ ] Route JSON to stdout, logs to stderr (Theme 1)

### 2. Data Structures
- Define a small struct matching schema in deacon crate for output serialization

### 3. Validation Rules
- [ ] No trailing whitespace/newlines in JSON

### 4. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract

## Testing Requirements

### Unit Tests
- [ ] Serialization matches schema

### Integration Tests
- [ ] End-to-end JSON mode prints schema to stdout

## Acceptance Criteria
- [ ] JSON output matches spec exactly
- [ ] CI passes

## Dependencies

**Blocked By:** #323 (to produce publishedTags)
