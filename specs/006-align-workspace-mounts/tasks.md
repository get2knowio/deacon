---

description: "Task list template for feature implementation"
---

# Tasks: Workspace Mount Consistency and Git-Root Handling

**Input**: Design documents from `/specs/006-align-workspace-mounts/`  
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Include targeted tests because acceptance requires visible consistency and git-root parity across Docker/Compose.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm workspace state and tooling before feature work

- [ ] T001 Ensure rust toolchain components installed (`rustfmt`, `clippy`) per rust-toolchain.toml
- [ ] T002 Verify nextest targets runnable (`make test-nextest-fast`) to confirm baseline passes

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish shared helpers and test scaffolding used by all stories

- [ ] T003 Map current workspace discovery and mount rendering entry points in `crates/core` and `crates/deacon` (note functions/modules)
- [ ] T004 [P] Identify existing tests covering workspace mount generation in `crates/deacon/tests` and note gaps for consistency/git-root parity
- [ ] T005 [P] Prepare fixtures/helpers for mount rendering tests if needed in `crates/deacon/tests/` (no behavior changes yet)

**Checkpoint**: Foundation ready—user story implementation can begin

---

## Phase 3: User Story 1 - Workspace mount consistency is honored (Priority: P1) 🎯 MVP

**Goal**: Every generated workspace mount reflects the user-specified consistency across Docker and Compose outputs.

**Independent Test**: Invoke CLI with a consistency value and verify Docker and Compose mounts both surface that value in their definitions.

### Tests for User Story 1

- [ ] T006 [P] [US1] Add/extend unit test for consistency propagation in Docker mount rendering (`crates/deacon/tests/workspace_mounts.rs`)
- [ ] T007 [P] [US1] Add/extend unit test for consistency propagation in Compose mount rendering (`crates/deacon/tests/workspace_mounts.rs`)

### Implementation for User Story 1

- [ ] T008 [US1] Apply consistency value during workspace mount construction in Docker path (`crates/deacon/src/commands/up.rs`)
- [ ] T009 [P] [US1] Apply consistency value during workspace mount construction in Compose path (`crates/core/src/compose.rs`)
- [ ] T010 [US1] Ensure consistency value is visible in rendered mount definitions/output for Docker and Compose (`crates/core/src/docker.rs` and `crates/core/src/compose.rs`)
- [ ] T011 [P] [US1] Add unit test confirming default workspace discovery unchanged when no consistency override is provided (`crates/deacon/tests/workspace_mounts.rs`)
- [ ] T012 [US1] Preserve default workspace discovery behavior when consistency flag is absent (`crates/deacon/src/commands/up.rs`)

**Checkpoint**: User Story 1 independently testable (consistency visible across Docker and Compose)

---

## Phase 4: User Story 2 - Git-root mounting for Docker flows (Priority: P2)

**Goal**: Docker runs with git-root flag mount the repository root (not just cwd) while honoring consistency.

**Independent Test**: From a subdirectory repo, enable git-root flag and verify Docker mount host path equals repo root.

### Tests for User Story 2

- [ ] T013 [P] [US2] Add unit/integration test for Docker mount host path selection when git-root flag is set (`crates/deacon/tests/workspace_mounts.rs`)

### Implementation for User Story 2

- [ ] T014 [US2] Align git-root discovery for Docker flow to use repository top-level (if present) in mount construction (`crates/core/src/docker.rs`)
- [ ] T015 [P] [US2] Preserve consistency value when using git-root host path in Docker mount rendering (`crates/core/src/docker.rs`)
- [ ] T016 [US2] Add fallback note/logging when git root absent while continuing with workspace root for Docker mounts (`crates/deacon/src/commands/up.rs`)

**Checkpoint**: User Story 2 independently testable (Docker git-root mount path correct with consistency)

---

## Phase 5: User Story 3 - Git-root mounting for Compose flows (Priority: P3)

**Goal**: Compose services with git-root flag use repository root for all workspace mounts, matching Docker behavior.

**Independent Test**: From subdirectory repo, enable git-root flag and verify every Compose service workspace mount uses repo root and chosen consistency.

### Tests for User Story 3

- [ ] T017 [P] [US3] Add unit/integration test for Compose mount host path selection with git-root flag across services (`crates/deacon/tests/workspace_mounts.rs`)

### Implementation for User Story 3

- [ ] T018 [US3] Apply git-root host path to all Compose service workspace mounts when flag set (`crates/core/src/compose.rs`)
- [ ] T019 [P] [US3] Ensure consistency value remains applied in Compose mounts when using git-root host path (`crates/core/src/compose.rs`)
- [ ] T020 [US3] Surface fallback note/logging for Compose when git root missing while continuing with workspace root (`crates/deacon/src/commands/up.rs`)

**Checkpoint**: User Story 3 independently testable (Compose git-root mount path correct with consistency)

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final alignment, docs, and verification across stories

- [ ] T021 [P] Update quickstart/tests documentation in `specs/006-align-workspace-mounts/quickstart.md` if test commands or coverage changed
- [ ] T022 [P] Verify `.config/nextest.toml` contains any new integration binaries/groups (docker-shared vs docker-exclusive) if added
- [ ] T023 Run full validation cadence: `cargo fmt --all && cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make test-nextest-fast`, `make test-nextest` before PR
- [ ] T024 [P] Final code/doc cleanup for mount handling (comments/log wording) in touched files
- [ ] T025 [P] Add lightweight timing check (<200ms) for workspace discovery and mount rendering path (`crates/deacon/tests/workspace_mounts.rs` or micro-benchmark harness)

---

## Dependencies & Execution Order

- Setup (Phase 1) → Foundational (Phase 2) → US1 (Phase 3) → US2 (Phase 4) → US3 (Phase 5) → Polish (Phase 6)
- User stories are independently testable; US1 is the MVP and can ship alone.

## Parallel Opportunities

- Marked `[P]` tasks can run concurrently when file paths do not conflict.
- After Phase 2, US1/US2/US3 tasks can proceed in parallel by different owners; US2/US3 depend logically on US1 patterns but not code-completion gating.

## Implementation Strategy

- MVP first: Complete US1 (consistency propagation) before broader git-root changes.
- Incremental delivery: Land US1 → US2 → US3, each validated independently.
- Testing cadence: unit-focused for path/consistency selection; integration only if render paths diverge; update `.config/nextest.toml` for any new integration binaries.
