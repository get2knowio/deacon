# Tasks: Fix Config Merge Rules

**Input**: Design documents from `/specs/013-fix-merge-rules/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, quickstart.md

**Tests**: Included — spec.md defines acceptance scenarios and plan.md specifies comprehensive test coverage.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Rust workspace**: `crates/core/src/` (library), `crates/deacon/src/` (binary)
- All changes for this feature are in `crates/core/src/config.rs`

---

## Phase 1: Setup

**Purpose**: No project initialization needed — this is a surgical bugfix in an existing codebase.

- [X] T001 Read and understand current `merge_two_configs()` implementation at `crates/core/src/config.rs:1053` including the existing helper pattern (`concat_string_arrays`, `merge_string_maps`, `merge_json_objects`)

---

## Phase 2: Foundational (Helper Functions)

**Purpose**: Add three private helper functions to the `ConfigMerger` impl block in `crates/core/src/config.rs`. These helpers are shared prerequisites for all user stories.

**CRITICAL**: No user story work can begin until this phase is complete.

- [X] T002 Implement `merge_bool_or(base: Option<bool>, overlay: Option<bool>) -> Option<bool>` helper in `crates/core/src/config.rs` using pattern matching per data-model.md truth table: `Some(true)` if either is `Some(true)`, `Some(false)` if both `Some(false)`, `None` if both `None`, pass-through for mixed `Some`/`None`
- [X] T003 Implement `union_json_arrays(base: &[serde_json::Value], overlay: &[serde_json::Value]) -> Vec<serde_json::Value>` helper in `crates/core/src/config.rs` — start with base clone, append overlay entries not already present using `serde_json::Value` structural equality
- [X] T004 Implement `union_port_arrays(base: &[PortSpec], overlay: &[PortSpec]) -> Vec<PortSpec>` helper in `crates/core/src/config.rs` — start with base clone, append overlay entries not already present using derived `PartialEq`

**Checkpoint**: All three helpers compiled and ready. Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings` to verify.

---

## Phase 3: User Story 1 — Feature requiring privileged mode merges correctly (Priority: P1) MVP

**Goal**: `privileged` boolean uses OR semantics in `merge_two_configs()` — if any source sets `true`, the merged result is `true`.

**Independent Test**: Merge two configs where base has `privileged: true` and overlay has `privileged: false`, verify merged result is `true`.

### Tests for User Story 1

- [X] T005 [US1] Add unit test `test_merge_bool_or_both_none` in `crates/core/src/config.rs` — verify `merge_bool_or(None, None)` returns `None` (FR-006)
- [X] T006 [US1] Add unit test `test_merge_bool_or_true_false` in `crates/core/src/config.rs` — verify `merge_bool_or(Some(true), Some(false))` returns `Some(true)` (FR-001)
- [X] T007 [US1] Add unit test `test_merge_bool_or_false_true` in `crates/core/src/config.rs` — verify `merge_bool_or(Some(false), Some(true))` returns `Some(true)` (FR-001)
- [X] T008 [US1] Add unit test `test_merge_bool_or_true_none` in `crates/core/src/config.rs` — verify `merge_bool_or(Some(true), None)` returns `Some(true)`
- [X] T009 [US1] Add unit test `test_merge_bool_or_none_true` in `crates/core/src/config.rs` — verify `merge_bool_or(None, Some(true))` returns `Some(true)`
- [X] T010 [US1] Add unit test `test_merge_bool_or_false_false` in `crates/core/src/config.rs` — verify `merge_bool_or(Some(false), Some(false))` returns `Some(false)`
- [X] T011 [US1] Add unit test `test_merge_bool_or_none_false` in `crates/core/src/config.rs` — verify `merge_bool_or(None, Some(false))` returns `Some(false)`
- [X] T012 [US1] Add unit test `test_merge_bool_or_false_none` in `crates/core/src/config.rs` — verify `merge_bool_or(Some(false), None)` returns `Some(false)`

### Implementation for User Story 1

- [X] T013 [US1] Update `privileged` field assignment in `merge_two_configs()` at `crates/core/src/config.rs:1179` — replace `overlay.privileged.or(base.privileged)` with `Self::merge_bool_or(base.privileged, overlay.privileged)`
- [X] T014 [US1] Add integration test `test_merge_privileged_or_semantics` in `crates/core/src/config.rs` — test via `merge_two_configs` with base `privileged: true` + overlay `privileged: false`, verify merged result is `Some(true)` per spec acceptance scenario 1

