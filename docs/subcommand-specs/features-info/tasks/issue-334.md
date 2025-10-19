# [features info] CLI flags: add --output-format and --log-level; enforce validation

https://github.com/get2knowio/deacon/issues/334

## Issue Type
- [x] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Error Handling
- [x] Testing & Validation

## Parent Issue
Tracks: #333 (tracking issue)

## Description
Replace the non-spec `--json` boolean with `--output-format <text|json>` and add `--log-level <info|debug|trace>`. Enforce validation and defaults per SPEC. This aligns with Consistency Theme 1 (JSON output contract) and Theme 2 (CLI validation).

## Specification Reference
- From SPEC.md Section: §2. Command-Line Interface
- From GAP.md Section: 1.1 CLI Flags and Options

### Expected Behavior
- `--output-format` accepts `text` (default) or `json`.
- `--log-level` accepts `info` (default), `debug`, `trace`.
- Unknown values result in error: "Invalid value for --output-format." or clap default messages.

### Current Behavior
- Boolean `--json` flag exists; `--log-level` missing.

## Implementation Requirements

### 1. Code Changes Required
#### Files to Modify
- `crates/deacon/src/cli.rs` (or wherever subcommand args are defined)
- `crates/deacon/src/commands/features.rs` (parse and map flags for `info`)

#### Specific Tasks
- [ ] Remove `--json` and introduce `--output-format <text|json>`
- [ ] Add `--log-level <info|debug|trace>` with default `info`
- [ ] Map to an enum `OutputFormat { Text, Json }`
- [ ] Wire `--log-level` to tracing initialization for this command
- [ ] Update help text to match SPEC

### 2. Data Structures
Add enum in CLI or shared module:
```rust
#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum OutputFormat { Text, Json }
```

### 3. Validation Rules
- [ ] Default format = text
- [ ] Unsupported values rejected by clap
- [ ] Error messages standardized per Theme 6

### 4. Cross-Cutting Concerns
- [x] Theme 1 - JSON Output Contract: Adds proper selector
- [x] Theme 2 - CLI Validation: clap `ValueEnum`
- [x] Theme 6 - Error Messages: sentence case

## Testing Requirements
- Unit Tests:
  - [ ] Parsing for default and explicit values
  - [ ] Invalid values rejected
- Integration Tests:
  - [ ] `features info manifest <ref> --output-format json` yields JSON
- Smoke Tests:
  - [ ] Update smoke flow if applicable
- Examples:
  - [ ] Update examples and README help excerpts

## Acceptance Criteria
- [ ] Flags implemented with correct types and defaults
- [ ] No `--json` usage remains
- [ ] All CI checks pass (build, test, fmt, clippy)

## Dependencies
Blocked By: None
