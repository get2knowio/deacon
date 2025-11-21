# Tasks: Devcontainer Up Gap Closure

**Input**: Design documents from `/specs/001-up-gap-spec/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Included (per spec FR-013 for comprehensive coverage).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare shared fixtures and guardrails for up parity work.

- [X] T001 Create shared up parity fixtures directory and placeholders in `fixtures/devcontainer-up/` (single-container devcontainer.json, compose with profiles/.env, feature+dotfiles fixture)
- [X] T002 Add quick references to fast-loop commands in `specs/001-up-gap-spec/quickstart.md` (ensure commands reflect current repo scripts)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core scaffolding that all user stories rely on.

- [X] T003 Define UpResult success/error structs and serializer helpers in `crates/deacon/src/commands/up.rs` and `crates/deacon/src/commands/mod.rs`
- [X] T004 Add parsed input/normalization struct skeleton and shared validation utilities in `crates/deacon/src/commands/up.rs` (mount/remote-env regex, terminal dims pairing)

**Checkpoint**: Foundation ready - user story implementation can now begin.

---

## Phase 3: User Story 1 - Reliable up invocation with full flag coverage (Priority: P1) 🎯 MVP

**Goal**: Provide complete flag coverage with strict validation and stdout JSON output contract for single-container flows.

**Independent Test**: Run `deacon up` with workspace, id-labels, mount/env/cache flags, include-config options; verify stdout JSON success, stderr logs only, and validation errors stop before runtime.

### Tests for User Story 1

- [X] T005 [P] [US1] Add unit tests for flag parsing/validation and JSON output serialization in `crates/deacon/src/cli.rs` and `crates/deacon/tests/up_validation.rs`
- [X] T006 [P] [US1] Add integration tests for invalid mount/remote-env and success JSON emission in `crates/deacon/tests/up_json_output.rs`
- [X] T027 [P] [US1] Add unit/integration tests for config filename validation, disallowed feature error, and image metadata merge in `crates/deacon/tests/up_config_resolution.rs`

### Implementation for User Story 1

- [X] T007 [US1] Implement missing CLI flags and help text (workspace/id-label, runtime behavior, mounts/env/cache/buildkit, metadata omission, output shaping, data folders, docker/compose paths) in `crates/deacon/src/cli.rs`
- [X] T008 [US1] Enforce validation and normalization rules (workspace/id-label/override-config requirements, mount/remote-env regex, terminal dims pairing, expect-existing fast-fail) in `crates/deacon/src/commands/up.rs`
- [X] T009 [US1] Wire normalized options into provision/runtime structures including runtime path overrides and build/cache options in `crates/deacon/src/commands/up.rs` and `crates/core/src/container.rs`
- [X] T010 [US1] Implement stdout JSON success/error contract with include-configuration/mergedConfiguration flags and stderr-only logging in `crates/deacon/src/commands/up.rs`
- [X] T011 [US1] Standardize error mapping/messages and exit codes for validation failures in `crates/deacon/src/commands/up.rs`
- [X] T028 [US1] Enforce devcontainer filename validation and override-only discovery rules in `crates/deacon/src/commands/read_configuration.rs`
- [X] T029 [US1] Implement id-label discovery, disallowed feature error, and image metadata merge into resolved configuration in `crates/deacon/src/commands/up.rs`

**Checkpoint**: User Story 1 independently testable (flags, validation, JSON output).

---

## Phase 4: User Story 2 - CI prebuild and lifecycle orchestration (Priority: P2)

**Goal**: Deliver prebuild/updateContent/dotfiles/feature-driven image flows with UID/security handling and lifecycle correctness.

**Independent Test**: Run `deacon up --prebuild` on fixtures with features and dotfiles; verify lifecycle stops after updateContent (first run), reruns updateContent on subsequent runs, dotfiles idempotent, security/UID options honored.

### Tests for User Story 2

- [X] T012 [P] [US2] Add integration test for prebuild lifecycle (stop after updateContent, rerun on repeat) in `crates/deacon/tests/up_prebuild.rs`
- [X] T013 [P] [US2] Add integration test for dotfiles installation idempotency in `crates/deacon/tests/up_dotfiles.rs`

### Implementation for User Story 2

- [X] T014 [US2] Execute updateContentCommand and prebuild/skip-post-attach sequencing with background task waits in `crates/deacon/src/commands/up.rs`
- [X] T015 [US2] Integrate dotfiles flags/workflow using `crates/core/src/dotfiles.rs` within lifecycle setup in `crates/deacon/src/commands/up.rs`
- [X] T016 [US2] Implement feature-driven image extension with BuildKit/cache options and provenance merge in `crates/deacon/src/commands/up.rs` and `crates/deacon/src/commands/features.rs`
- [X] T017 [US2] Apply UID update flow and security options (privileged, capAdd, securityOpt) in `crates/deacon/src/commands/up.rs` and `crates/core/src/container.rs`

**Checkpoint**: User Story 2 independently testable (prebuild, lifecycle, dotfiles, features, UID/security).

---

## Phase 5: User Story 3 - Compose and reconnect workflows (Priority: P3)

**Goal**: Achieve compose parity with mount conversion, profiles, remote env/secrets handling, runtime path overrides, and expect-existing behavior.

**Independent Test**: Run compose-based fixtures with profiles, extra mounts, remote env, and secrets files; ensure volume conversion, profile application, redacted logs, and fast-fail expect-existing with id labels.

### Tests for User Story 3

- [X] T018 [P] [US3] Add integration test for compose mount conversion and profile selection (including .env project name) in `crates/deacon/tests/up_compose_profiles.rs`
- [X] T019 [P] [US3] Add integration test for expect-existing/id-label fast-fail and remote-env/secrets redaction in `crates/deacon/tests/up_reconnect.rs`

### Implementation for User Story 3

- [ ] T020 [US3] Convert additional mounts to compose volumes and propagate profiles/project name from .env in `crates/deacon/src/commands/up.rs` and `crates/core/src/container.rs` (TODO: All 7 tests in up_compose_profiles.rs disabled)
- [ ] T021 [US3] Merge remote-env flags and secrets-file contents with redaction for runtime/lifecycle env in `crates/deacon/src/commands/up.rs` and `crates/core/src/secrets.rs` (TODO: Secrets file loading and redaction not implemented, tests disabled)
- [X] T022 [US3] Support docker/compose path overrides, data folder options, and buildx cache/platform hooks for compose flows in `crates/deacon/src/commands/up.rs`
- [ ] T023 [US3] Ensure expect-existing/remove-existing logic for compose/id-label flows errors before create/build, with standardized JSON error output in `crates/deacon/src/commands/up.rs` (TODO: Fast-fail logic not implemented, tests disabled)
- [ ] T030 [US3] Implement user-data/container-session folder usage and probe caching hooks in `crates/deacon/src/commands/up.rs` and `crates/core/src/container.rs`; validate via compose fixtures (TODO: No clear implementation found, needs verification)

**Checkpoint**: User Story 3 independently testable (compose parity, reconnection, secrets/redaction).

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Repository-wide consistency, docs, and final validation.

- [X] T024 [P] Update examples and docs to reflect new up flags/output in `examples/` and `docs/subcommand-specs/up/`
- [X] T025 Run quickstart scenarios and adjust fixtures as needed in `fixtures/devcontainer-up/`
- [X] T026 [P] Final fmt/clippy/test full gate and document results in `specs/001-up-gap-spec/quickstart.md`
- [X] T031 [P] Measure representative `deacon up` runs (<3 minutes target) and record results in `specs/001-up-gap-spec/quickstart.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately.
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories.
- **User Stories (Phase 3+)**: Depend on Foundational completion. They can proceed in priority order (US1 → US2 → US3) or in parallel if resources allow.
- **Polish (Phase 6)**: Depends on all desired user stories being complete.

