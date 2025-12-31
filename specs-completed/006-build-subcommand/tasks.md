---

description: "Task list for closing build subcommand parity gaps"
---

# Tasks: Build Subcommand Parity Closure

**Input**: Design documents from `/specs/006-build-subcommand/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/
**Tests**: Included where acceptance scenarios mandate validation
**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish documentation alignment before implementation begins.

- [X] T001 Update `docs/subcommand-specs/build/GAP.md` with parity targets covering tags, push/export, and compose modes. ✅
- [X] T002 Add BuildKit gating validation steps to `specs/006-build-subcommand/quickstart.md` for future execution checks. ✅

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core domain scaffolding required by all user stories.

- [X] T003 Define `BuildRequest`, `ImageArtifact`, `FeatureManifest`, and `ValidationEvent` structs in `crates/core/src/build/mod.rs` per `data-model.md`. ✅
- [X] T004 [P] Re-export the new build module from `crates/core/src/lib.rs` to make domain types available to consumers. ✅
- [X] T005 Create `BuildSuccess` and `BuildError` result structs aligned with `contracts/build-cli-contract.yaml` in `crates/deacon/src/commands/build/result.rs`. ✅
- [X] T006 Introduce shared label and image-tag validation helpers in `crates/core/src/docker.rs` for reuse across build flows. ✅
- [X] T006A Add BuildKit capability detection helpers in `crates/core/src/build/buildkit.rs` to flag feature metadata that requires BuildKit execution. ✅

---

## Phase 3: User Story 1 - Tagged Build Deliverable (Priority: P1) 🎯 MVP

**Goal**: Ensure `deacon build` applies requested tags, devcontainer metadata, and user labels while emitting spec-compliant success payloads.

**Independent Test**: Run `deacon build` against a Dockerfile workspace with multiple `--image-name` and `--label` inputs; verify tags exist locally, metadata label contains merged configuration, and stdout returns the `{ "outcome": "success", "imageName": [ ... ] }` payload.

### Tests for User Story 1

- [X] T007 [P] [US1] Add CLI flag parsing assertions for `--image-name` and `--label` in `crates/deacon/tests/integration_build_args.rs`. ✅
- [X] T008 [P] [US1] Extend JSON output purity checks for multi-tag success payloads in `crates/deacon/tests/json_output_purity.rs`. ✅

### Implementation for User Story 1

- [X] T009 [US1] Extend the build subcommand definition with repeatable `--image-name` and `--label` options in `crates/deacon/src/cli.rs`. ✅
- [X] T009A [US2] Surface `--push` and `--output` switches in `crates/deacon/src/cli.rs`, updating CLI help text and ensuring mutual exclusivity is documented. ✅
- [X] T010 [US1] Map image names and labels into `BuildArgs` and the new `BuildRequest` translation in `crates/deacon/src/commands/build.rs`. ✅
- [X] T011 [US1] Enforce tag/label validation and inject devcontainer metadata labels during Dockerfile builds in `crates/deacon/src/commands/build.rs`. ✅
- [X] T012 [US1] Serialize merged devcontainer metadata and user labels into the build artifact record within `crates/core/src/build/metadata.rs`. ✅

**Checkpoint**: Tagged Dockerfile builds emit spec-compliant JSON with accurate labels and tags.

---

## Phase 4: User Story 2 - Registry and Artifact Distribution (Priority: P2)

**Goal**: Support pushing images to registries or exporting archives with explicit BuildKit gating and structured error handling.

**Independent Test**: Execute builds with `--push` and `--output` on both BuildKit-enabled and disabled hosts; confirm artifacts are published or exports created, and gating errors match spec text when prerequisites fail.

### Tests for User Story 2

- [X] T013 [P] [US2] Cover BuildKit gating and mutually exclusive flag errors in `crates/deacon/tests/integration_build.rs`. ✅
- [X] T013A [P] [US2] Assert CLI argument parsing and help output for `--push` and `--output` in `crates/deacon/tests/integration_build_args.rs`. ✅
- [X] T014 [P] [US2] Verify pushed tags and exported artifact reporting in `crates/deacon/tests/parity_build.rs`. ✅
- [X] T014A [P] [US2] Add regression coverage for BuildKit-only feature contexts in `crates/deacon/tests/parity_build.rs`. ✅

### Implementation for User Story 2

- [X] T015 [US2] Enforce `--push`/`--output` exclusivity and BuildKit requirement checks within `crates/deacon/src/commands/build.rs`. ✅
- [X] T016 [US2] Extend Docker build execution to support push/export flows and capture statuses in `crates/core/src/docker.rs`. ✅
- [X] T016A [US2] Integrate BuildKit-only detection from `crates/core/src/build/buildkit.rs` into the execution path, returning the documented fail-fast error when unavailable. ✅
- [X] T017 [US2] Populate `pushed` and `exportPath` fields in the JSON success payload emitted from `crates/deacon/src/commands/build/result.rs`. ✅
- [X] T018 [US2] Map validation failures to spec-defined `BuildError` responses in `crates/deacon/src/commands/build.rs`. ✅

**Checkpoint**: Push/export workflows succeed or fail fast with contract-compliant output and errors.

---

## Phase 5: User Story 3 - Multi-source Configuration Coverage (Priority: P3)

**Goal**: Enable builds for Compose and image-reference configurations with parity validation and feature application.

**Independent Test**: Build a Compose workspace targeting the configured service and an image-reference workspace; ensure features, labels, and tagging match Dockerfile mode while unsupported flags are rejected.

### Tests for User Story 3

- [X] T019 [P] [US3] Add Compose build acceptance covering targeted service selection to `crates/deacon/tests/smoke_compose_edges.rs`. ✅
- [X] T020 [P] [US3] Add image-reference build acceptance case to `crates/deacon/tests/parity_build.rs`. ✅

### Implementation for User Story 3

- [X] T021 [US3] Resolve Compose service targeting and unsupported flag preflight in `crates/core/src/compose.rs`. ✅
- [X] T022 [US3] Integrate Compose execution path and validation into `execute_build` within `crates/deacon/src/commands/build.rs`. ✅
- [X] T023 [US3] Enable image-reference builds with feature application and tagging in `crates/deacon/src/commands/build.rs`. ✅
- [X] T024 [US3] Add Compose and image reference fixtures under `examples/build/compose-service-target/` and `fixtures/config/build/compose-service-target/` for parity tests. ✅

**Checkpoint**: Compose and image-reference builds behave consistently with Dockerfile mode and honor validation rules.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, examples, and release hygiene after user stories complete.

- [X] T025 [P] Update `docs/subcommand-specs/build/SPEC.md` with the completed flag set, validation messages, and success payload schemas. ✅
- [X] T026 [P] Document new build scenarios in `examples/README.md`, linking the Compose and image-reference fixtures. ✅
- [X] T027 Capture parity closure summary and release notes in `docs/CLI_PARITY.md`. ✅

---

## Dependencies & Execution Order

### Phase Dependencies

- Phase 1 (Setup) must complete before any foundational or story work to keep documentation aligned.
- Phase 2 (Foundational) depends on Phase 1, and all user stories rely on the shared domain scaffolding delivered here.
- Phase 3 (US1) begins after foundational work and serves as the MVP; Phases 4 and 5 depend on its completion to reuse tagging/metadata primitives.
- Phase 6 (Polish) runs after all targeted user stories are complete.

### User Story Dependencies

- **US1 (P1)** depends only on Phase 2 and unlocks the JSON contract and tagging flows used by later stories.
- **US2 (P2)** depends on US1 for BuildSuccess payloads and tag handling.
- **US3 (P3)** depends on US1 for tagging/metadata and partially on US2 for validation helpers introduced in the build command.

### Task Dependency Highlights

- T003 → T004/T010/T011/T015/T022: Core BuildRequest types must exist before downstream usage.
- T005 → T008/T017/T018: Contract structs must be defined before tests and payload wiring.
- T006/T006A → T011/T015/T016A/T023: Shared validators and BuildKit detection must precede enforcement paths.
- T009A → T013A/T015: CLI surface updates are prerequisites for argument parsing coverage and validation wiring.
- Fixture work (T024) depends on successful implementation of Compose/image build support (T021–T023).

---

## Parallel Opportunities

- **During Phase 2**: T004 and T006 can proceed in parallel after T003 establishes the module structure.
- **US1**: T007 and T008 can run concurrently with T009 once CLI option shapes are defined; T011/T012 can parallelize after T010 stabilizes the BuildRequest mapping.
- **US2**: T013, T013A, and T014 can execute alongside T016/T016A after T015 finalizes validation logic.
- **US3**: T019 and T020 can run in parallel; T021 can begin while US2 polish wraps, enabling T022 and T023 to work concurrently once Compose scaffolding lands.
- **Polish**: T025–T027 can proceed in any order after story completion because they touch disjoint documentation files.

---

## Parallel Examples

### Parallel Example: User Story 1

```bash
# Tests
cargo test --quiet -p deacon integration_build_args:: -- --ignored
cargo test --quiet -p deacon json_output_purity::build_json_success
```

### Parallel Example: User Story 2

```bash
# Tests
cargo test --quiet -p deacon integration_build::buildkit_gating
cargo test --quiet -p deacon parity_build::push_export_contract
```

### Parallel Example: User Story 3

```bash
# Tests
cargo test --quiet -p deacon smoke_compose_edges::build_target_service
cargo test --quiet -p deacon parity_build::image_reference_build
```

---

## Implementation Strategy

1. Complete Setup (Phase 1) and Foundational scaffolding (Phase 2) to unlock shared types and validation helpers.
2. Deliver MVP by finishing User Story 1 (Phase 3), ensuring tagging, labels, and JSON payloads meet the spec.
3. Iterate on User Story 2 (Phase 4) to enable push/export workflows with BuildKit gating; validate via contract tests.
4. Finalize parity with User Story 3 (Phase 5), covering Compose and image-reference builds plus supporting fixtures.
5. Close out documentation and release artifacts in Phase 6 to signal completion and update parity tracking.
