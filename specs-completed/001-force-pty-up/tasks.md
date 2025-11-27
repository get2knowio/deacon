# Tasks: Force PTY toggle for up lifecycle

**Input**: Design documents from `/specs/001-force-pty-up/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Integration tests are requested in the quickstart; include nextest group updates for new binaries.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Ensure design alignment before implementation

- [x] T001 Confirm spec/plan alignment for PTY toggle inputs in specs/001-force-pty-up/spec.md and specs/001-force-pty-up/plan.md

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Baseline understanding and hooks before story work

- [x] T002 Review current lifecycle exec and PTY/log handling in crates/deacon/src/commands/up.rs and crates/deacon/src/commands/exec.rs
- [x] T003 [P] Identify JSON log mode detection and stdout/stderr routing hooks in crates/deacon/src/commands/up.rs and crates/deacon/src/runtime_utils.rs
- [x] T004 [P] Inspect existing PTY-related integration coverage to align fixtures in crates/deacon/tests/integration_exec_pty.rs and crates/deacon/tests/parity_up_exec.rs

---

## Phase 3: User Story 1 - Force PTY when JSON logs are requested (Priority: P1) 🎯 MVP

**Goal**: When `--json` is active and flag/env toggles PTY, lifecycle exec runs under PTY while preserving JSON log/stderr separation.

**Independent Test**: Run `deacon up --json` with `--force-tty-if-json` or `DEACON_FORCE_TTY_IF_JSON=true`; verify lifecycle exec is PTY-backed and JSON output purity holds.

### Tests for User Story 1

- [x] T005 [P] [US1] Add integration for PTY-on with flag/env in crates/deacon/tests/integration_up_force_tty_if_json.rs
- [x] T005a [P] [US1] Add integration asserting stdout JSON purity and stderr-only logs under PTY in crates/deacon/tests/integration_up_force_tty_if_json.rs
- [x] T005b [P] [US1] Add integration covering PTY allocation failure path (clear error, no silent downgrade) in crates/deacon/tests/integration_up_force_tty_if_json.rs
- [x] T005c [P] [US1] Integration verifying flag overrides env (flag on + env false) in crates/deacon/tests/integration_up_force_tty_if_json.rs
- [x] T006 [P] [US1] Configure nextest group for new integration binary in .config/nextest.toml (docker-shared unless isolation requires docker-exclusive)

### Implementation for User Story 1

- [x] T007 [US1] Implement PTY preference resolution (flag > env `DEACON_FORCE_TTY_IF_JSON` truthy parsing > default) scoped to JSON log mode in crates/deacon/src/commands/up.rs
- [x] T008 [P] [US1] Apply resolved PTY to lifecycle exec invocations while preserving stdout/stderr separation in crates/deacon/src/commands/up.rs
- [x] T008a [P] [US1] Ensure PTY execution preserves stdout/stderr separation in crates/deacon/src/commands/up.rs
- [x] T008b [US1] Surface explicit error when PTY allocation fails (no silent downgrade) in crates/deacon/src/commands/up.rs
- [x] T009 [P] [US1] Document flag/env PTY toggle in crates/deacon/src/cli.rs help text; update docs/CLI-SPEC.md if contract requires user-facing doc change

**Checkpoint**: User Story 1 independently delivers PTY-on behavior in JSON mode with structured logs intact.

---

## Phase 4: User Story 2 - Preserve non-PTY default when not requested (Priority: P2)

**Goal**: Without flag/env, lifecycle exec stays non-PTY even in JSON mode; env falsey/unset honors default.

**Independent Test**: Run `deacon up --json` with toggle unset/false; confirm non-PTY execution and unchanged log channel split.

### Tests for User Story 2

- [x] T010 [P] [US2] Add integration asserting non-PTY default when flag/env absent or false in crates/deacon/tests/integration_up_force_tty_if_json.rs
- [x] T011 [P] [US2] Add integration for non-JSON mode ignoring PTY toggle in crates/deacon/tests/integration_up_force_tty_if_json.rs

### Implementation for User Story 2

- [x] T012 [US2] Guard default non-PTY path (including env falsey/unset) in lifecycle exec resolution in crates/deacon/src/commands/up.rs

**Checkpoint**: User Story 2 independently proves defaults are unchanged when PTY not requested.

---

## Phase 5: User Story 3 - No regression to other exec paths (Priority: P3)

**Goal**: Exec entry points outside `up` retain their existing TTY/log/exit behaviors regardless of PTY toggle presence.

**Independent Test**: Run `deacon exec` with/without PTY toggles; behavior matches pre-change expectations.

### Tests for User Story 3

- [x] T013 [P] [US3] Add regression coverage for exec PTY behavior unaffected by toggle in crates/deacon/tests/integration_exec_pty.rs

### Implementation for User Story 3

- [x] T014 [US3] Scope PTY toggle logic to up lifecycle only, leaving exec command defaults and exit codes unchanged in crates/deacon/src/commands/exec.rs

**Checkpoint**: User Story 3 independently validates no regressions to exec paths.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T015 [P] Update quickstart and spec references to reflect final PTY toggle behavior in specs/001-force-pty-up/quickstart.md and note if docs/CLI-SPEC.md change is required or not
- [x] T016 Validate formatting/lint/tests per Constitution (cargo fmt --all, cargo fmt --all -- --check, cargo clippy --all-targets -- -D warnings, targeted make test-nextest-* with new nextest group updates) after changes in workspace root

---

## Dependencies & Execution Order

- Phase dependencies: Setup → Foundational → US1 → US2 → US3 → Polish
- User story order: US1 (P1) before US2 (P2) before US3 (P3); US2/US3 can start after Foundational if US1 interfaces are stable.
- Within-story dependencies:
  - US1: Tests T005/T006 can start after Foundational; implementation T007 precedes T008; docs T009 after behavior is set.
  - US2: Tests T010/T011 depend on US1 plumbing; implementation T012 after US1 resolution logic.
  - US3: Tests T013 and implementation T014 after US1 baseline to ensure scoping is correct.

## Parallel Execution Examples

- Run T003 and T004 in parallel (different files, analysis only).
- In US1, T005 and T006 can proceed in parallel; T008 and T009 can run in parallel after T007 resolves preference logic.
- In US2, T010 and T011 can run in parallel once US1 logic is in place.
- Polish tasks T015 and T016 can run in parallel after all stories finish.

## Implementation Strategy

- MVP = Complete US1 (Phase 3) to deliver PTY-on behavior in JSON mode with correct logging and env/flag precedence.
- Build incrementally: finalize US1, then lock defaults with US2, then validate non-up exec regression with US3, ending with polish.
- Maintain test cadence with targeted `make test-nextest-fast`; expand to full `make test-nextest` before PR if required by changes.
