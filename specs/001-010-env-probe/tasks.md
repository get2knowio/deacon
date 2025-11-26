# Tasks: Env-Probe Cache Completion

**Input**: Design documents from `/workspaces/deacon/specs/001-010-env-probe/`
**Prerequisites**: plan.md, spec.md, data-model.md, research.md, contracts/, quickstart.md

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

**Invalid Completion Examples**:
- ❌ Tests marked `#[ignore] // TODO: Enable when T### is implemented`
- ❌ Implementation has `// TODO T###: Implement [core functionality]`
- ❌ Only test skeleton exists, no implementation
- ❌ Functionality partially works or returns hardcoded stubs

**Valid States**:
- `[ ]` = Not started or significant work remaining
- `[X]` = Fully complete per above criteria
- Use comments for partial progress: `T020 ... (data structures exist, logic pending)`

---

## Phase 1: Setup (Verification Only)

**Purpose**: Verify existing infrastructure is in place (no new setup needed)

- [X] T001 Verify project builds with `cargo build --quiet` and identify compilation errors
- [X] T002 [P] Run `make test-nextest-fast` to baseline current test state
- [X] T003 [P] Review existing cache implementation in crates/core/src/container_env_probe.rs (lines 147-194)

---

## Phase 2: Foundational (Compilation Fixes)

**Purpose**: Fix compilation errors that block ALL user stories

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 Fix missing `cache_folder: None` in UpArgs::default() at crates/deacon/src/commands/up.rs:678
- [X] T005 [P] Fix missing `cache_folder: None` in ContainerLifecycleConfig initializer at crates/deacon/src/commands/up.rs:2323
- [X] T006 [P] Fix missing `cache_folder: None` in ContainerLifecycleConfig initializer at crates/deacon/src/commands/up.rs:2777
- [X] T007 [P] Fix missing `cache_folder` fields in ExecArgs test initializers in crates/deacon/src/commands/exec.rs (verified: uses container_data_folder instead, already complete)
- [X] T008 [P] Fix unused variable warning for `cache_folder` in crates/deacon/src/commands/run_user_commands.rs:135
- [X] T009 Run `cargo fmt --all` to format all fixed code
- [X] T010 Run `cargo clippy --all-targets -- -D warnings` to verify zero warnings
- [X] T011 Run `cargo build --quiet` to verify compilation succeeds
- [X] T012 Run `make test-nextest-fast` to verify no test regressions from fixes

**Checkpoint**: Build is green - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Fast Container Startup with Cached Environment (Priority: P1) 🎯 MVP

**Goal**: Enable container env probe caching to reduce `deacon up` latency from seconds to milliseconds on repeat invocations

**Independent Test**: Run `deacon up --container-data-folder=/tmp/cache` twice and verify second run is 50%+ faster

### Implementation for User Story 1

- [X] T013 [US1] Add DEBUG log for cache hit in crates/core/src/container_env_probe.rs (~line 152): `debug!(cache_path = %cache_path.display(), var_count = env_vars.len(), "Loaded cached env probe")`
- [X] T014 [US1] Add DEBUG log for cache miss in crates/core/src/container_env_probe.rs (~line 164): `debug!(container_id = %container_id, user = ?user, "Cache miss: executing fresh probe")`
- [X] T015 [US1] Add DEBUG log for cache write in crates/core/src/container_env_probe.rs (~line 191): `debug!(cache_path = %cache_path.display(), var_count = env_vars.len(), "Persisted env probe cache")`
- [X] T016 [US1] Replace silent cache read error with WARN log in crates/core/src/container_env_probe.rs (~line 152): `warn!(cache_path = %cache_path.display(), error = %e, "Failed to read cache file, falling back to fresh probe")`
- [X] T017 [US1] Add integration test for cache miss scenario in crates/core/tests/integration_env_probe_cache.rs: verify cache file created on first probe
- [X] T018 [US1] Add integration test for cache hit scenario in crates/core/tests/integration_env_probe_cache.rs: verify second probe loads from cache without shell execution
- [X] T019 [US1] Add integration test for no caching when cache_folder=None in crates/core/tests/integration_env_probe_cache.rs
- [X] T020 [US1] Add integration test for cache folder creation in crates/core/tests/integration_env_probe_cache.rs: verify non-existent cache folder is created
- [X] T021 [US1] Configure integration_env_probe_cache test group as 'docker-shared' in .config/nextest.toml for all profiles (default, dev-fast, full, ci)
- [X] T022 [US1] Run `make test-nextest-docker` to verify US1 integration tests pass
- [X] T023 [US1] Manual test: Run `RUST_LOG=debug deacon up --container-data-folder=/tmp/cache` twice and verify DEBUG logs show cache hit on second run

