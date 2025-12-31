---

description: "Task list for implementing the Outdated subcommand"
---

# Tasks: Outdated Subcommand Parity

**Input**: Design documents from `/specs/009-outdated-subcommand/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

**Tests**: Tests are OPTIONAL. This plan focuses on implementation tasks and independent test criteria per user story.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- Rust workspace with CLI crate `deacon` and core crate `deacon-core`
- Source paths: `crates/deacon/src/`, `crates/core/src/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Establish minimal scaffolding to host the new subcommand

- [X] T001 Create command module skeleton in crates/deacon/src/commands/outdated.rs
- [X] T002 Expose module in crates/deacon/src/commands/mod.rs

<!-- Completed on 2025-11-17 by automation: created minimal skeleton and module export -->

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core data structures and helpers required by all user stories

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T003 Create core module with data structs (FeatureVersionInfo, OutdatedResult) in crates/core/src/outdated.rs
- [ ] T004 Export new module in crates/core/src/lib.rs
- [ ] T005 Implement `canonical_feature_id(&str) -> String` in crates/core/src/outdated.rs
- [ ] T006 Implement `compute_wanted_version(...) -> Option<String>` and `wanted_major(...) -> Option<String>` per tag/digest rules in crates/core/src/outdated.rs
- [ ] T007 Implement `derive_current_version(...) -> Option<String>` using lockfile fallback in crates/core/src/outdated.rs
- [ ] T008 Implement `fetch_latest_stable_version(...) -> Option<String>` and `latest_major(...) -> Option<String>` using crates/core/src/oci.rs + crates/core/src/semver_utils.rs in crates/core/src/outdated.rs

**Checkpoint**: Core types and helpers ready; user story implementation can begin

---

## Phase 3: User Story 1 - See outdated features quickly (Priority: P1) MVP
**Goal**: Human-readable report listing Feature | Current | Wanted | Latest preserving declaration order

**Independent Test**: Run `deacon outdated --workspace-folder .` in a project with features; verify ordered table with correct Current/Wanted/Latest values. Empty config yields header/empty output with exit code 0.

### Implementation for User Story 1

- [ ] T009 [US1] Add `Outdated` subcommand and route to executor in crates/deacon/src/cli.rs
- [ ] T010 [US1] Discover effective configuration and extract features (ordered) in crates/deacon/src/commands/outdated.rs
- [ ] T011 [US1] Compute wanted version per feature using core helpers in crates/deacon/src/commands/outdated.rs
- [ ] T012 [US1] Compute current version (lockfile→wanted fallback) in crates/deacon/src/commands/outdated.rs
- [ ] T013 [US1] Lookup latest stable semver version via core in crates/deacon/src/commands/outdated.rs
- [ ] T014 [US1] Render text table `Feature | Current | Wanted | Latest` preserving config order in crates/deacon/src/commands/outdated.rs
- [ ] T015 [US1] Ensure stdout/stderr split (table to stdout; logs to stderr) in crates/deacon/src/commands/outdated.rs
- [ ] T016 [US1] Handle projects with zero features (empty report, exit 0) in crates/deacon/src/commands/outdated.rs
- [ ] T044 [US1] Implement config-not-found error path (exit 1 with clear message) in crates/deacon/src/commands/outdated.rs

**Checkpoint**: User Story 1 functional and independently testable

---

## Phase 4: User Story 2 - Machine-readable output for CI (Priority: P2)

**Goal**: Stable JSON report keyed by canonical fully-qualified feature ID without version; optional CI gating flag

**Independent Test**: Run with `--output json`; parse map keys and fields (current, wanted, latest, majors). With `--fail-on-outdated`, exit code is 2 when any outdated.

### Implementation for User Story 2

- [ ] T017 [US2] Add `--output json` support to subcommand in crates/deacon/src/cli.rs
- [ ] T018 [US2] Serialize `OutdatedResult` to JSON with nulls for unknowns in crates/deacon/src/commands/outdated.rs
- [ ] T019 [US2] Emit only JSON to stdout when `--output json`; route logs to stderr in crates/deacon/src/commands/outdated.rs
- [ ] T020 [US2] Add `--fail-on-outdated` flag to subcommand in crates/deacon/src/cli.rs
- [ ] T021 [US2] Implement outdated evaluation: `(current < wanted) OR (wanted < latest)` in crates/deacon/src/commands/outdated.rs
- [ ] T022 [US2] Return exit code 2 when any outdated and `--fail-on-outdated` is set in crates/deacon/src/commands/outdated.rs

**Checkpoint**: JSON mode and CI gating behavior complete and independently testable

---

## Phase 5: User Story 3 - Resilient and predictable behavior (Priority: P3)

**Goal**: Robust operation under network issues and invalid identifiers, with deterministic output order

**Independent Test**: Simulate registry/network errors; command completes with exit 0, setting unknown fields to null for affected features only. Non-versionable identifiers skipped or shown with nulls without failure.

### Implementation for User Story 3

