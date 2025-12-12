# Tasks: Up Lifecycle Semantics Compliance

**Input**: Design documents from `/specs/008-up-lifecycle-hooks/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Include targeted tests per story to validate lifecycle ordering, resume, skip flag, and prebuild behaviors.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm docs and entry points for lifecycle work

- [x] T001 Review spec and plan alignment in `specs/008-up-lifecycle-hooks/spec.md` and `specs/008-up-lifecycle-hooks/plan.md` to confirm scope/acceptance items.
- [x] T002 [P] Validate code touchpoints listed in `specs/008-up-lifecycle-hooks/quickstart.md` against current files under `crates/core/src/` and `crates/deacon/src/commands/`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared lifecycle primitives required before story work

- [x] T003 Align lifecycle phase state structures with data model (status/reason/marker paths) in `crates/core/src/state.rs` and `crates/core/src/lifecycle.rs`.
- [x] T004 [P] Ensure invocation context/flag parsing covers resume, prebuild, and `--skip-post-create` in `crates/deacon/src/commands/up.rs` and `crates/deacon/src/commands/shared/`.
- [x] T005 [P] Centralize phase marker read/write helpers (including isolated prebuild markers) in `crates/core/src/state.rs` for reuse across stories.

---

## Phase 3: User Story 1 - Fresh up enforces lifecycle order (Priority: P1) 🎯 MVP

**Goal**: Fresh `up` enforces onCreate → updateContent → postCreate → dotfiles → postStart → postAttach exactly once.

**Independent Test**: Run `up` on a fresh environment with all hooks/dotfiles configured; verify phases run once in order and markers recorded for each phase.

### Tests for User Story 1

- [x] T006 [P] [US1] Add/extend ordering integration test to assert SC-001: onCreate→updateContent→postCreate→dotfiles→postStart→postAttach run exactly once with no reordering/duplication in `crates/deacon/tests/smoke_lifecycle.rs` (or nearest lifecycle integration).
- [x] T007 [P] [US1] Add dotfiles ordering test to ensure dotfiles execute exactly once between postCreate and postStart (SC-001) in `crates/deacon/tests/up_dotfiles.rs`.

### Implementation for User Story 1

- [x] T008 [US1] Enforce lifecycle execution order and single-run guards in `crates/core/src/lifecycle.rs` and `crates/deacon/src/commands/up.rs`.
- [x] T009 [P] [US1] Integrate dotfiles execution at postCreate→dotfiles→postStart boundary in `crates/core/src/dotfiles.rs` and orchestrate from `crates/core/src/lifecycle.rs`.
- [x] T010 [US1] Record per-phase markers (including dotfiles) in order for fresh runs in `crates/core/src/state.rs`.
- [x] T011 [US1] Render ordered phase summary (executed/skipped) in `crates/deacon/src/ui/` consistent with spec ordering and maintain stdout/json purity vs stderr logs.

**Checkpoint**: User Story 1 independently testable via added integration tests.

---

## Phase 4: User Story 2 - Resume only reruns runtime hooks (Priority: P2)

**Goal**: Resume reruns only postStart and postAttach; incomplete prior runs resume from earliest missing phase.

**Independent Test**: After a successful initial `up`, rerun to confirm only postStart/postAttach execute; if prior run failed before postStart, rerun completes earlier phases then runtime hooks.

### Tests for User Story 2

- [x] T012 [P] [US2] Add resume integration test asserting SC-002: only postStart/postAttach rerun when markers exist in `crates/deacon/tests/smoke_lifecycle.rs`.
- [x] T013 [P] [US2] Add recovery test ensuring rerun starts from earliest incomplete marker before runtime hooks (SC-002/FR-004) in `crates/deacon/tests/up_lifecycle_recovery.rs`.

### Implementation for User Story 2

- [x] T014 [US2] Implement resume decision logic using per-phase markers to skip onCreate/updateContent/postCreate/dotfiles in `crates/core/src/lifecycle.rs` and `crates/deacon/src/commands/up.rs`.
- [x] T015 [P] [US2] Handle corrupted/missing markers by rerunning from earliest phase and updating summaries in `crates/core/src/state.rs` and `crates/deacon/src/ui/` while keeping stdout/json purity vs stderr logs.
- [x] T016 [US2] Ensure runtime hook reruns (postStart/postAttach) execute in order with markers updated in `crates/core/src/container_lifecycle.rs`.

**Checkpoint**: User Story 2 independently testable via resume/recovery tests.

---

## Phase 5: User Story 3 - Controlled skipping for flags and prebuild (Priority: P3)

**Goal**: `--skip-post-create` skips post* hooks and dotfiles; prebuild stops after updateContent, skips post* and dotfiles, and uses isolated markers while rerunning updateContent each time.

**Independent Test**: Run `up` with `--skip-post-create` to confirm base setup only; run `up` in prebuild mode repeatedly to confirm only onCreate/updateContent run, dotfiles skipped, and later normal `up` reruns onCreate/updateContent.

### Tests for User Story 3

- [x] T017 [P] [US3] Add skip-flag test verifying post* hooks and dotfiles are skipped with reasons (SC-003) in `crates/deacon/tests/up_dotfiles.rs` or adjacent file.
- [x] T018 [P] [US3] Add prebuild mode test ensuring stop after updateContent, dotfiles/post* skipped, and updateContent reruns on repeat prebuild (SC-004) in `crates/deacon/tests/up_prebuild.rs`.
- [x] T019 [P] [US3] Add transition test confirming normal `up` after prebuild reruns onCreate/updateContent despite prebuild markers (SC-004/FR-008) in `crates/deacon/tests/up_prebuild.rs`.

### Implementation for User Story 3

- [x] T020 [US3] Wire `--skip-post-create` flag handling to bypass postCreate/postStart/postAttach and dotfiles with reasons in `crates/deacon/src/commands/up.rs`.
- [x] T021 [P] [US3] Implement prebuild mode to stop after updateContent, skip dotfiles/post* hooks, and isolate markers in `crates/core/src/lifecycle.rs` and `crates/core/src/state.rs`.
- [x] T022 [US3] Ensure summary/output reflects skipped phases and prebuild isolation in `crates/deacon/src/ui/` with stdout/json purity vs stderr logs.

**Checkpoint**: User Story 3 independently testable via skip-flag and prebuild tests.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, docs, and quality gates

- [x] T023 [P] Update quickstart/spec references if behavior nuances change in `specs/008-up-lifecycle-hooks/quickstart.md` and `specs/008-up-lifecycle-hooks/spec.md`.
- [x] T024 [P] Verify OpenAPI lifecycle contract reflects final behaviors in `specs/008-up-lifecycle-hooks/contracts/up-lifecycle.yaml`.
- [x] T025 Run fmt/lints and targeted nextest suites (`make test-nextest-fast`, plus `make test-nextest-unit`/`make test-nextest-docker` as needed).
- [x] T026 Capture any Deferred Work tasks (if introduced) in `specs/008-up-lifecycle-hooks/tasks.md` under "Deferred Work" with research.md references.
- [x] T027 [P] Verify stdout/json purity and stderr logging for lifecycle summaries and JSON modes in `crates/deacon/tests/` (adjust or add integration coverage).

---

## Deferred Work

None planned; add entries here if any deferrals are introduced.

---

## Dependencies & Execution Order

- Phase 1 (Setup) → Phase 2 (Foundational) → User Stories in priority order (Phase 3: US1, Phase 4: US2, Phase 5: US3) → Phase 6 (Polish).
- User stories can proceed in parallel after Phase 2, but US1 delivers MVP and provides ordering baseline for later stories.
- Within each story: tests first, then implementation aligned to markers/resume logic, then summary/output.

---

## Parallel Opportunities

- Setup task T002 can run in parallel with T001.
- Foundational tasks T004–T005 can run in parallel after T003.
- Within stories, test additions (e.g., T006/T007) can run in parallel, and implementation tasks marked [P] touch separate files.
- Different user stories can be developed concurrently once Phase 2 completes, given non-overlapping files.

---

## Implementation Strategy

### MVP First (User Story 1 Only)
1. Complete Phase 1 and Phase 2.
2. Deliver User Story 1 (ordering, markers, dotfiles placement, summary).
3. Validate with US1 tests before expanding.

### Incremental Delivery
1. Add User Story 2 resume behavior and tests.
2. Add User Story 3 skip/prebuild behaviors and tests.
3. Polish with docs/contracts and full test cadence.

### Parallel Team Strategy
1. Finish Phase 2 shared helpers.
2. Assign US1 (ordering/dotfiles), US2 (resume), US3 (skip/prebuild) to different owners; coordinate on shared files (`lifecycle.rs`, `state.rs`, `up.rs`) to avoid conflicts.
