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

- [ ] T012 [P] [US1] Unit tests for merge_security_options() in crates/core/src/features.rs (all merge rules from contract)
- [ ] T013 [P] [US1] Integration test for privileged mode in crates/deacon/tests/integration_feature_security.rs (docker-shared group)

### Implementation for User Story 1

- [ ] T014 [US1] Implement merge_security_options() function per contract in crates/core/src/features.rs
- [ ] T015 [US1] Update container.rs to call merge_security_options() and apply to Docker create in crates/deacon/src/commands/up/container.rs
- [ ] T016 [US1] Pass --privileged flag when merged_security.privileged is true in crates/core/src/docker.rs
- [ ] T017 [US1] Pass --init flag when merged_security.init is true in crates/core/src/docker.rs
- [ ] T018 [US1] Pass --cap-add flags for all merged capabilities in crates/core/src/docker.rs
- [ ] T019 [US1] Pass --security-opt flags for all merged security options in crates/core/src/docker.rs
- [ ] T020 [US1] Add tracing spans for security options merging in crates/deacon/src/commands/up/container.rs

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently

---

## Phase 4: User Story 2 - Feature Lifecycle Commands Execute Before User Commands (Priority: P1)

**Goal**: Aggregate lifecycle commands from features (in installation order) before config commands, execute with fail-fast

**Independent Test**: Create devcontainer with feature that creates file in onCreateCommand, config command reads file

### Tests for User Story 2

- [ ] T021 [P] [US2] Unit tests for aggregate_lifecycle_commands() in crates/core/src/container_lifecycle.rs (ordering, filtering)
- [ ] T022 [P] [US2] Integration test for lifecycle ordering in crates/deacon/tests/integration_feature_lifecycle.rs (docker-shared group)
- [ ] T023 [P] [US2] Test fail-fast behavior when feature command fails in crates/deacon/tests/integration_feature_lifecycle.rs

### Implementation for User Story 2

- [ ] T024 [US2] Implement aggregate_lifecycle_commands() per contract in crates/core/src/container_lifecycle.rs
- [ ] T025 [US2] Filter empty/null commands using is_empty_command() in crates/core/src/container_lifecycle.rs
- [ ] T026 [US2] Update lifecycle.rs to use aggregate_lifecycle_commands() in crates/deacon/src/commands/up/lifecycle.rs
- [ ] T027 [US2] Implement fail-fast error handling with source attribution in crates/deacon/src/commands/up/lifecycle.rs
- [ ] T028 [US2] Ensure exit code 1 on lifecycle command failure in crates/deacon/src/commands/up/lifecycle.rs
- [ ] T029 [US2] Add tracing spans for lifecycle command execution with source in crates/deacon/src/commands/up/lifecycle.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently

---

## Phase 5: User Story 3 - Feature Mounts Applied to Container (Priority: P2)

**Goal**: Merge feature mounts with config mounts (config takes precedence for same target), apply to container

**Independent Test**: Create devcontainer with feature declaring volume mount, verify mount exists in running container

### Tests for User Story 3

- [ ] T030 [P] [US3] Unit tests for merge_mounts() in crates/core/src/mount.rs (precedence, normalization)
- [ ] T031 [P] [US3] Integration test for mount merging in crates/deacon/tests/integration_feature_mounts.rs (docker-shared group)

### Implementation for User Story 3

- [ ] T032 [US3] Implement merge_mounts() per contract in crates/core/src/mount.rs
- [ ] T033 [US3] Normalize object mounts to string format using MountParser in crates/core/src/mount.rs
- [ ] T034 [US3] Report mount parsing errors with feature attribution in crates/core/src/mount.rs
- [ ] T035 [US3] Update container.rs to call merge_mounts() and pass to Docker in crates/deacon/src/commands/up/container.rs
- [ ] T036 [US3] Add tracing for mount merging operations in crates/deacon/src/commands/up/container.rs

**Checkpoint**: User Stories 1, 2, and 3 should all work independently

---

## Phase 6: User Story 4 - Feature Entrypoints Wrap Container Entry (Priority: P2)

**Goal**: Chain feature entrypoints in installation order, generate wrapper script if multiple

**Independent Test**: Create devcontainer with feature declaring entrypoint wrapper, verify wrapper executes before main command

### Tests for User Story 4

- [ ] T037 [P] [US4] Unit tests for build_entrypoint_chain() in crates/core/src/features.rs
- [ ] T038 [P] [US4] Unit tests for generate_wrapper_script() in crates/core/src/features.rs
- [ ] T039 [P] [US4] Integration test for entrypoint chaining in crates/deacon/tests/integration_feature_entrypoints.rs (docker-shared group)

### Implementation for User Story 4

- [ ] T040 [US4] Implement build_entrypoint_chain() per contract in crates/core/src/features.rs
- [ ] T041 [US4] Implement generate_wrapper_script() per contract in crates/core/src/features.rs
- [ ] T042 [US4] Write wrapper script to container data folder (or temp location if not specified) in crates/deacon/src/commands/up/container.rs
- [ ] T043 [US4] Update Docker create to use entrypoint from chain in crates/core/src/docker.rs
- [ ] T044 [US4] Add tracing for entrypoint chain construction in crates/deacon/src/commands/up/container.rs

