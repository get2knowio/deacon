# Tasks: Complete Feature Support During Up Command

**Input**: Design documents from `/specs/009-complete-feature-support/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Tests are included following the project's testing patterns (cargo-nextest with test groups).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Crates**: `crates/deacon/` (CLI binary), `crates/core/` (library)
- **Tests**: Inline tests in modules, integration tests in `crates/*/tests/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create new data types and extend existing structures required by all user stories

- [x] T001 Create FeatureRefType enum and parsing logic in crates/core/src/feature_ref.rs
- [x] T002 [P] Create MergedSecurityOptions struct in crates/core/src/features.rs
- [x] T003 [P] Create LifecycleCommandSource and AggregatedLifecycleCommand types in crates/core/src/container_lifecycle.rs
- [x] T004 [P] Create MergedMounts struct in crates/core/src/mount.rs
- [x] T005 [P] Create EntrypointChain enum in crates/core/src/features.rs
- [x] T006 Extend FeatureBuildOutput to include merged_security, merged_mounts, entrypoint_chain in crates/core/src/features.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**CRITICAL**: No user story work can begin until this phase is complete

- [x] T007 Implement parse_feature_reference() function in crates/core/src/feature_ref.rs (detection for OCI/LocalPath/HttpsTarball)
- [x] T008 [P] Add is_empty_command() helper function in crates/core/src/container_lifecycle.rs
- [x] T009 [P] Add deduplicate_uppercase() helper for capability normalization in crates/core/src/features.rs
- [x] T010 Wire feature_ref.rs into crates/core/src/lib.rs exports
- [x] T011 Add unit tests for parse_feature_reference() in crates/core/src/feature_ref.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Feature Security Options Applied to Container (Priority: P1)

**Goal**: Merge security options (privileged, init, capAdd, securityOpt) from features and config, apply to container creation

**Independent Test**: Create devcontainer with feature requiring `privileged: true`, verify container has `--privileged` flag

### Tests for User Story 1

- [x] T012 [P] [US1] Unit tests for merge_security_options() in crates/core/src/features.rs (all merge rules from contract)
- [x] T013 [P] [US1] Integration test for privileged mode in crates/deacon/tests/integration_feature_security.rs (docker-shared group)

### Implementation for User Story 1

- [x] T014 [US1] Implement merge_security_options() function per contract in crates/core/src/features.rs
- [x] T015 [US1] Update container.rs to call merge_security_options() and apply to Docker create in crates/deacon/src/commands/up/container.rs
- [x] T016 [US1] Pass --privileged flag when merged_security.privileged is true in crates/core/src/docker.rs
- [x] T017 [US1] Pass --init flag when merged_security.init is true in crates/core/src/docker.rs
- [x] T018 [US1] Pass --cap-add flags for all merged capabilities in crates/core/src/docker.rs
- [x] T019 [US1] Pass --security-opt flags for all merged security options in crates/core/src/docker.rs
- [x] T020 [US1] Add tracing spans for security options merging in crates/deacon/src/commands/up/container.rs

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently

---

## Phase 4: User Story 2 - Feature Lifecycle Commands Execute Before User Commands (Priority: P1)

**Goal**: Aggregate lifecycle commands from features (in installation order) before config commands, execute with fail-fast

**Independent Test**: Create devcontainer with feature that creates file in onCreateCommand, config command reads file

### Tests for User Story 2

- [x] T021 [P] [US2] Unit tests for aggregate_lifecycle_commands() in crates/core/src/container_lifecycle.rs (ordering, filtering)
- [x] T022 [P] [US2] Integration test for lifecycle ordering in crates/deacon/tests/integration_feature_lifecycle.rs (docker-shared group)
- [x] T023 [P] [US2] Test fail-fast behavior when feature command fails in crates/deacon/tests/integration_feature_lifecycle.rs

### Implementation for User Story 2

- [x] T024 [US2] Implement aggregate_lifecycle_commands() per contract in crates/core/src/container_lifecycle.rs
- [x] T025 [US2] Filter empty/null commands using is_empty_command() in crates/core/src/container_lifecycle.rs
- [x] T026 [US2] Update lifecycle.rs to use aggregate_lifecycle_commands() in crates/deacon/src/commands/up/lifecycle.rs
- [x] T027 [US2] Implement fail-fast error handling with source attribution in crates/deacon/src/commands/up/lifecycle.rs
- [x] T028 [US2] Ensure exit code 1 on lifecycle command failure in crates/deacon/src/commands/up/lifecycle.rs
- [x] T029 [US2] Add tracing spans for lifecycle command execution with source in crates/deacon/src/commands/up/lifecycle.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently

---

## Phase 5: User Story 3 - Feature Mounts Applied to Container (Priority: P2)

**Goal**: Merge feature mounts with config mounts (config takes precedence for same target), apply to container

**Independent Test**: Create devcontainer with feature declaring volume mount, verify mount exists in running container

### Tests for User Story 3

- [x] T030 [P] [US3] Unit tests for merge_mounts() in crates/core/src/mount.rs (precedence, normalization)
- [x] T031 [P] [US3] Integration test for mount merging in crates/deacon/tests/integration_feature_mounts.rs (docker-shared group)

### Implementation for User Story 3

- [x] T032 [US3] Implement merge_mounts() per contract in crates/core/src/mount.rs
- [x] T033 [US3] Normalize object mounts to string format using MountParser in crates/core/src/mount.rs
- [x] T034 [US3] Report mount parsing errors with feature attribution in crates/core/src/mount.rs
- [x] T035 [US3] Update container.rs to call merge_mounts() and pass to Docker in crates/deacon/src/commands/up/container.rs
- [x] T036 [US3] Add tracing for mount merging operations in crates/deacon/src/commands/up/container.rs

**Checkpoint**: User Stories 1, 2, and 3 should all work independently

---

## Phase 6: User Story 4 - Feature Entrypoints Wrap Container Entry (Priority: P2)

**Goal**: Chain feature entrypoints in installation order, generate wrapper script if multiple

**Independent Test**: Create devcontainer with feature declaring entrypoint wrapper, verify wrapper executes before main command

### Tests for User Story 4

- [x] T037 [P] [US4] Unit tests for build_entrypoint_chain() in crates/core/src/features.rs
- [x] T038 [P] [US4] Unit tests for generate_wrapper_script() in crates/core/src/features.rs
- [x] T039 [P] [US4] Integration test for entrypoint chaining in crates/deacon/tests/integration_feature_entrypoints.rs (docker-shared group)

### Implementation for User Story 4

- [x] T040 [US4] Implement build_entrypoint_chain() per contract in crates/core/src/features.rs
- [x] T041 [US4] Implement generate_wrapper_script() per contract in crates/core/src/features.rs
- [x] T042 [US4] Write wrapper script to container data folder (or temp location if not specified) in crates/deacon/src/commands/up/container.rs
- [x] T043 [US4] Update Docker create to use entrypoint from chain in crates/core/src/docker.rs
- [x] T044 [US4] Add tracing for entrypoint chain construction in crates/deacon/src/commands/up/container.rs

**Checkpoint**: All P1 and P2 user stories should work

---

## ~~Phase 7: User Story 5 - Local Feature References Work (Priority: P2)~~ [DESCOPED]

**Descoped**: Feature-creator tooling — deferred to a future branch.

- [x] T045–T052 [US5] Descoped — local feature references are creator-facing, not consumer-facing

---

## ~~Phase 8: User Story 6 - HTTPS Tarball Feature References Work (Priority: P3)~~ [DESCOPED]

**Descoped**: Feature-distributor tooling — deferred to a future branch.

- [x] T053–T062 [US6] Descoped — HTTPS tarball references are creator/distributor-facing, not consumer-facing

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [x] T063 [P] Add example for features with lifecycle commands in examples/ (include exec.sh per Constitution VIII) — descoped (US2 complete without example)
- [x] T064 [P] Add example for local feature reference in examples/ (include exec.sh per Constitution VIII) — descoped (US5 descoped)
- [x] T065 Run quickstart.md validation scenarios manually
- [x] T066 Update deacon up --help with new feature behaviors
- [x] T067 Verify all integration tests are assigned to correct nextest groups in .config/nextest.toml

---

## Deferred Work

**Purpose**: Track implementation work deferred from MVP per research.md decisions

No deferrals identified in research.md. All decisions have direct implementation paths.

**Checkpoint**: Specification complete when all tasks are resolved

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: Complete ✅
- **Foundational (Phase 2)**: Complete ✅
- **User Story 1 - Security (Phase 3)**: Complete ✅
- **User Story 2 - Lifecycle (Phase 4)**: Complete ✅
- **User Story 3 - Mounts (Phase 5)**: Complete ✅
- **User Story 4 - Entrypoints (Phase 6)**: Complete ✅
- **User Story 5 - Local Refs (Phase 7)**: Descoped (creator-facing)
- **User Story 6 - HTTPS Tarballs (Phase 8)**: Descoped (creator/distributor-facing)
- **Polish (Phase 9)**: Complete ✅

### Remaining Work

All tasks complete. ✅

- ~~**US4 (Phase 6)**: T037-T044 — Entrypoint chaining implementation~~ ✅
- ~~**Polish**: T065-T067 — Validation, help text, nextest audit~~ ✅
- ~~**Bug fix**: Fix `test_empty_string_error` in feature_ref.rs (empty string not rejected)~~ ✅ Fixed
- ~~**Cleanup**: Remove leftover files (test_security_merge.sh, maverick.yaml)~~ ✅ Removed

---

## Parallel Example: User Story 1 (Security Options)

```bash
# Launch tests in parallel:
Task: "Unit tests for merge_security_options() in crates/core/src/features.rs"
Task: "Integration test for privileged mode in crates/deacon/tests/integration_feature_security.rs"

# Then implement sequentially:
Task: "Implement merge_security_options() function"
Task: "Update container.rs to call merge_security_options()"
Task: "Pass --privileged, --init, --cap-add, --security-opt flags"
```

---

## Implementation Strategy

### MVP First (User Stories 1 and 2 Only)

1. Complete Phase 1: Setup (data types)
2. Complete Phase 2: Foundational (parsing, helpers)
3. Complete Phase 3: User Story 1 (Security Options)
4. Complete Phase 4: User Story 2 (Lifecycle Commands)
5. **STOP and VALIDATE**: Test both stories independently
6. Deploy/demo if ready - features with security options and lifecycle commands work

### Incremental Delivery

1. Complete Setup + Foundational -> Foundation ready
2. Add User Story 1 (Security) -> Features requiring privileged/capabilities work
3. Add User Story 2 (Lifecycle) -> Feature lifecycle commands execute correctly
4. Add User Story 3 (Mounts) -> Feature-declared mounts apply
5. Add User Story 4 (Entrypoints) -> Feature entrypoint wrappers chain
6. Add User Story 5 (Local Refs) -> Development with local features works
7. Add User Story 6 (HTTPS) -> Full spec compliance

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1 (Security Options)
   - Developer B: User Story 2 (Lifecycle Commands)
3. After P1 stories:
   - Developer A: User Story 3 (Mounts)
   - Developer B: User Story 4 (Entrypoints)
   - Developer C: User Story 5 (Local Refs)
4. User Story 6 (HTTPS) can be done by any developer

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests pass after implementing each story
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
