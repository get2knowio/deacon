# Tasks: Fix Lifecycle Command Format Support

**Input**: Design documents from `/specs/012-fix-lifecycle-formats/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/lifecycle-execution.md

**Tests**: Included per SC-007 (spec requires new automated tests for array and object formats).

**Organization**: Tasks grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- **Core crate**: `crates/core/src/` (domain logic: types, parsing, execution)
- **CLI crate**: `crates/deacon/src/commands/` (CLI orchestration, delegates to core)
- **Core tests**: `crates/core/tests/` (integration tests against core APIs)

---

## Phase 1: Setup

**Purpose**: Verify prerequisites are in place

- [X] T001 Verify `indexmap` dependency with `serde` feature exists in `crates/core/Cargo.toml` (already present: `indexmap = { version = "2.0", features = ["serde"] }`) and ensure `tokio` has `rt` feature for `JoinSet` in `crates/core/Cargo.toml`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Introduce `LifecycleCommandValue` type system and thread it through the aggregation and command container types. ALL user story work depends on this phase.

**CRITICAL**: No user story work can begin until this phase is complete.

- [X] T002 Define `LifecycleCommandValue` enum with `Shell(String)`, `Exec(Vec<String>)`, `Parallel(IndexMap<String, LifecycleCommandValue>)` variants, implement `from_json_value(&serde_json::Value) -> Result<Self>` parsing with spec-compliant type detection (null→None, string→Shell, array→Exec with all-string validation, object→Parallel with recursive parsing, other→Err), `is_empty() -> bool` for no-op detection, and `substitute_variables(&VariableSubstitution) -> Self` for element-wise substitution in `crates/core/src/container_lifecycle.rs`
- [X] T003 [P] Define `ParallelCommandResult` struct with fields `key: String`, `exit_code: i32`, `duration: Duration`, `success: bool` in `crates/core/src/container_lifecycle.rs`
- [X] T004 Change `AggregatedLifecycleCommand.command` field type from `serde_json::Value` to `LifecycleCommandValue` and update `aggregate_lifecycle_commands()` (line ~145) to call `LifecycleCommandValue::from_json_value()` during aggregation, filtering `None` results (null/empty) in `crates/core/src/container_lifecycle.rs`
- [X] T005 Change `ContainerLifecycleCommands` (line ~1548) phase fields from `Option<Vec<String>>` to `Option<LifecycleCommandList>` and update all builder methods (`with_on_create`, `with_post_create`, etc.) to accept `LifecycleCommandList` in `crates/core/src/container_lifecycle.rs`
- [X] T006 [P] Add unit tests for `LifecycleCommandValue::from_json_value()` covering: string→Shell, empty string→empty Shell, array→Exec, empty array→empty Exec, array with non-string→Err, object→Parallel with string and array values, object with invalid value type→skip with log, empty object→empty Parallel, null→None, number/boolean→Err in `crates/core/src/container_lifecycle.rs` tests module

**Checkpoint**: Type system in place — `LifecycleCommandValue` defined, aggregation produces typed commands, `ContainerLifecycleCommands` carries typed commands. Compilation will have errors in downstream code until Phase 3 wires up execution.

---

## Phase 3: User Story 1 — String Lifecycle Commands (Priority: P1) MVP

**Goal**: Wire the new `LifecycleCommandValue` type through the full execution pipeline, preserving existing Shell (string) format behavior. After this phase, string lifecycle commands work exactly as before through the typed pipeline.

**Independent Test**: Run `make test-nextest-fast` — all existing lifecycle tests must pass without modification (SC-005, SC-006).

### Implementation for User Story 1

- [X] T007 [US1] Update `execute_lifecycle_phase_impl()` (line ~958) to match on `LifecycleCommandValue` variants: for `Shell(cmd)`, preserve existing `sh -c` wrapping logic (login shell detection via `get_shell_command_for_lifecycle` or `["sh", "-c", cmd]`); for `Exec` and `Parallel`, add `todo!()` stubs (implemented in US2/US3) in `crates/core/src/container_lifecycle.rs`
- [X] T008 [US1] Update `execute_host_lifecycle_phase()` (line ~712) to accept `&[AggregatedLifecycleCommand]` instead of `&[String]`, dispatch `Shell(cmd)` through existing `crate::lifecycle::run_phase()` path, add `todo!()` stubs for `Exec`/`Parallel` in `crates/core/src/container_lifecycle.rs`
- [X] T009 [US1] Update the phase-processing loop in `execute_container_lifecycle_with_progress_callback_and_docker()` (line ~387) to iterate `LifecycleCommandList` from `ContainerLifecycleCommands` instead of `Vec<String>`, passing `AggregatedLifecycleCommand` items to the phase executor in `crates/core/src/container_lifecycle.rs`
- [X] T010 [US1] Refactor `execute_lifecycle_commands()` (line ~137) in `crates/deacon/src/commands/up/lifecycle.rs`: replace `flatten_aggregated_commands()` calls with direct use of `LifecycleCommandList` from `aggregate_lifecycle_commands()`; set each phase on `ContainerLifecycleCommands` using the typed builder methods; remove `flatten_aggregated_commands()`, `commands_from_json_value()`, and `shell_quote_for_exec()` functions and their tests
- [X] T011 [US1] Refactor `execute_initialize_command()` (line ~504) in `crates/deacon/src/commands/up/lifecycle.rs`: replace `commands_from_json_value()` call with `LifecycleCommandValue::from_json_value()`, pass typed command to host execution path
- [X] T012 [P] [US1] Refactor `run_user_commands.rs` in `crates/deacon/src/commands/run_user_commands.rs`: replace local `commands_from_json_value()` calls (lines ~152-191) with `LifecycleCommandValue::from_json_value()` from core, build `ContainerLifecycleCommands` with `LifecycleCommandList` per phase, remove local `commands_from_json_value()` function (line ~234) and its tests
- [X] T013 [US1] Update integration tests that construct `ContainerLifecycleCommands` with `Vec<String>` to use `LifecycleCommandList` with `AggregatedLifecycleCommand` containing `LifecycleCommandValue::Shell` in: `crates/core/tests/integration_container_lifecycle.rs`, `crates/core/tests/integration_non_blocking_lifecycle.rs`, `crates/core/tests/integration_per_command_events.rs`, `crates/core/tests/integration_mock_runtime.rs`
- [X] T014 [US1] Remove old test file references: delete `commands_from_json_value` tests from `crates/deacon/src/commands/up/tests.rs` (lines ~29-47) that test the removed function
- [X] T015 [US1] Run `make test-nextest-fast` to verify all existing lifecycle tests pass with zero regressions (SC-005, SC-006)

**Checkpoint**: String-format lifecycle commands work identically to before. The typed pipeline is fully wired. `Exec` and `Parallel` branches have `todo!()` stubs. All existing tests pass.

---

## Phase 4: User Story 2 — Array (Exec-Style) Lifecycle Commands (Priority: P1)

**Goal**: Array-format lifecycle commands execute directly without shell wrapping, passing arguments as-is to the OS (exec-style semantics).

**Independent Test**: Create a devcontainer.json with `"postCreateCommand": ["echo", "hello world"]` and verify `echo` receives `hello world` as a single argument (no shell splitting).

### Implementation for User Story 2

- [X] T016 [US2] Replace `todo!()` stub for `Exec(args)` in `execute_lifecycle_phase_impl()`: pass `args` directly to `docker.exec(container_id, &args, exec_config)` as the command slice — first element is executable, remaining are arguments, no `sh -c` wrapper in `crates/core/src/container_lifecycle.rs`
- [X] T017 [US2] Replace `todo!()` stub for `Exec(args)` in `execute_host_lifecycle_phase()`: use `tokio::process::Command::new(&args[0]).args(&args[1..])` for direct OS invocation without shell in `crates/core/src/container_lifecycle.rs`
- [X] T018 [US2] Add unit tests for exec-style execution: verify array args are passed without shell wrapping (no quoting, no `sh -c`), verify empty array is no-op, verify single-element array works, verify args with spaces/metacharacters are preserved literally in `crates/core/src/container_lifecycle.rs` tests module

**Checkpoint**: Array-format commands execute exec-style in both container and host paths. Shell metacharacters in arguments are passed literally (SC-002).

---

## Phase 5: User Story 3 — Object (Parallel) Lifecycle Commands (Priority: P1)

**Goal**: Object-format lifecycle commands execute all named entries concurrently, with per-entry output attribution and proper error aggregation.

**Independent Test**: Create a devcontainer.json with `"postCreateCommand": {"install": "npm install", "build": ["npm", "run", "build"]}` and verify both commands run concurrently (not sequentially).

### Implementation for User Story 3

- [X] T019 [US3] Replace `todo!()` stub for `Parallel(entries)` in `execute_lifecycle_phase_impl()`: spawn one `tokio::JoinSet` task per entry, dispatch each value as Shell (via `sh -c`) or Exec (direct args) to `docker.exec()`, wait for all tasks to complete, collect `ParallelCommandResult` per entry in `crates/core/src/container_lifecycle.rs`
- [X] T020 [US3] Add output attribution for parallel commands: prefix output/logs with `[key]` per entry (FR-012), emit per-entry progress events with `command_id: "{phase}-{key}"` format (`LifecycleCommandBegin`/`LifecycleCommandEnd` per entry, `LifecyclePhaseBegin`/`LifecyclePhaseEnd` for the set) in `crates/core/src/container_lifecycle.rs`
- [X] T021 [US3] Implement error aggregation for parallel commands: wait for ALL commands to complete (no cancellation on first failure per Decision 8), if any command failed report phase failure with all failed keys and exit codes in error message in `crates/core/src/container_lifecycle.rs`
- [X] T022 [US3] Replace `todo!()` stub for `Parallel(entries)` in `execute_host_lifecycle_phase()`: use `tokio::JoinSet` with `tokio::task::spawn_blocking` per entry, dispatch Shell via `sh -c` and Exec via `Command::new().args()`, wait for all, aggregate errors in `crates/core/src/container_lifecycle.rs`
- [X] T023 [US3] Add unit tests for parallel execution: verify concurrent execution (multiple entries run simultaneously), verify error aggregation (one failure fails phase, all entries complete), verify mixed format values (Shell + Exec in same object), verify empty object is no-op, verify [key] attribution in progress events in `crates/core/src/container_lifecycle.rs` tests module

**Checkpoint**: Object-format commands execute concurrently with per-key attribution. Failed entries don't cancel siblings. Phase fails if any entry fails (SC-003, SC-004).

---

## Phase 6: User Story 4 — All Six Lifecycle Commands Support All Formats (Priority: P2)

**Goal**: Verify all three formats (string, array, object) work uniformly across all six lifecycle phases: `initializeCommand`, `onCreateCommand`, `updateContentCommand`, `postCreateCommand`, `postStartCommand`, `postAttachCommand`.

**Independent Test**: Configure each lifecycle command with each format and verify correct execution behavior.

### Implementation for User Story 4

- [X] T024 [US4] Audit the execution path for all six lifecycle phases in `execute_container_lifecycle_with_progress_callback_and_docker()` and `execute_lifecycle_commands()` to confirm all phases route through the format-aware `execute_lifecycle_phase_impl()` (container) and `execute_host_lifecycle_phase()` (host for initializeCommand), fix any phase that bypasses format dispatch in `crates/core/src/container_lifecycle.rs` and `crates/deacon/src/commands/up/lifecycle.rs`
- [X] T025 [P] [US4] Add integration tests for array format with `onCreateCommand` (container path) and `initializeCommand` (host path) to verify exec-style semantics in both execution contexts in `crates/core/tests/test_lifecycle_formats.rs`
- [X] T026 [P] [US4] Add integration tests for object format with `postCreateCommand` (container path) and `initializeCommand` (host path) to verify concurrent execution in both contexts in `crates/core/tests/test_lifecycle_formats.rs`

**Checkpoint**: All format × phase combinations work. SC-001 met (all three formats execute successfully for all six lifecycle phases).

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Final validation, lint compliance, and cleanup

- [X] T027 Run `cargo fmt --all && cargo clippy --all-targets -- -D warnings` for lint and format compliance
- [X] T028 Run `make test-nextest-fast` for final comprehensive validation (SC-005, SC-006, SC-007)
- [X] T029 Configure nextest test groups for new integration tests in `.config/nextest.toml`: add `test_lifecycle_formats` to appropriate group (docker-shared or unit depending on mock usage)
- [X] T030 Validate quickstart.md scenarios: string format shell execution, array format exec-style, object format parallel execution

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — verify prerequisites only
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user stories
- **US1 (Phase 3)**: Depends on Phase 2 — wires typed pipeline, enables US2/US3
- **US2 (Phase 4)**: Depends on Phase 3 — replaces Exec `todo!()` stubs
- **US3 (Phase 5)**: Depends on Phase 3 — replaces Parallel `todo!()` stubs; can run in parallel with US2
- **US4 (Phase 6)**: Depends on US2 + US3 — verifies cross-phase coverage
- **Polish (Phase 7)**: Depends on all user stories complete

### User Story Dependencies

- **US1 (P1)**: Depends on Foundational — MVP; must complete before US2/US3
- **US2 (P1)**: Depends on US1 — can run in parallel with US3
- **US3 (P1)**: Depends on US1 — can run in parallel with US2
- **US4 (P2)**: Depends on US2 + US3 — integration validation

### Within Each User Story

- Core execution changes before deacon-side refactoring
- Container path before (or parallel with) host path
- Implementation before tests
- All tests pass before moving to next story

### Parallel Opportunities

- **Phase 2**: T003 (ParallelCommandResult) parallel with T002 (enum definition); T006 (tests) parallel with T004/T005
- **Phase 3**: T012 (run_user_commands.rs) parallel with T010/T011 (up/lifecycle.rs) — different files
- **Phase 4 + Phase 5**: US2 and US3 can proceed in parallel after US1 completes — they modify different code branches (`Exec` vs `Parallel` match arms)
- **Phase 6**: T025 and T026 (integration tests) can run in parallel — different test scenarios

---

## Parallel Example: User Story 2 + User Story 3

```text
# After US1 completes, these can run concurrently:

