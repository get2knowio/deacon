# [features info] Boxed text output formatter utility

https://github.com/get2knowio/deacon/issues/339

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [x] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Provide a small utility to print boxed text sections with Unicode box-drawing characters, used by manifest/tags/dependencies text outputs.

## Specification Reference
- From SPEC.md Section: §10. Output Specifications (Text formatting)
- From GAP.md Section: 3.4 Boxed Text Output Formatting

### Expected Behavior
- `print_boxed_section(title, content)` prints a box with a header line and the content below.
- Box width adapts to content width up to a reasonable maximum; simple implementation acceptable.

### Current Behavior
- No boxed formatting; plain text only.

## Implementation Requirements

### 1. Code Changes Required
#### Files to Modify
- `crates/core/src/` new `ui.rs` or similar utility module
- `crates/deacon/src/commands/features.rs` to use the utility

#### Specific Tasks
- [ ] Implement box drawing function
- [ ] Unit tests for basic rendering
- [ ] Avoid trailing spaces (rustfmt sensitive)

### 2. Data Structures
- N/A

### 3. Validation Rules
- [ ] Ensure outputs are stable for tests

### 4. Cross-Cutting Concerns
- [x] Theme 6 - Error Messages formatting consistency

## Testing Requirements
- Unit Tests:
  - [ ] Verify header and borders
  - [ ] Multi-line content rendering

## Acceptance Criteria
- [ ] Utility available and used by features info text outputs
- [ ] CI checks pass

## Dependencies
Blocks: #335, #336, #337
