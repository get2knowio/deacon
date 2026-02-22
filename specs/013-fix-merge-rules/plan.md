# Implementation Plan: Fix Config Merge Rules

**Branch**: `013-fix-merge-rules` | **Date**: 2026-02-22 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/013-fix-merge-rules/spec.md`

## Summary

Fix two categories of property merge bugs in `ConfigMerger::merge_two_configs()` to match the upstream containers.dev specification:
1. **Boolean properties** (`privileged`, `init`): Change from last-wins (`Option::or()`) to OR semantics (true if either source is true).
2. **Array properties** (`mounts`, `forwardPorts`): Change from replace-if-non-empty to union with deduplication.

The fix is surgical — three new private helper functions added to `ConfigMerger`, four lines changed in `merge_two_configs()`, comprehensive tests added. No new dependencies.

## Technical Context

**Language/Version**: Rust 1.70+ (Edition 2021)
**Primary Dependencies**: `serde_json` (Value PartialEq for mount dedup), `clap`, `tracing`
**Storage**: N/A
**Testing**: `cargo-nextest` via `make test-nextest-fast`
**Target Platform**: Linux (Docker/Podman host)
**Project Type**: CLI (Rust workspace: `crates/core` library + `crates/deacon` binary)
**Performance Goals**: N/A — merge operates on small arrays (< 20 entries), O(n²) dedup is negligible
**Constraints**: Must not break any existing merge behavior (FR-007)
**Scale/Scope**: Single file change (`crates/core/src/config.rs`), ~50 lines implementation + ~200 lines tests

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity | **PASS** | Fix aligns merge behavior with upstream containers.dev spec (boolean OR, array union) |
| II. Consumer-Only Scope | **PASS** | Config merging is core consumer functionality used by `up`, `read-configuration`, `build` |
| III. Keep Build Green | **PASS** | All existing tests must continue to pass; new tests added for fixed behavior |
| IV. No Silent Fallbacks | **PASS** | Fix removes a silent fallback (booleans silently losing `true`, arrays silently dropping entries) |
| V. Idiomatic Rust | **PASS** | Helper functions use `Option<bool>` pattern matching; no unsafe, no unwrap |
| VI. Observability | **N/A** | Internal merge logic, no output format changes |
| VII. Testing Completeness | **PASS** | All spec acceptance scenarios will have corresponding unit tests |
| VIII. Subcommand Consistency | **PASS** | Fix is in shared `ConfigMerger` used by all subcommands — no per-command changes needed |
| IX. Executable Examples | **N/A** | No example changes required for internal merge fix |

**Post-Design Re-check**: All gates still pass. No new dependencies, no new modules, no architecture changes.

## Project Structure

### Documentation (this feature)

```text
specs/013-fix-merge-rules/
├── plan.md              # This file
├── research.md          # Phase 0: decisions and rationale
├── data-model.md        # Phase 1: merge truth tables and algorithms
├── quickstart.md        # Phase 1: verification guide
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
crates/core/src/
└── config.rs            # ConfigMerger::merge_two_configs() — the only file modified
                         # New helpers: merge_bool_or(), union_json_arrays(), union_port_arrays()
                         # New tests in mod tests block
```

**Structure Decision**: Single file modification in existing `crates/core/src/config.rs`. The `ConfigMerger` impl block already contains all merge helpers — new helpers follow the same pattern (`concat_string_arrays`, `merge_string_maps`, etc.).

## Complexity Tracking

No constitution violations. No complexity justifications needed.

## Implementation Approach

### Step 1: Add Helper Functions

Add three private helper functions to the `ConfigMerger` impl block:

1. **`merge_bool_or(base: Option<bool>, overlay: Option<bool>) -> Option<bool>`**
   - Pattern match on `(base, overlay)`:
     - `(None, None)` → `None`
     - `(Some(true), _) | (_, Some(true))` → `Some(true)`
     - `(Some(false), Some(false))` → `Some(false)`
     - `(Some(v), None) | (None, Some(v))` → `Some(v)`

2. **`union_json_arrays(base: &[serde_json::Value], overlay: &[serde_json::Value]) -> Vec<serde_json::Value>`**
   - Start with base clone, append overlay entries not in base (using `serde_json::Value::eq`)

3. **`union_port_arrays(base: &[PortSpec], overlay: &[PortSpec]) -> Vec<PortSpec>`**
   - Start with base clone, append overlay entries not in base (using `PortSpec::eq`)

### Step 2: Update merge_two_configs

Replace four field assignments in `merge_two_configs()`:

```rust
// Before (boolean last-wins):
privileged: overlay.privileged.or(base.privileged),
init: overlay.init.or(base.init),

