# Quickstart: Fix Config Merge Rules

**Branch**: `013-fix-merge-rules` | **Date**: 2026-02-22

## What Changed

Fixed two categories of property merge bugs in `ConfigMerger::merge_two_configs()`:

1. **Boolean OR**: `privileged` and `init` now use OR semantics — if any source sets `true`, the result is `true`.
2. **Array Union**: `mounts` and `forwardPorts` now use union with deduplication — entries from all sources are preserved.

## Files Modified

- `crates/core/src/config.rs` — `merge_two_configs()`, plus three new helpers: `merge_bool_or()`, `union_json_arrays()`, `union_port_arrays()`

## How to Verify

```bash
# Run all tests
make test-nextest-fast

# Run specific merge tests
cargo nextest run test_merge_bool_or
cargo nextest run test_merge_mounts_union
cargo nextest run test_merge_forward_ports_union
cargo nextest run test_merge_chain
```

## Before/After

### Boolean merge (privileged)

```
Before: base=Some(true), overlay=Some(false) → Some(false)  [WRONG]
After:  base=Some(true), overlay=Some(false) → Some(true)   [CORRECT]
```

### Array merge (mounts)

```
Before: base=[A, B], overlay=[C, D] → [C, D]         [WRONG - base lost]
After:  base=[A, B], overlay=[C, D] → [A, B, C, D]   [CORRECT - union]
```

### Array merge (forwardPorts)

```
Before: base=[3000], overlay=[8080] → [8080]         [WRONG - base lost]
After:  base=[3000], overlay=[8080] → [3000, 8080]   [CORRECT - union]
```