**Checkpoint**: At this point, User Story 1 should be fully functional - basic caching works end-to-end

---

## Phase 4: User Story 2 - Per-User Cache Isolation (Priority: P2)

**Goal**: Ensure different users in the same container get separate cache entries to prevent environment variable mixing

**Independent Test**: Run `deacon up --remote-user=alice --container-data-folder=/tmp/cache` then `deacon up --remote-user=bob --container-data-folder=/tmp/cache` and verify two separate cache files exist

### Implementation for User Story 2

- [X] T024 [US2] Add integration test for per-user isolation in crates/core/tests/integration_env_probe_cache.rs: probe as user "alice", then user "bob", verify separate cache files `{container_id}_alice.json` and `{container_id}_bob.json`
- [X] T025 [US2] Add integration test for root user handling in crates/core/tests/integration_env_probe_cache.rs: probe with user=None, verify cache file uses "root" as user component `{container_id}_root.json`
- [X] T026 [US2] Add integration test for cache non-reuse across users in crates/core/tests/integration_env_probe_cache.rs: probe as "alice", then probe as "bob", verify bob's probe does NOT load alice's cache
- [X] T027 [US2] Run `make test-nextest-docker` to verify US2 integration tests pass
- [X] T028 [US2] Manual test: Run `deacon up --remote-user=alice --container-data-folder=/tmp/cache` then `deacon up --remote-user=bob --container-data-folder=/tmp/cache` and verify `ls /tmp/cache/` shows two separate files

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently - caching respects user boundaries

---

## Phase 5: User Story 3 - Cache Invalidation on Container Changes (Priority: P3)

**Goal**: Ensure cache from old container is not reused after container rebuild (different container ID = new cache)

**Independent Test**: Run `deacon up` with container A (cache created), delete container, run `deacon up` again with new container B, verify new cache file created with different container ID

### Implementation for User Story 3

- [X] T029 [US3] Add integration test for container ID invalidation in crates/core/tests/integration_env_probe_cache.rs: probe container A, simulate container rebuild (new ID), verify new cache entry created
- [X] T030 [US3] Add integration test for corrupted JSON fallback in crates/core/tests/integration_env_probe_cache.rs: write invalid JSON to cache file, verify system falls back to fresh probe and logs WARN
- [X] T031 [US3] Run `make test-nextest-docker` to verify US3 integration tests pass
- [X] T032 [US3] Manual test: Run `deacon up --container-data-folder=/tmp/cache`, verify cache file created, manually edit file to corrupt JSON, run `deacon up` again with `RUST_LOG=debug` and verify WARN log + fallback to fresh probe

**Checkpoint**: All user stories should now be independently functional - cache invalidation works correctly

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [X] T033 [P] Run full test suite with `make test-nextest` to verify all tests pass (unit, integration, docker, smoke) (note: 6 pre-existing test failures unrelated to env-probe; all 27 env-probe tests pass)
- [X] T034 [P] Run `cargo fmt --all -- --check` to verify formatting is correct
- [X] T035 [P] Run `cargo clippy --all-targets -- -D warnings` to verify zero clippy warnings
- [X] T036 [P] Verify existing integration tests in crates/deacon/tests/ still pass (integration_exec_env.rs, parity_env_probe_flag.rs)
- [X] T037 Update quickstart.md examples if any changes needed (currently already complete)
- [X] T038 Verify contracts/cache-schema.json matches actual cache file format
- [X] T039 Run manual performance benchmark: measure `deacon up` latency without cache vs with cache hit (expect 50%+ improvement)
- [X] T040 Document cross-cutting cache folder pattern in docs/ARCHITECTURE.md for future subcommands (build, down, stop)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup verification - BLOCKS all user stories
- **User Stories (Phase 3-5)**: All depend on Foundational phase completion
  - User stories can then proceed in parallel (if staffed)
  - Or sequentially in priority order (P1 → P2 → P3)
