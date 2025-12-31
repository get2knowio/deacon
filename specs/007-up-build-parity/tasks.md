# Tasks: Up Build Parity and Metadata

**Input**: Design documents from `/specs/007-up-build-parity/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Include targeted test tasks per user story to validate acceptance scenarios.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm scope and locate insertion points

- [x] T001 Review scope and constraints for up build parity in specs/007-up-build-parity/plan.md and specs/007-up-build-parity/spec.md
- [x] T002 Map current up build and feature build flow entry points in crates/deacon/src/commands/up.rs for later wiring

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Baseline shared helpers and validation points

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T003 Survey BuildKit/buildx detection helpers and gaps in crates/core/src/build/buildkit.rs to plan fail-fast checks
- [x] T004 Catalog feature resolution and lockfile handling behaviors in crates/core/src/features.rs and crates/core/src/lockfile.rs to understand enforcement hooks
- [x] T005 Review merged configuration enrichment code and tests in crates/deacon/src/commands/up.rs and crates/deacon/tests/up_merged_configuration.rs to identify metadata injection points

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Build options respected (Priority: P1) 🎯 MVP

**Goal**: BuildKit/cache-from/cache-to/buildx options apply to both Dockerfile and feature builds

**Independent Test**: Run `deacon up` with cache-from/cache-to/buildx options and verify both build paths receive them; defaults unchanged when options absent

### Implementation for User Story 1

- [x] T006 [P] [US1] Wire cache-from/cache-to/buildx/builder flag parsing through up CLI in crates/deacon/src/commands/up.rs (reuse shared flag helpers from build where available)
- [x] T007 [US1] Thread BuildOptions through up Dockerfile build invocation to docker/buildx adapter in crates/deacon/src/commands/up.rs and crates/core/src/docker.rs
- [x] T008 [US1] Apply BuildOptions to feature build pipeline so feature builds honor cache-from/cache-to/buildx settings in crates/core/src/feature_installer.rs
- [x] T009 [US1] Enforce BuildKit/buildx availability and emit fail-fast errors when requested options are unsupported in crates/core/src/build/buildkit.rs
- [x] T010 [P] [US1] Add integration coverage proving cache-from/cache-to/buildx propagate to Dockerfile and feature builds in crates/deacon/tests/integration_up_build_options.rs
- [x] T021 [US1] Audit default build invocation to ensure no cache/buildx args are injected when options are absent in crates/deacon/src/commands/up.rs and crates/core/src/docker.rs
- [x] T022 [P] [US1] Add integration test verifying default build behavior with no cache/buildx options remains unchanged in crates/deacon/tests/integration_up_build_options.rs
- [x] T023 [US1] Implement warning-and-continue handling when cache-from/cache-to endpoints are unreachable in crates/core/src/docker.rs and crates/core/src/feature_installer.rs
- [x] T024 [P] [US1] Add integration test for unreachable cache endpoints emitting warnings while builds proceed for Dockerfile and feature builds in crates/deacon/tests/integration_up_build_options.rs

**Checkpoint**: User Story 1 fully functional and testable independently

---

## Phase 4: User Story 2 - Deterministic feature selection (Priority: P2)

**Goal**: Skip auto-mapped features and enforce lockfile/frozen before builds start

**Independent Test**: Run `deacon up` with skip-feature-auto-mapping and lockfile/frozen; only declared features resolve, and mismatches halt before builds with clear errors

### Implementation for User Story 2

- [x] T011 [US2] Implement skip-feature-auto-mapping flag handling to block implicit feature additions in crates/deacon/src/commands/up.rs and crates/core/src/features.rs
- [x] T012 [US2] Enforce lockfile/frozen validation pre-build and halt on mismatch/missing using crates/core/src/lockfile.rs with entrypoint checks in crates/deacon/src/commands/up.rs
- [x] T013 [P] [US2] Refine user-facing errors and exit codes for skip-auto-mapping and lockfile/frozen enforcement in crates/deacon/src/commands/up.rs
- [x] T014 [P] [US2] Add integration tests for skip-feature-auto-mapping and lockfile/frozen fail-fast behavior in crates/deacon/tests/up_lockfile_frozen.rs

**Checkpoint**: User Stories 1 and 2 functional and testable independently

---

## Phase 5: User Story 3 - Metadata available downstream (Priority: P3)

**Goal**: mergedConfiguration includes metadata for every built feature, even when empty

**Independent Test**: After `deacon up`, mergedConfiguration JSON lists all features with metadata entries (empty when none emitted) preserving declaration order

### Implementation for User Story 3

- [x] T015 [US3] Ensure merged configuration always includes feature metadata entries (empty when none) in crates/core/src/config.rs and crates/deacon/src/commands/up.rs
- [x] T016 [P] [US3] Preserve feature order and origin when serializing mergedConfiguration metadata in crates/deacon/tests/up_merged_configuration.rs and related merge helpers
- [x] T017 [P] [US3] Add integration/regression test confirming mergedConfiguration JSON contains metadata for all features in crates/deacon/tests/integration_up_with_features.rs

**Checkpoint**: All user stories independently functional

---

## Phase N: Polish & Cross-Cutting Concerns

**Purpose**: Finalize documentation, configuration, and quality gates

- [x] T018 [P] Refresh contracts and quickstart to reflect new up behaviors in specs/007-up-build-parity/contracts/up.md and specs/007-up-build-parity/quickstart.md
- [x] T019 [P] Update nextest grouping if new integration test binaries are added in .config/nextest.toml
- [x] T020 Run fmt, clippy, and targeted nextest suites from workspace root per specs/007-up-build-parity/quickstart.md commands
- [x] T025 Run acceptance sweep to validate ≥95% pass rate across defined scenarios and record results (logs/artifacts) from workspace root
- [x] T026 [P] Update specs/007-up-build-parity/quickstart.md with acceptance sweep command and expected outcome

---

## Deferred Work

No deferrals planned; add here if research documents future phases.

---

## Dependencies & Execution Order

### Phase Dependencies
- Setup (Phase 1) → Foundational (Phase 2) → User Stories (Phases 3-5) → Polish
- User stories follow priority order (US1 → US2 → US3); US2/US3 can start after foundational if US1 interfaces are stable, but final validation depends on US1 build option wiring.

### User Story Dependency Graph
- US1: Build options parity (unblocks feature builds and metadata propagation)
- US2: Depends on foundational; can run after US1 core build threading is stable to avoid rework
- US3: Depends on US1 (metadata tied to resolved features/build outputs) and foundational merge review; can proceed once merged config path is stable

### Parallel Execution Examples
- While T006-T009 are in progress, begin test scaffolding T010 in parallel once flag shapes are known.
- After T011-T012 land, T014 can run in parallel with T013 since it validates behavior rather than alters it.
- T016 and T017 can proceed in parallel once metadata shape from T015 is defined.

## Implementation Strategy

Adopt MVP-first delivery: complete Phase 3 (US1) to ensure build option parity before enabling deterministic feature selection (US2) and metadata completeness (US3). Each story delivers independently testable increments; polish consolidates docs, nextest config, and quality gates.
