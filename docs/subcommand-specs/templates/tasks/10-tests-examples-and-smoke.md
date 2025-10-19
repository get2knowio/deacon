---
subcommand: templates
type: enhancement
priority: high
scope: medium
labels: ["subcommand: templates", "type: enhancement", "priority: high", "scope: medium", "testing"]
---

# [templates] Add Comprehensive Tests, Fixtures, and Examples

## Issue Type
- [x] Testing & Validation

## Description
Add unit, integration, and smoke tests to cover spec-defined behavior for `templates` subcommands, and provide curated examples and fixtures demonstrating apply with options, feature injection, publish collection, and metadata retrieval from registry annotations.

## Specification Reference

**From SPEC.md Section:** §15 Testing Strategy

**From GAP.md Section:** 5.1 Spec-Compliant Tests (missing cases)

### Expected Behavior
- Tests follow the suite outlined in SPEC and validate JSON output contracts, error handling, and OCI interactions via mocks.

## Implementation Requirements

### 1. Code Changes Required
- `crates/deacon/tests/` — Add/extend integration tests for `apply`, `publish`, `metadata`.
- `crates/core/tests/` — Add unit tests for substitution, omit paths, publish tag computation.
- `crates/deacon/tests/smoke_basic.rs` — Extend with minimal `templates` invocations resilient to missing Docker (not needed) and offline registry (mocked).

### 2. Fixtures and Examples
- `fixtures/templates/minimal/` and `fixtures/templates/with-options/` — Template folders used in tests.
- `examples/template-management/minimal-template/` and `templates-with-options/` — User-facing docs.

### 3. Cross-Cutting Concerns
- [ ] Theme 1 - JSON Output Contract in assertions.
- [ ] Theme 6 - Error messages exact match.

## Acceptance Criteria
- [ ] Tests pass locally and in CI.
- [ ] Examples build/run with CLI.

## Definition of Done
- [ ] Coverage of all scenarios listed in SPEC §15.
