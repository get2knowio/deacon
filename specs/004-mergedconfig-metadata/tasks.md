# Tasks: Enriched mergedConfiguration metadata for up

**Input**: Design documents from `/specs/001-mergedconfig-metadata/`  
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Include targeted tests where they protect acceptance (ordering, null semantics, provenance).  
**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Ensure design references and tooling are ready.

- [x] T001 Review feature spec and plan (specs/001-mergedconfig-metadata/spec.md, plan.md) to confirm scope and success criteria.
- [x] T002 Collect schema references for mergedConfiguration and metadata/labels (docs/repomix-output-devcontainers-cli.xml, docs/subcommand-specs/up/DATA-STRUCTURES.md).
- [x] T003 [P] Verify dev tooling baseline: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `make test-nextest-fast` dry run from repo root.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core alignment work before story implementation.

- [x] T004 Map existing merge helpers used by read_configuration (crates/deacon/src/commands/read_configuration.rs) that generate feature metadata and labels.
- [x] T005 [P] Identify data structures/serialization paths carrying mergedConfiguration in up (crates/deacon/src/commands/up.rs) and confirm ordering/null semantics requirements.
- [x] T006 Document current fixtures/tests touching mergedConfiguration to avoid regressions (search crates/deacon/tests and fixtures/ for mergedConfiguration usage).

**Checkpoint**: Foundation ready - user story work can now begin.

---

## Phase 3: User Story 1 - Verify feature metadata presence (Priority: P1) MVP

**Goal**: mergedConfiguration lists feature metadata with provenance and ordering across single/compose flows.

**Independent Test**: Run `up` with multiple features and assert mergedConfiguration includes ordered featureMetadata entries with required keys and null handling.

### Implementation for User Story 1

- [x] T007 [US1] Reuse read_configuration feature metadata merge path in up single-flow output shaping (crates/deacon/src/commands/up.rs).
- [x] T008 [US1] Apply the same feature metadata merge path for compose flows, preserving service-aware provenance and order (crates/deacon/src/commands/up.rs).
- [x] T009 [P] [US1] Ensure serialization retains required fields with null/empty values instead of omission (crates/deacon/src/commands/up.rs).
- [x] T010 [US1] Add/adjust tests validating feature metadata presence/order/null semantics for single flow (crates/deacon/tests/, fixtures/).
- [x] T011 [US1] Add/adjust tests validating feature metadata presence/order/null semantics for compose flow (crates/deacon/tests/, fixtures/compose).
- [x] T012 [US1] Add test for devcontainer with no features ensuring mergedConfiguration keeps metadata fields with null/empty placeholders (crates/deacon/tests/, fixtures/).

**Checkpoint**: User Story 1 independently verifiable via mergedConfiguration feature metadata assertions.

---

## Phase 4: User Story 2 - Capture image and container labels (Priority: P2)

**Goal**: mergedConfiguration surfaces image/container labels with provenance for single and compose, including null/empty handling.

**Independent Test**: Run `up` on single and compose configs with known labels; mergedConfiguration includes those labels with source annotations, and retains label fields when absent.

### Implementation for User Story 2

- [x] T013 [US2] Reuse label merge logic from read_configuration for single-flow mergedConfiguration (crates/deacon/src/commands/up.rs).
- [x] T014 [US2] Extend compose path to include per-service label provenance and ordering in mergedConfiguration (crates/deacon/src/commands/up.rs).
- [x] T015 [P] [US2] Ensure label sections remain present with null/empty values when labels are missing (crates/deacon/src/commands/up.rs).
- [x] T016 [US2] Add/adjust tests covering image/container labels for single flow, including null/empty cases (crates/deacon/tests/, fixtures/).
- [x] T017 [US2] Add/adjust tests covering compose services label capture and provenance/order (crates/deacon/tests/, fixtures/compose).
- [x] T018 [US2] Add fixture/test for conflicting or duplicate labels asserting spec-defined deterministic prioritization/ordering (crates/deacon/tests/, fixtures/compose).

**Checkpoint**: User Story 2 independently verifiable via label capture scenarios.

---

## Phase 5: User Story 3 - Compare base vs merged configuration (Priority: P3)

**Goal**: mergedConfiguration differs from base by enriched metadata/labels while remaining schema-compliant across flows.

**Independent Test**: Generate base and merged outputs for single/compose; confirm differences reflect enrichment only and pass schema validation.

### Implementation for User Story 3

- [x] T019 [US3] Ensure mergedConfiguration retains schema/ordering while diverging from base when enrichment applies (crates/deacon/src/commands/up.rs).
- [x] T020 [P] [US3] Add test asserting base vs merged diff shows added metadata/labels without unrelated drift (crates/deacon/tests/, fixtures/).
- [x] T021 [US3] Add mandatory schema/contract validation for mergedConfiguration output covering single and compose (crates/deacon/tests/, fixtures/).

**Checkpoint**: User Story 3 independently verifiable via diff and schema checks.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T022 [P] Update quickstart and docs references if output shape changed (specs/001-mergedconfig-metadata/quickstart.md, docs/subcommand-specs/up/SPEC.md notes if required).
- [x] T023 [P] Run final formatting and lint gate (`cargo fmt --all && cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`).
- [x] T024 Execute targeted test suite per change scope (`make test-nextest-unit` for merge logic; `make test-nextest-fast`; add `make test-nextest-docker` if compose fixtures require Docker).
- [x] T025 [P] Update fixtures/golden outputs if test expectations shift (fixtures/, crates/deacon/tests/).
- [x] T026 Capture learnings/decisions in research.md if implementation deviates from plan (specs/001-mergedconfig-metadata/research.md).
- [x] T027 Benchmark mergedConfiguration merge overhead vs base merge and document results (artifacts/ or docs/notes).

---

## Dependencies & Execution Order

- Phase dependencies: Setup -> Foundational -> US1 (MVP) -> US2 -> US3 -> Polish.
- User stories: US1 (feature metadata) is MVP; US2 (labels) and US3 (base-vs-merged diff/schema) can proceed after foundational, but US3 comparisons depend on metadata/labels being present.
- Task ordering within stories follows data flow: reuse merge helpers -> compose path alignment -> serialization/null handling -> tests.

## Parallel Opportunities

- Setup T002/T003 can run in parallel after T001.  
- Foundational T004/T005/T006 mostly independent; T005 benefits from T004 outputs.  
- Within US1: T007/T008 sequentially depend on understanding; T009 can follow T007; tests T010/T011 can proceed after code paths stubbed.  
- Within US2: T012/T013 sequential; T014 can follow; T015/T016 in parallel once code paths exist.  
- US3 tasks can start after US1/US2 enrichment is available.

## Implementation Strategy

- MVP = Complete US1 (feature metadata) with tests; validate mergedConfiguration shows feature metadata in order with null handling.  
- Incrementally add US2 (labels) and US3 (base vs merged diffs/schema) with their tests.  
- Keep build green: run fmt/clippy and targeted nextest commands per scope; avoid regressions in existing fixtures.***
