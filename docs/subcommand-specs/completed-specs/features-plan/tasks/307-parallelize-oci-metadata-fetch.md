---
title: "[features-plan] Performance: parallelize OCI metadata fetch with concurrency limit"
issue: 307
labels:
  - performance
  - type:enhancement
  - scope:medium
  - subcommand:features-plan
created: 2025-10-13T23:29:23Z
author: pofallon
---

## Summary
Introduce parallel OCI metadata fetch with a small bounded concurrency (e.g., 4–8) to reduce total latency when many features are declared. Keep ordering deterministic.

## Details

Issue Type
- Core Logic Implementation
- Performance

Parent Issue
Tracks: #298 (tracking issue)

Specification Reference
- From SPEC.md Section: §11 Performance Considerations — could parallelize with concurrency limit
- From GAP.md Section: 10. Performance Considerations — Missing parallelization

Expected Behavior
- Fetch feature metadata concurrently with a bounded concurrency, collect results deterministically (sorted by key or stable order), and proceed to resolution.

Current Behavior
- Serial fetch loop.

Implementation Requirements

1. Code Changes Required

Files to Modify
- `crates/deacon/src/commands/features.rs` — fetching loop

Specific Tasks
- Use `futures::stream` with `buffer_unordered(N)` or `try_for_each_concurrent` with stable collection into `BTreeMap`
- Preserve deterministic order after fetch by sorting keys
- Add a feature flag or environment variable to control concurrency limit (optional)

Cross-Cutting Concerns
- Keep code simple; avoid complex error fan-in

Testing Requirements

Unit/Integration Tests
- Simulate multiple features and ensure order/graph identical to serial fetch

Acceptance Criteria
- Concurrency implemented; behavior deterministic

References
- SPEC: `docs/subcommand-specs/features-plan/SPEC.md` (§11)
- GAP: `docs/subcommand-specs/features-plan/GAP.md` (§10)
