# Tasks: GPU Mode Handling for Up

**Input**: Design documents from `/specs/001-gpu-modes/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests included where needed for acceptance and spec compliance.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Task Completion Criteria (CRITICAL)

A task MUST NOT be marked `[X]` unless ALL of these are true:

1. ✅ **Implementation Complete**: All code is written and functional (no stubs, no blocking TODOs)
2. ✅ **Tests Pass**: All related tests pass without `#[ignore]` or skip markers
3. ✅ **No Blocking TODOs**: No `TODO T###` comments indicating missing functionality in the task scope
4. ✅ **Verified**: Task has been tested against its acceptance criteria

**Valid States**:
- `[ ]` = Not started or significant work remaining
- `[X]` = Fully complete per above criteria
- Use comments for partial progress: `T020 ... (data structures exist, logic pending)`

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm scope and existing surfaces before wiring GPU modes.

- [X] T001 Review GPU mode spec/plan/research to confirm scope and acceptance (specs/001-gpu-modes/spec.md, specs/001-gpu-modes/plan.md, specs/001-gpu-modes/research.md).
- [X] T002 Inspect existing CLI argument surfaces for GPU placeholders and value enums (crates/deacon/src/cli.rs).
- [X] T003 Map runtime and compose invocation points for GPU flag injection in up flow (crates/deacon/src/commands/up.rs, crates/core/src/docker.rs, crates/core/src/compose.rs).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core structures and plumbing required by all user stories.

- [X] T004 Add GPUMode enum and HostGpuCapability structs per data-model (crates/core/src/gpu.rs).
- [X] T005 Wire GPU mode CLI parsing with default=none into Up args and normalization (crates/deacon/src/cli.rs, crates/deacon/src/commands/up.rs).
- [X] T006 Implement GPU detection helper using Docker runtime introspection with warning context plumbing (crates/core/src/docker.rs).
- [X] T007 Thread GPU mode through runtime/compose helper interfaces to enable downstream application (crates/core/src/runtime.rs, crates/core/src/compose.rs).

**Checkpoint**: Foundation ready - user story implementation can now begin.

---

## Phase 3: User Story 1 - Guarantee GPU access (Priority: P1) 🎯 MVP

**Goal**: Mode `all` requests GPU resources for all run/build/compose operations started by `up`.

**Independent Test**: Run `up --gpu-mode all` on a GPU-capable host and verify docker run/build/compose invocations include GPU flags without extra user input.

### Implementation for User Story 1

- [X] T008 [US1] Apply GPU mode `all` to docker run/build/compose invocations via shared helpers (crates/core/src/docker.rs, crates/core/src/compose.rs).
- [X] T009 [US1] Map CLI mode `all` through up flow and emit confirmation in user-facing output/logs (crates/deacon/src/commands/up.rs).
- [X] T010 [P] [US1] Add coverage asserting `--gpus all` propagation for run/build and compose paths when mode=`all` (crates/deacon/tests/up_gpu_all.rs).

**Checkpoint**: User Story 1 fully functional and testable independently.

---

## Phase 4: User Story 2 - Auto-detect with safe fallback (Priority: P2)

**Goal**: Mode `detect` probes GPUs, requests them when available, and warns once before continuing without GPUs when unavailable.

**Independent Test**: Run `up --gpu-mode detect` on GPU-less host to see a single warning and no GPU flags; repeat on GPU host to see GPU flags applied.

### Implementation for User Story 2

- [X] T011 [US2] Integrate detect-mode probe results into up flow; request GPUs when available and issue a single warning when absent (crates/deacon/src/commands/up.rs).
- [X] T012 [P] [US2] Add tests for detect mode covering GPU-present (flags added) and GPU-absent (warning, no flags) cases (crates/deacon/tests/up_gpu_detect.rs).
- [X] T013 [US2] Ensure build and compose flows reuse detection results without duplicate probes or log spam (crates/core/src/docker.rs, crates/core/src/compose.rs).

**Checkpoint**: User Story 2 functional with warning behavior validated.

---

## Phase 5: User Story 3 - Explicit CPU-only runs (Priority: P3)

**Goal**: Mode `none` bypasses GPU requests and emits no GPU-related warnings.

**Independent Test**: Run `up --gpu-mode none` on any host and confirm no GPU flags or GPU-related output.

### Implementation for User Story 3

- [X] T014 [US3] Enforce GPU mode `none` to bypass GPU flag emission and suppress GPU warnings across run/build/compose (crates/deacon/src/commands/up.rs).
- [X] T015 [P] [US3] Add regression test verifying no GPU flags or warnings when mode=`none` (crates/deacon/tests/up_gpu_none.rs).

**Checkpoint**: User Story 3 functional and testable independently.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Cross-story documentation, examples, and quality gates.

- [X] T016 [P] Update CLI help/quickstart/docs to reflect GPU modes, defaults, and warning behaviors (crates/deacon/src/cli.rs, specs/001-gpu-modes/quickstart.md).
- [X] T017 [P] Create/update examples in examples/up/ to demonstrate GPU modes and keep README/exec.sh in sync (examples/up/).
- [X] T018 Run formatting, clippy, and targeted nextest suite for GPU mode changes (cargo fmt, cargo clippy, make test-nextest-fast; add docker/compose-focused nextest suite if runtime/compose wiring changed).
- [X] T019 [P] Validate warning wording/channel and output contracts for all/detect/none (stderr for warnings; stdout JSON intact) (crates/deacon/tests/up_gpu_output.rs).
- [X] T020 [P] Test edge cases: missing GPU drivers/permissions, runtime failure surfaces, mixed service GPU needs, explicit mode override per invocation (crates/deacon/tests/up_gpu_edge_cases.rs).
- [X] T021 [P] Validate consistency across run/build/compose over repeated runs for selected mode (crates/deacon/tests/up_gpu_consistency.rs).
- [X] T022 [P] Compare GPU mode behavior against docs/repomix-output-devcontainers-cli.xml reference expectations and align flags/warnings (docs/repomix-output-devcontainers-cli.xml).

---

## Dependencies & Execution Order

- Phase 1 → Phase 2 → User Stories (Phase 3 P1, Phase 4 P2, Phase 5 P3) → Phase 6.
- User stories can proceed in priority order after Phase 2; US2/US3 depend on the foundational GPU mode plumbing.
- Testing tasks within a story can run in parallel with code tasks when files do not overlap.

## Parallel Execution Examples

- T004 and T005 can run in parallel after T001–T003 (distinct files).
- T010 can run in parallel with T008/T009 once interfaces are defined.
- T012 can run in parallel with T011 once probe helper is in place.
- T015 can run in parallel with T014 after GPU mode wiring exists.
- T016–T018 can run in parallel after story phases complete (docs/examples/tests in separate paths).

## Implementation Strategy

- MVP = User Story 1 (mode `all` propagation) after foundational tasks.
- Iterate by adding detect-mode behavior (US2), then finalize with CPU-only assurance (US3), closing with docs/examples/polish.