Stream A (US2 - Exec):
  T016: Add Exec branch to container execution
  T017: Add Exec branch to host execution
  T018: Add exec-style unit tests

Stream B (US3 - Parallel):
  T019: Add Parallel branch to container execution (JoinSet)
  T020: Add output attribution + progress events
  T021: Add error aggregation
  T022: Add Parallel branch to host execution
  T023: Add parallel unit tests
```

---

## Parallel Example: Phase 3 Deacon Refactoring

```text
# After T007-T009 (core execution updates), these can run concurrently:

Stream A: T010-T011 Refactor crates/deacon/src/commands/up/lifecycle.rs
Stream B: T012 Refactor crates/deacon/src/commands/run_user_commands.rs
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup verification
2. Complete Phase 2: Foundational type system (CRITICAL — blocks everything)
3. Complete Phase 3: US1 — Wire typed pipeline, preserve string behavior
4. **STOP and VALIDATE**: `make test-nextest-fast` — all existing tests must pass
5. String-format lifecycle commands work identically to before

### Incremental Delivery

1. Phase 1 + 2 → Type system and aggregation ready
2. Phase 3 (US1) → String commands work through typed pipeline → Validate (MVP!)
3. Phase 4 (US2) → Array exec-style works → Validate
4. Phase 5 (US3) → Object parallel works → Validate
5. Phase 6 (US4) → All phases verified → Validate
6. Phase 7 → Polish, lint, final gate