**Checkpoint**: `privileged` OR merge works. Run `cargo nextest run test_merge_bool_or && cargo nextest run test_merge_privileged`.

---

## Phase 4: User Story 2 — Feature adding mounts preserves existing mounts (Priority: P1)

**Goal**: `mounts` array uses union with deduplication — entries from all sources are preserved, duplicates removed by JSON structural equality.

**Independent Test**: Merge two configs with distinct mounts `[A, B]` and `[C, D]`, verify result is `[A, B, C, D]`.

### Tests for User Story 2

- [X] T015 [US2] Add unit test `test_merge_mounts_union_disjoint` in `crates/core/src/config.rs` — base `[A, B]`, overlay `[C, D]`, verify result `[A, B, C, D]` (FR-002)
- [X] T016 [US2] Add unit test `test_merge_mounts_union_with_duplicates` in `crates/core/src/config.rs` — base `[A, B]`, overlay `[B, C]`, verify result `[A, B, C]` with B not duplicated (FR-004)
- [X] T017 [US2] Add unit test `test_merge_mounts_union_base_empty` in `crates/core/src/config.rs` — base `[]`, overlay `[A]`, verify result `[A]`
- [X] T018 [US2] Add unit test `test_merge_mounts_union_overlay_empty` in `crates/core/src/config.rs` — base `[A]`, overlay `[]`, verify result `[A]`
- [X] T019 [US2] Add unit test `test_merge_mounts_union_both_empty` in `crates/core/src/config.rs` — both `[]`, verify result `[]`

### Implementation for User Story 2

- [X] T020 [US2] Update `mounts` field assignment in `merge_two_configs()` at `crates/core/src/config.rs:1107-1111` — replace `if overlay.mounts.is_empty() { base.mounts.clone() } else { overlay.mounts.clone() }` with `Self::union_json_arrays(&base.mounts, &overlay.mounts)`

**Checkpoint**: `mounts` union works. Run `cargo nextest run test_merge_mounts`.

---

## Phase 5: User Story 3 — Feature adding forwarded ports preserves existing ports (Priority: P1)

**Goal**: `forwardPorts` array uses union with deduplication — all unique ports from both configs preserved, duplicates removed by `PortSpec::PartialEq`.

**Independent Test**: Merge two configs with `forwardPorts: [3000, 8080]` and `forwardPorts: [5432, 6379]`, verify result is `[3000, 8080, 5432, 6379]`.

### Tests for User Story 3

- [X] T021 [US3] Add unit test `test_merge_forward_ports_union_disjoint` in `crates/core/src/config.rs` — base `[3000, 8080]`, overlay `[5432, 6379]`, verify result `[3000, 8080, 5432, 6379]` (FR-003)
- [X] T022 [US3] Add unit test `test_merge_forward_ports_union_with_duplicates` in `crates/core/src/config.rs` — base `[3000, 8080]`, overlay `[8080, 5432]`, verify result `[3000, 8080, 5432]` with 8080 not duplicated (FR-005)
- [X] T023 [US3] Add unit test `test_merge_forward_ports_union_mixed_types` in `crates/core/src/config.rs` — base `[Number(3000)]`, overlay `[String("3000:3000")]`, verify both kept as distinct entries per edge case spec

### Implementation for User Story 3

- [X] T024 [US3] Update `forward_ports` field assignment in `merge_two_configs()` at `crates/core/src/config.rs:1112-1115` — replace `if overlay.forward_ports.is_empty() { base.forward_ports.clone() } else { overlay.forward_ports.clone() }` with `Self::union_port_arrays(&base.forward_ports, &overlay.forward_ports)`

**Checkpoint**: `forwardPorts` union works. Run `cargo nextest run test_merge_forward_ports`.

---

## Phase 6: User Story 4 — Init boolean merges with OR semantics (Priority: P2)

**Goal**: `init` property follows the same boolean OR merge rules as `privileged` — if any source sets `init: true`, the merged result is `true`.

**Independent Test**: Merge two configs where one has `init: true` and the other has `init: false`, verify merged result is `true`.

### Tests for User Story 4

- [X] T025 [US4] Add integration test `test_merge_init_or_semantics` in `crates/core/src/config.rs` — test via `merge_two_configs` with base `init: true` + overlay `init: false`, verify merged result is `Some(true)` (FR-001)

### Implementation for User Story 4

- [X] T026 [US4] Update `init` field assignment in `merge_two_configs()` at `crates/core/src/config.rs:1180` — replace `overlay.init.or(base.init)` with `Self::merge_bool_or(base.init, overlay.init)`

