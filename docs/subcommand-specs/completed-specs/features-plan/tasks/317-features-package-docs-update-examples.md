---
number: 317
title: "[Features Package] [Docs] Update examples and README for collection packaging"
author: pofallon
createdAt: 2025-10-13T23:44:48Z
updatedAt: 2025-10-13T23:44:48Z
labels:
  - priority:medium
  - scope:small
  - subcommand:features-package
  - type:docs
---

## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [x] Other: Documentation

## Parent Issue
Tracks: #310 (tracking issue)

## Description
Add or update examples under `examples/feature-management/` to illustrate single-feature and collection packaging, including use of the `-f` flag and the default positional `target`. Update `examples/README.md` index.

## Specification Reference
**From SPEC.md Section:** §10 Output Specifications; §15 Testing Strategy (as guidance)

**From GAP.md Section:** References to examples needed for multi-feature repos.

## Implementation Requirements
- [ ] Add collection example with `src/` and 2 features.
- [ ] Show expected output artifacts including `devcontainer-collection.json`.
- [ ] Update README with usage snippets.

## Acceptance Criteria
- [ ] Examples build and run with the CLI.
- [ ] Documentation is clear and follows repository conventions.

## Dependencies
- Tracks: #310
- Blocked By: #313, #314