**Checkpoint**: All P1 and P2 user stories should work

---

## Phase 7: User Story 5 - Local Feature References Work (Priority: P2)

**Goal**: Support `./` and `../` relative path feature references, resolve relative to devcontainer.json location

**Independent Test**: Create `.devcontainer/my-feature/devcontainer-feature.json`, reference as `./my-feature`, verify installation

### Tests for User Story 5

- [ ] T045 [P] [US5] Unit tests for local path detection in parse_feature_reference() in crates/core/src/feature_ref.rs
- [ ] T046 [P] [US5] Unit tests for resolve_local_feature() path resolution in crates/core/src/feature_ref.rs
- [ ] T047 [P] [US5] Integration test for local feature installation in crates/deacon/tests/integration_feature_refs.rs (fs-heavy group)

### Implementation for User Story 5

- [ ] T048 [US5] Implement resolve_local_feature() per research.md Decision 4 in crates/core/src/feature_ref.rs
- [ ] T049 [US5] Load devcontainer-feature.json from local path in crates/core/src/feature_ref.rs
- [ ] T050 [US5] Report clear errors for missing local paths in crates/core/src/feature_ref.rs
- [ ] T051 [US5] Integrate local feature loading into features_build.rs in crates/deacon/src/commands/up/features_build.rs
- [ ] T052 [US5] Add tracing for local feature resolution in crates/deacon/src/commands/up/features_build.rs

**Checkpoint**: Local features work alongside OCI features

---

## Phase 8: User Story 6 - HTTPS Tarball Feature References Work (Priority: P3)

**Goal**: Support `https://` tarball feature references with 30s timeout and single retry

**Independent Test**: Reference HTTPS URL to feature tarball, verify download and installation

### Tests for User Story 6

- [ ] T053 [P] [US6] Unit tests for HTTPS URL detection in parse_feature_reference() in crates/core/src/feature_ref.rs
- [ ] T054 [P] [US6] Unit tests for is_transient_error() in crates/core/src/feature_ref.rs
- [ ] T055 [P] [US6] Mock HTTP tests for fetch_https_feature() (success, 404, timeout, retry) in crates/core/src/feature_ref.rs

### Implementation for User Story 6

- [ ] T056 [US6] Implement fetch_https_feature() with 30s timeout per research.md Decision 5 in crates/core/src/feature_ref.rs
- [ ] T057 [US6] Implement is_transient_error() for retry logic in crates/core/src/feature_ref.rs
- [ ] T058 [US6] Extract tarball to temp directory in crates/core/src/feature_ref.rs
- [ ] T059 [US6] Parse devcontainer-feature.json from extracted tarball in crates/core/src/feature_ref.rs
- [ ] T060 [US6] Report clear errors for HTTPS failures with URL in crates/core/src/feature_ref.rs
- [ ] T061 [US6] Integrate HTTPS feature loading into features_build.rs in crates/deacon/src/commands/up/features_build.rs
- [ ] T062 [US6] Add tracing for HTTPS download with progress in crates/deacon/src/commands/up/features_build.rs

**Checkpoint**: All user stories should now be independently functional

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] T063 [P] Add example for features with lifecycle commands in examples/ (include exec.sh per Constitution VIII)
- [ ] T064 [P] Add example for local feature reference in examples/ (include exec.sh per Constitution VIII)
- [ ] T065 Run quickstart.md validation scenarios manually
- [ ] T066 Update deacon up --help with new feature behaviors
- [ ] T067 Verify all integration tests are assigned to correct nextest groups in .config/nextest.toml

---

## Deferred Work

**Purpose**: Track implementation work deferred from MVP per research.md decisions

No deferrals identified in research.md. All decisions have direct implementation paths.

**Checkpoint**: Specification complete when all tasks are resolved

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-8)**: All depend on Foundational phase completion
  - US1 (Security) and US2 (Lifecycle) can proceed in parallel (both P1)
  - US3 (Mounts), US4 (Entrypoints), US5 (Local Refs) can proceed in parallel after P1 stories
  - US6 (HTTPS) can start after Foundational but is P3 priority
- **Polish (Phase 9)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational - No dependencies on other stories
- **User Story 2 (P1)**: Can start after Foundational - No dependencies on other stories
- **User Story 3 (P2)**: Can start after Foundational - Independent of other stories
- **User Story 4 (P2)**: Can start after Foundational - Independent of other stories
- **User Story 5 (P2)**: Can start after Foundational - Independent of other stories
- **User Story 6 (P3)**: Can start after Foundational - Independent of other stories

### Within Each User Story

- Tests SHOULD be written first to verify requirements
- Data structures before business logic
- Core logic before integration
- Integration before tracing/polish

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel (T002-T005)
- All Foundational tasks marked [P] can run in parallel (T008-T009)
- Test tasks within each user story can run in parallel
- User stories 1 and 2 can run in parallel (both P1)
- User stories 3, 4, 5, 6 can run in parallel (after P1)

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