**Checkpoint**: `init` OR merge works. Run `cargo nextest run test_merge_init`.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Chain merge tests, regression guard, and full validation across all stories.

- [X] T027 Add chain merge test `test_merge_chain_bool_or` in `crates/core/src/config.rs` — merge three configs where config1 has `privileged: false`, config2 has `privileged: true`, config3 has `privileged: false`, verify final result is `Some(true)` (FR-008)
- [X] T028 Add chain merge test `test_merge_chain_array_union` in `crates/core/src/config.rs` — merge three configs with distinct mounts `[A]`, `[B]`, `[C]`, verify final result is `[A, B, C]` (FR-008)
- [X] T029 Add regression test `test_merge_other_categories_unchanged` in `crates/core/src/config.rs` — verify scalars use last-wins, maps use key-merge, concat arrays use concatenation — all unchanged by this fix (FR-007)
- [X] T030 Run full verification: `cargo fmt --all && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && make test-nextest-fast` (SC-004, SC-005)
- [X] T031 Run quickstart.md validation scenarios per `specs/013-fix-merge-rules/quickstart.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — read-only orientation
- **Foundational (Phase 2)**: Depends on Phase 1 — adds helper functions that all stories use
- **US1 (Phase 3)**: Depends on Phase 2 (needs `merge_bool_or` helper)
- **US2 (Phase 4)**: Depends on Phase 2 (needs `union_json_arrays` helper)
- **US3 (Phase 5)**: Depends on Phase 2 (needs `union_port_arrays` helper)
- **US4 (Phase 6)**: Depends on Phase 2 (needs `merge_bool_or` helper — same as US1)
- **Polish (Phase 7)**: Depends on all user story phases (3–6) being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational — no dependency on other stories
- **User Story 2 (P1)**: Can start after Foundational — no dependency on other stories
- **User Story 3 (P1)**: Can start after Foundational — no dependency on other stories
- **User Story 4 (P2)**: Can start after Foundational — no dependency on other stories

**Note**: While stories are logically independent, all changes are in the same file (`crates/core/src/config.rs`), so parallel execution requires careful merge coordination. Sequential execution in priority order (US1 → US2 → US3 → US4) is recommended for a single implementer.

### Within Each User Story

- Tests written first, verified to compile (they will fail until implementation)
- Implementation updates the merge line in `merge_two_configs()`
- Tests re-run and verified passing
- Checkpoint verification before moving to next story

---

## Parallel Example: Foundational Phase

```bash
# All three helpers can be written together (same file, adjacent location):
Task: "Implement merge_bool_or helper in crates/core/src/config.rs"
Task: "Implement union_json_arrays helper in crates/core/src/config.rs"
Task: "Implement union_port_arrays helper in crates/core/src/config.rs"
```

## Parallel Example: User Stories (with merge coordination)

```bash
# After Foundational phase, stories are logically parallelizable:
Agent A: "US1 — privileged boolean OR (Phase 3)"
Agent B: "US2 — mounts array union (Phase 4)"
# Note: Requires file-level merge coordination since both touch config.rs
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Read existing code
2. Complete Phase 2: Add all three helper functions
3. Complete Phase 3: US1 — privileged boolean OR with tests
4. **STOP and VALIDATE**: Run `cargo nextest run test_merge_bool_or && cargo nextest run test_merge_privileged`
5. Verify the critical `base=true, overlay=false → true` case passes

### Incremental Delivery

1. Phase 2 (Foundational) → All helpers ready
2. Phase 3 (US1: privileged OR) → Test independently → Core bug fixed
3. Phase 4 (US2: mounts union) → Test independently → Mount preservation fixed
4. Phase 5 (US3: forwardPorts union) → Test independently → Port preservation fixed
5. Phase 6 (US4: init OR) → Test independently → Full boolean OR coverage
6. Phase 7 (Polish) → Chain tests, regression guard, full validation

### Suggested MVP Scope

**US1 + US4 together** (both use `merge_bool_or`, ~10 min) — fixes all boolean merge bugs. Then US2 + US3 (both use array union, ~10 min) — fixes all array merge bugs.

---

## Notes

- All 31 tasks operate on a single file: `crates/core/src/config.rs`
- No new dependencies required (research.md Decision 5)
- `mount::merge_mounts()` in `mount.rs` is NOT modified (research.md Decision 2)
- Existing merge behavior for other categories is explicitly regression-tested (FR-007)
- Implementation is ~50 lines code + ~200 lines tests per plan.md estimate