### Key Files Modified

| File | Phases | Changes |
|------|--------|---------|
| `crates/core/src/container_lifecycle.rs` | 2-6 | New types, aggregation, container + host execution |
| `crates/deacon/src/commands/up/lifecycle.rs` | 3 | Remove flattening, use typed pipeline |
| `crates/deacon/src/commands/run_user_commands.rs` | 3 | Use core parsing, typed commands |
| `crates/deacon/src/commands/up/tests.rs` | 3 | Remove obsolete tests |
| `crates/core/tests/integration_container_lifecycle.rs` | 3 | Update to typed API |
| `crates/core/tests/integration_non_blocking_lifecycle.rs` | 3 | Update to typed API |
| `crates/core/tests/integration_per_command_events.rs` | 3 | Update to typed API |
| `crates/core/tests/integration_mock_runtime.rs` | 3 | Update to typed API |
| `crates/core/tests/test_lifecycle_formats.rs` | 6 | NEW: Format × phase integration tests |
| `.config/nextest.toml` | 7 | Test group configuration |

---

## Notes

- [P] tasks = different files, no dependencies on incomplete tasks
- [Story] label maps task to specific user story for traceability
- US2 and US3 can be implemented in parallel after US1 completes
- Phase 2 is the hardest phase — type system changes cascade through many call sites
- `todo!()` stubs in Phase 3 allow compilation while Exec/Parallel aren't yet implemented
- Source attribution via `LifecycleCommandSource` is preserved through the entire refactoring
- Variable substitution must be applied BEFORE execution, using element-wise strategy per Decision 5