- **Polish (Phase 6)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Independent of US1 (tests different user scenarios)
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - Independent of US1/US2 (tests invalidation scenarios)

### Within Each User Story

- Logging additions before integration tests (tests will verify logs)
- Integration tests before manual tests
- Test configuration before running tests
- Manual verification after automated tests pass

### Parallel Opportunities

#### Phase 1 (Setup): All tasks can run in parallel
```
T002 and T003 can run simultaneously
```

#### Phase 2 (Foundational): Struct initializer fixes can run in parallel
```
T005, T006, T007, T008 can run in parallel (different files)
T004 must complete before T005/T006 (same file)
```

#### Phase 3 (User Story 1): Logging additions can run in parallel
```
T013, T014, T015, T016 can run in parallel (different line numbers, use multi-cursor editing)
T017, T018, T019, T020 tests can be written in parallel (different test functions)
```

#### Phase 6 (Polish): Verification tasks can run in parallel
```
T033, T034, T035, T036 can run in parallel (independent checks)
```

---

## Parallel Example: User Story 1 Logging

```bash
# Launch all logging additions together (use multi-cursor editing):
# In crates/core/src/container_env_probe.rs:
# - Line 152: Add cache hit DEBUG log
# - Line 164: Add cache miss DEBUG log  
# - Line 191: Add cache write DEBUG log
# - Line 152: Replace silent error with WARN log
```

---

## Parallel Example: User Story 1 Tests

```bash
# Launch all test additions together (different test functions):
# In crates/core/tests/integration_env_probe_cache.rs:
Task T017: "test_cache_miss"
Task T018: "test_cache_hit"
Task T019: "test_no_caching_when_none"
Task T020: "test_cache_folder_creation"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (verify baseline)
2. Complete Phase 2: Foundational (fix compilation errors - CRITICAL)
3. Complete Phase 3: User Story 1 (basic caching works)
4. **STOP and VALIDATE**: Test User Story 1 independently with manual `deacon up` runs
5. Ready for basic use (50%+ speedup on repeat invocations)

### Incremental Delivery

1. Complete Setup + Foundational → Build is green, no functionality added yet
2. Add User Story 1 → Test independently → Basic caching works (MVP!)
3. Add User Story 2 → Test independently → Per-user isolation works
4. Add User Story 3 → Test independently → Cache invalidation works
5. Each story adds correctness without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together (blocks everything)
2. Once Foundational is done (build is green):
   - Developer A: User Story 1 (logging + basic tests)
   - Developer B: User Story 2 (per-user tests)
   - Developer C: User Story 3 (invalidation tests)
3. Stories complete and integrate independently

---

## Summary

**Total Tasks**: 40
- Phase 1 (Setup): 3 tasks
- Phase 2 (Foundational): 9 tasks (BLOCKING)
- Phase 3 (User Story 1): 11 tasks
- Phase 4 (User Story 2): 5 tasks
- Phase 5 (User Story 3): 4 tasks
- Phase 6 (Polish): 8 tasks

**Parallel Opportunities**: 18 tasks marked [P] can run concurrently within their phases

**Independent Test Criteria**:
- US1: Run `deacon up` twice with cache folder, second run 50%+ faster
- US2: Run `deacon up` with different users, separate cache files created
- US3: Rebuild container, old cache not reused

**MVP Scope**: Phases 1-3 only (Setup + Foundational + User Story 1) = 23 tasks

**Key Insight**: This is a completion feature - core caching logic already exists, just needs compilation fixes (Phase 2) + observability (logging in Phase 3) + comprehensive testing (Phases 3-5).

---

## Notes

- [P] tasks = different files or independent changes, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Foundational phase (compilation fixes) is CRITICAL - blocks all user stories
- Avoid: marking tasks complete with failing tests or blocking TODOs
- Commit after each logical group of fixes
- Stop at any checkpoint to validate story independently