### User Story Dependencies

- **User Story 1 (P1)**: Start after Foundational; no dependency on other stories.
- **User Story 2 (P2)**: Start after Foundational; may reuse US1 normalization/output but should remain independently testable.
- **User Story 3 (P3)**: Start after Foundational; leverages US1 validation and US2 lifecycle readiness but should be testable on compose fixtures alone.

### Within Each User Story

- Tests come before implementation tasks for that story.
- Validation/normalization before runtime/build paths.
- Lifecycle updates before cross-cutting features (dotfiles/features/UID or compose conversions).
- Ensure stdout/stderr contract preserved in every change.

### Parallel Opportunities

- Setup tasks T001–T002 can run in parallel.
- Foundational tasks T003–T004 can run in parallel once setup is done.
- Story-specific tests marked [P] (e.g., T005, T006, T012, T013, T018, T019) can run concurrently.
- Separate story phases (US1, US2, US3) can proceed in parallel after Foundational when staffed separately; avoid touching the same files concurrently (coordinate changes in `crates/deacon/src/commands/up.rs`).

---

## Parallel Example: User Story 1

```bash
# Parallelizable tests
run cargo test -p deacon up_validation
run cargo test -p deacon up_json_output

# Parallelizable implementation prep
edit crates/deacon/src/cli.rs (flags) while another developer refines validation in crates/deacon/src/commands/up.rs
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 (Setup) and Phase 2 (Foundational).
2. Implement Phase 3 (US1) and verify JSON contract/validation via tests.
3. Stop and validate outputs/log separation before moving on.

### Incremental Delivery

1. Finish Setup + Foundational.
2. Deliver US1 → validate → commit.
3. Deliver US2 (prebuild/lifecycle/dotfiles/features) → validate → commit.
4. Deliver US3 (compose/reconnect/secrets) → validate → commit.
5. Run Polish tasks and full gate before PR.

### Parallel Team Strategy

1. Shared team completes Setup + Foundational.
2. Parallel tracks:
   - Track A: US1 flags/validation/output.
   - Track B: US2 lifecycle/prebuild/features/dotfiles.
   - Track C: US3 compose/remote-env/secrets/runtime overrides.
3. Reconvene for Polish and final test gate.