// After (boolean OR):
privileged: Self::merge_bool_or(base.privileged, overlay.privileged),
init: Self::merge_bool_or(base.init, overlay.init),

// Before (array replace):
mounts: if overlay.mounts.is_empty() { base.mounts.clone() } else { overlay.mounts.clone() },
forward_ports: if overlay.forward_ports.is_empty() { base.forward_ports.clone() } else { overlay.forward_ports.clone() },

// After (array union):
mounts: Self::union_json_arrays(&base.mounts, &overlay.mounts),
forward_ports: Self::union_port_arrays(&base.forward_ports, &overlay.forward_ports),
```

### Step 3: Add Tests

Unit tests covering all spec acceptance scenarios:

**Boolean OR tests** (FR-001, FR-006, FR-008):
- `test_merge_bool_or_both_none` → `None`
- `test_merge_bool_or_true_false` → `Some(true)`
- `test_merge_bool_or_false_true` → `Some(true)`
- `test_merge_bool_or_true_none` → `Some(true)`
- `test_merge_bool_or_none_true` → `Some(true)`
- `test_merge_bool_or_false_false` → `Some(false)`
- `test_merge_bool_or_none_false` → `Some(false)`
- `test_merge_bool_or_false_none` → `Some(false)`
- `test_merge_privileged_or_semantics` — integration via `merge_two_configs`
- `test_merge_init_or_semantics` — integration via `merge_two_configs`

**Array union tests** (FR-002, FR-003, FR-004, FR-005):
- `test_merge_mounts_union_disjoint` → all entries preserved
- `test_merge_mounts_union_with_duplicates` → dedup by JSON equality
- `test_merge_mounts_union_base_empty` → overlay entries
- `test_merge_mounts_union_overlay_empty` → base entries
- `test_merge_mounts_union_both_empty` → empty
- `test_merge_forward_ports_union_disjoint` → all entries preserved
- `test_merge_forward_ports_union_with_duplicates` → dedup by PortSpec equality
- `test_merge_forward_ports_union_mixed_types` → Number and String kept distinct

**Chain merge tests** (FR-008):
- `test_merge_chain_bool_or` — three configs, boolean OR across chain
- `test_merge_chain_array_union` — three configs, array union across chain

**Regression test** (FR-007):
- `test_merge_other_categories_unchanged` — verify scalars, maps, concat arrays unchanged

### Step 4: Verify

```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
make test-nextest-fast
```

## Pre-Implementation Checklist

1. **Spec Review**: Read complete upstream merge rules at containers.dev/implementors/spec/ ✓
2. **Scope Check**: Config merging is consumer functionality (Principle II) ✓
3. **Data Model Alignment**: No struct changes, only merge behavior ✓
4. **Algorithm Alignment**: Boolean OR and array union match upstream spec exactly ✓
5. **Input Validation**: N/A — inputs are already-parsed DevContainerConfig objects ✓
6. **Configuration Resolution**: N/A — merge operates on already-resolved configs ✓
7. **Output Contracts**: No output format changes ✓
8. **Testing Coverage**: All 8 FR acceptance scenarios have corresponding tests ✓
9. **Infrastructure Reuse**: Uses existing `ConfigMerger` impl block and test patterns ✓
10. **Nextest Configuration**: New tests are unit tests in config.rs — no new nextest group config needed ✓
