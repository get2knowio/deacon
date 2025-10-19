# [features info] Dependencies mode: generate Mermaid graph (text only)

https://github.com/get2knowio/deacon/issues/337

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Implement dependencies mode as text-only Mermaid diagram based on `dependsOn` and `installsAfter` from feature metadata. Remove JSON output for this mode. Include a boxed section header and a render hint.

## Specification Reference
- From SPEC.md Section: §5. Core Execution Logic (Dependencies)
- From GAP.md Section: 2.3 Dependencies Mode

### Expected Behavior
- Text output only: boxed header "Dependency Tree (Render with https://mermaid.live/)" followed by `graph TD` Mermaid syntax.
- No JSON output for dependencies mode; in JSON mode, either skip or emit error depending on spec decision (prefer skip as in TS reference).

### Current Behavior
- Outputs simple lists and allows JSON; no Mermaid generation.

## Implementation Requirements

### 1. Code Changes Required
#### Files to Modify
- `crates/deacon/src/commands/features.rs` (dependencies mode)
- `crates/core/src/config.rs` or new util module for graph building

#### Specific Tasks
- [ ] Build immediate dependency edges from metadata
- [ ] Optionally include transitive dependencies (if easily resolvable)
- [ ] Generate Mermaid `graph TD` edges
- [ ] Print boxed section and render hint
- [ ] Ensure JSON mode does not emit dependency graph

### 2. Data Structures
- Graph edges as pairs of strings; output as Mermaid text

### 3. Validation Rules
- [ ] Handle no dependencies gracefully (empty graph with comment)

### 4. Cross-Cutting Concerns
- [x] Theme 1 - JSON Output Contract (no graph in JSON)
- [x] Theme 6 - Error Messages

## Testing Requirements
- Unit Tests:
  - [ ] Mermaid generation from sample metadata
  - [ ] No-dependency case
- Integration Tests:
  - [ ] Combine with verbose mode text output
- Smoke Tests:
  - [ ] Basic `features info dependencies` invocation
- Examples:
  - [ ] Include output snippet in examples

## Acceptance Criteria
- [ ] Mermaid diagram output matches spec
- [ ] No JSON emitted for dependencies mode
- [ ] CI checks pass

## Dependencies
Blocked By: #339 (Boxed text formatter)
