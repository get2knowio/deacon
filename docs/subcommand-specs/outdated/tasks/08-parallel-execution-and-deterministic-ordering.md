# [outdated] Parallel Execution and Deterministic Ordering

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
Implement bounded-parallel per-feature resolution (tag listing and metadata) and ensure final output order matches the declaration order from the devcontainer configuration.

## Specification Reference

- From SPEC.md Section: §5. Core Execution Logic (PARALLEL_FOR); §6. State Management (deterministic order)
- From GAP.md Section: 8. Cross-Cutting Concerns (Deterministic Ordering, Parallel Execution)

### Expected Behavior
- Use a concurrency-limited stream (e.g., `futures::stream::iter(...).buffer_unordered(N)`) to resolve features concurrently.
- Reconstruct ordered results to exactly match config order.

### Current Behavior
- Not implemented.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/outdated.rs`
  - Introduce bounded parallelism for per-feature work (N≈8).
  - Reorder collected results by config order before rendering.

#### Specific Tasks
- [ ] Implement feature list extraction preserving config order.
- [ ] Ensure output order is deterministic across runs.

### 2. Data Structures
- Use `Vec<(id, info)>` for intermediate, then fold into `LinkedHashMap`-like order or standard `HashMap` plus explicit ordering when rendering/serialization.

### 3. Validation Rules
- [ ] None.

### 4. Cross-Cutting Concerns
- Theme 1 - JSON Output Contract: ordering matters for readability though JSON map order is not guaranteed—preserve order during serialization with an ordered map if needed, or emit as object in config order by serializer that preserves insertion order.

## Testing Requirements

### Unit Tests
- [ ] Ordering test: shuffled processing still yields ordered output.

### Integration Tests
- [ ] Parallel path exercised with multiple features and artificial delay.

### Smoke Tests
- [ ] N/A.

### Examples
- [ ] N/A.

## Acceptance Criteria
- [ ] Parallel execution implemented with bound.
- [ ] Deterministic order ensured.
- [ ] CI passes.

## Implementation Notes
- Consider `indexmap::IndexMap` if JSON serialization must reflect order; otherwise control order in table rendering and ensure JSON tests accept order-insensitive comparisons.

### Edge Cases to Handle
- Duplicate feature IDs (should not happen; if present, last wins or consistent behavior per config parser).

### References
- SPEC: §5, §6
- GAP: §8