- [ ] T023 [US3] Gracefully handle registry/HTTP failures by setting nulls and continuing in crates/deacon/src/commands/outdated.rs
- [ ] T024 [US3] Handle non-versionable/invalid feature references without failure in crates/deacon/src/commands/outdated.rs
- [ ] T025 [US3] Add parallel fetching of latest versions with bounded concurrency (tokio) in crates/deacon/src/commands/outdated.rs
- [ ] T026 [US3] Preserve deterministic output order regardless of parallel execution in crates/deacon/src/commands/outdated.rs
- [ ] T027 [US3] Configure sensible HTTP timeouts/retries for tag queries using core client in crates/deacon/src/commands/outdated.rs
- [ ] T028 [US3] Ensure non-interactive behavior (no TTY dependencies) and compact JSON in crates/deacon/src/commands/outdated.rs
- [ ] T029 [US3] Redact sensitive info in logs and avoid echoing credentials in crates/deacon/src/commands/outdated.rs
- [ ] T030 [US3] Verify lockfile-absent fallback path for `current` remains `wanted` in crates/deacon/src/commands/outdated.rs

**Checkpoint**: Resilience and predictability complete; behavior stable in CI and local

---

## Phase N: Polish & Cross-Cutting Concerns

**Purpose**: Final documentation, quality gates, and small refinements

- [ ] T031 [P] Update CLI help strings and examples in crates/deacon/src/cli.rs
- [ ] T032 Run cargo fmt and clippy from repository root (workspace at Cargo.toml)
- [ ] T048 Run Fast Loop locally (`make dev-fast`) after each logical group
- [ ] T049 Run Full Gate (`cargo build --verbose` && `cargo test -- --test-threads=1` && `cargo test --doc` && `cargo fmt --all -- --check` && `cargo clippy --all-targets -- -D warnings`) before PR
- [ ] T033 [P] Align JSON schema and docs with docs/subcommand-specs/outdated/DATA-STRUCTURES.md
- [ ] T034 [P] Validate quickstart commands in specs/009-outdated-subcommand/quickstart.md
- [ ] T035 Review logging fields and tracing levels across crates/deacon/src/commands/outdated.rs
- [ ] T036 Review error messages for clarity and constitution compliance in crates/deacon/src/commands/outdated.rs
- [ ] T037 [P] Add unit tests for core helpers in crates/core/src/outdated.rs
- [ ] T038 [P] [US1] Add unit tests for text rendering in crates/deacon/src/commands/outdated.rs
- [ ] T039 [P] [US1] Add CLI integration test for text output in crates/deacon/tests/integration_outdated_text.rs
- [ ] T040 [P] [US2] Add CLI integration test for JSON output in crates/deacon/tests/integration_outdated_json.rs
- [ ] T041 [P] [US2] Add CLI integration test for --fail-on-outdated exit 2 in crates/deacon/tests/integration_outdated_fail_flag.rs
- [ ] T042 [P] [US3] Add resilience test (mock registry failures) in crates/deacon/tests/integration_outdated_resilience.rs
- [ ] T043 Add doctests for public items in crates/core/src/outdated.rs
- [ ] T045 [P] Add performance validation for ~20 features with mocked registry ≤10s in crates/deacon/tests/perf_outdated.rs
- [ ] T046 Ensure tests use mocks/fakes (no network) across all new tests
- [ ] T047 Verify no unsafe blocks introduced (workspace-wide check)

---

## Dependencies & Execution Order

### Phase Dependencies

- Setup (Phase 1): No dependencies
- Foundational (Phase 2): Depends on Setup completion - BLOCKS all user stories
- User Stories (Phase 3+): Depend on Foundational completion; then proceed in priority order or in parallel by team
- Polish (Final Phase): After desired user stories complete

### User Story Dependencies

- User Story 1 (P1): No dependency on other stories; depends on foundational helpers
- User Story 2 (P2): Depends on US1 execution path and foundational helpers
- User Story 3 (P3): Depends on US1 and foundational helpers; independent of JSON mode except for shared data structures

### Within Each User Story

- Data structs/helpers (core) before CLI wiring
- Core computations before rendering
- Rendering last; maintain stdout/stderr contracts

### Parallel Opportunities

- [P]-marked tasks target different files/modules and can run concurrently
- Parallelism primarily applies to tests (T037–T042, T045) and documentation alignment (T033–T034)
- US2/US3 implementation tasks occur in the same module and are sequential unless refactored into helpers

---

## Parallel Example: User Story 1

```bash
# Run these tasks in parallel (no file conflicts):
Task: "T010 [US1] Discover effective configuration and extract features (ordered) in crates/deacon/src/commands/outdated.rs"
Task: "T011 [US1] Compute wanted version per feature using core helpers in crates/deacon/src/commands/outdated.rs"
Task: "T012 [US1] Compute current version (lockfile→wanted fallback) in crates/deacon/src/commands/outdated.rs"
Task: "T013 [US1] Lookup latest stable semver version via core in crates/deacon/src/commands/outdated.rs"
```

## Parallel Example: User Story 2

```bash
# Run these tasks in parallel (no file conflicts):
# (None) — US2 tasks occur in the same file and share dependencies
```

## Parallel Example: User Story 3

```bash
# Run these tasks in parallel (no file conflicts):
# (None) — US3 tasks occur in the same file and share dependencies
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 1
4. STOP and VALIDATE: Verify human-readable output, ordering, and exit codes
5. Demo in local project

### Incremental Delivery

1. After MVP, implement Phase 4 (JSON + CI gating)
2. Add Phase 5 (Resilience + parallel fetching)
3. Polish and documentation

### Parallel Team Strategy

- Developer A: Phase 2 helpers (core)
- Developer B: Phase 3 CLI text mode
- Developer C: Phase 4 JSON + CI gating
- Developer D: Phase 5 resilience + parallelism

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Keep logs on stderr; stdout reserved for report output
- Avoid: vague tasks, same-file conflicts, cross-story coupling that breaks independence
