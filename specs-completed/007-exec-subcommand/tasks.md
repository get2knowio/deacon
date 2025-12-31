---

description: "Tasks to implement Exec subcommand parity"
---

# Tasks: Exec Subcommand Parity

- Feature: `specs/007-exec-subcommand/`
- Required inputs: `plan.md`, `spec.md`
- Optional inputs used: `data-model.md`, `contracts/exec.openapi.yaml`, `research.md`, `quickstart.md`

Notes
- Tests are included because the spec declares mandatory testing scenarios and this repo requires green CI.
- Paths match the existing Rust workspace layout in this repo.

## Phase 1: Setup (Shared Infrastructure)

Purpose: Ensure minimal scaffolding and flags to support story work.

- [X] T001 [P] Add `--remote-env` as visible alias to `exec --env` in `crates/deacon/src/cli.rs` (accept empty values)
- [X] T002 [P] Add `--default-user-env-probe` option to `Exec` in `crates/deacon/src/cli.rs` (enum: none|loginInteractiveShell|interactiveShell|loginShell)
- [X] T003 [P] Thread global `--log-format` into `ExecArgs` (add `force_tty_if_json: bool`) in `crates/deacon/src/cli.rs` and `crates/deacon/src/commands/exec.rs`
- [X] T033 [P] Set default for `--default-user-env-probe` to `loginInteractiveShell` and document in help (in `crates/deacon/src/cli.rs`)
- [X] T039 [P] Thread Docker tooling path flags through exec: `--docker-path`, `--docker-compose-path`, `--container-data-folder`, `--container-system-data-folder`

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core utilities used by multiple stories; complete before story work.

- [X] T004 [P] Expose `ContainerProbeMode` mapping function in `crates/core/src/container_env_probe.rs` (map CLI string → enum)
- [X] T005 [P] Add helper to build effective remote env: `build_effective_env(probed, config_remoteEnv, cli_kv)` in `crates/core/src/container_env_probe.rs`
- [X] T006 Wire `exec` to accept effective env map via `ExecConfig.env` instead of raw `args.env` in `crates/deacon/src/commands/exec.rs`
- [X] T007 Ensure consistent container selection helpers are reused (by id/labels/workspace) from `deacon_core::container` in `crates/deacon/src/commands/exec.rs`
- [X] T034 [P] Unit tests for `ContainerProbeMode` mapper including default behavior and invalid inputs in `crates/core/src/container_env_probe.rs`
- [X] T037 [P] Add `resolve_effective_config(...)` in `crates/core/` to merge devcontainer config with image labels and apply variable substitution (with unit tests)

Checkpoint: Foundation ready — user stories can proceed in parallel.

---

## Phase 3: User Story 1 - Run command in intended container (Priority: P1) 🎯 MVP

Goal: Target the correct container using precedence `--container-id` > `--id-label` > `--workspace-folder`; return command output/exit code.

Independent Test: Execute via each selection method in isolation and verify stdout/exit code.

Tests (add first)
- [X] T008 [P] [US1] Integration: direct ID selection in `crates/deacon/tests/integration_exec_selection.rs`
- [X] T009 [P] [US1] Integration: label selection (`--id-label name=value`) in `crates/deacon/tests/integration_exec_selection.rs`
- [X] T010 [P] [US1] Integration: workspace discovery selection in `crates/deacon/tests/integration_exec_selection.rs`
- [X] T036 [P] [US1] Negative: missing devcontainer config for `--workspace-folder` yields exact error text with absolute path

Implementation
- [X] T011 [US1] Enforce missing selection error when none of `--container-id|--id-label|--workspace-folder` set (exact text per spec) in `crates/deacon/src/commands/exec.rs`
- [X] T012 [US1] Apply explicit precedence when multiple selectors provided in `crates/deacon/src/commands/exec.rs`
- [X] T013 [US1] Validate `--id-label` format `name=value` with clear error text in `crates/deacon/src/commands/exec.rs`
- [X] T014 [US1] Default working dir: `determine_container_working_dir()` for workspace-based; `/` for id/label in `crates/deacon/src/commands/exec.rs`
- [X] T035 [US1] Implement exact discovery error message: `Dev container config (<path>) not found.` (path must be absolute)

Checkpoint: US1 independently runnable and verifiable per quickstart.

---

## Phase 4: User Story 2 - Environment matches expectations (Priority: P1)

Goal: Compute environment using probe → config remoteEnv → CLI `--remote-env` (CLI wins) and inject for exec.

Independent Test: Verify PATH reflects shell init; CLI variables present and override config values; empty values preserved.

Tests (add first)
- [X] T015 [P] [US2] Unit: merge precedence table tests in `crates/core/src/container_env_probe.rs` (CLI wins over config; config over probe)
- [X] T016 [P] [US2] Integration: `--remote-env FOO=bar` and `FOO=` empty value in `crates/deacon/tests/integration_exec_env.rs`

Implementation
- [X] T017 [US2] Map `--default-user-env-probe` to `ContainerProbeMode` in `crates/deacon/src/commands/exec.rs`
- [X] T018 [US2] Probe container env using `ContainerEnvironmentProber::probe_container_environment` in `crates/deacon/src/commands/exec.rs`
- [X] T019 [US2] Load config `remoteEnv` and merge with probed + CLI via `build_effective_env` in `crates/deacon/src/commands/exec.rs`
- [X] T020 [US2] Inject merged env into `ExecConfig.env` (preserve empty values) in `crates/deacon/src/commands/exec.rs`
- [X] T038 [US2] Use `resolve_effective_config(...)` for env/user inputs; ensure CLI `--remote-env` overrides config

Checkpoint: US2 independently verifiable via `env` inside container.

---

## Phase 5: User Story 3 - Interactive and non-interactive usage (Priority: P2)

Goal: Allocate PTY when TTY or when `--log-format json` is set; allow terminal size hints.

Independent Test: `tty` reports presence in PTY mode; size flags reflected where applicable; non-PTY preserves separate streams.

Tests (add first)
- [X] T021 [P] [US3] Unit: PTY decision logic (TTY detected or force when JSON) in `crates/deacon/src/commands/exec.rs` tests
- [X] T022 [P] [US3] Integration: non-TTY run preserves exit and streams in `crates/deacon/tests/integration_exec_pty.rs`

Implementation
- [X] T023 [US3] Force PTY when `force_tty_if_json` true; otherwise require stdin/stdout TTY and not `--no-tty` in `crates/deacon/src/commands/exec.rs`
- [X] T024 [US3] Thread `terminal_columns/rows` into tracing and future PTY sizing (document limitation of Docker exec) in `crates/deacon/src/commands/exec.rs`

Checkpoint: US3 independently verifiable in TTY and non-TTY sessions.

---

## Phase 6: User Story 4 - Clear errors and exit codes (Priority: P2)

Goal: Deterministic error messages and exit code mapping (propagate code; signal → 128+N).

Independent Test: Invalid flags, missing selection, and simulated signal termination yield specified messages/codes.

Tests (add first)
- [X] T025 [P] [US4] Unit: exact error text assertions for invalid `--id-label`/missing selection in `crates/deacon/src/commands/exec.rs` tests
- [X] T026 [P] [US4] Integration: exit code propagation including a signal-based termination case in `crates/deacon/tests/integration_exec_exit.rs`
- [X] T042 [P] [US4] Integration: JSON log mode preserves stdout/stderr contract (stdout reserved, logs to stderr)

Implementation
- [X] T027 [US4] Normalize and surface exact error strings per spec in `crates/deacon/src/commands/exec.rs`
- [X] T028 [US4] Ensure exit code from Docker is returned verbatim; retain 128+signal behavior in `crates/deacon/src/commands/exec.rs`
- [X] T041 [US4] Ensure global `--log-level` is honored and does not alter stdout/stderr contract for `exec`
- [X] T043 [US4] Implement precedence: CLI `--user` overrides config `remoteUser` for execution
- [X] T044 [US4] Add integration test asserting `--user` precedence (e.g., `whoami` inside container)

Checkpoint: US4 independently verifiable via negative scenarios.

---

## Phase N: Polish & Cross-Cutting Concerns

- [X] T029 [P] Update CLI help text for `exec` flags in `crates/deacon/src/cli.rs`
- [X] T030 [P] Extend `examples/exec/` quickstart with env/PTY examples in `examples/exec/README.md`
- [X] T031 Format, clippy, and doctests green (`make release-check`) across workspace
- [X] T032 [P] Update `docs/subcommand-specs/exec/SPEC.md` notes on PTY sizing limits (Docker exec)
- [X] T040 [P] Add parsing/wiring test for Docker tooling path flags (no external binary dependency)

---

## Dependencies & Execution Order

- Setup (Phase 1): none
- Foundational (Phase 2): depends on Setup; blocks all user stories
- User Stories: then proceed in priority order or parallel after Phase 2
  - US1 (P1) → MVP; independent of US2–US4
  - US2 (P1) independent; uses Phase 2 helpers
  - US3 (P2) independent; uses Phase 2 helpers
  - US4 (P2) independent; uses Phase 2 helpers
- Polish: after desired stories complete

## Parallel Examples

US1
- T008, T009, T010 can run in parallel (separate tests)
- T011–T014 follow after tests exist

US2
- T015 and T016 in parallel (unit vs integration)
- T017–T020 sequential after tests

US3
- T021 and T022 in parallel
- T023–T024 sequential after tests

US4
- T025 and T026 in parallel
- T027–T028 sequential after tests

## Implementation Strategy

MVP First (US1 only)
1) Complete Phase 1–2
2) Implement US1 selection + error handling
3) Validate with integration tests

Incremental Delivery
- Add US2 (env merge), then US3 (PTY), then US4 (errors/exit mapping)

## Report Summary

- Total tasks: 32
- Tasks per story: US1: 7 (T008–T014), US2: 6 (T015–T020), US3: 4 (T021–T024), US4: 9 (T025–T044)

Note: Phase 6 (US4) tasks have been completed and checked off.
- Parallel opportunities: Tests in each story; some foundational tasks marked [P]
- Independent test criteria:
  - US1: Selection by ID/label/workspace; correct stdout/exit code
  - US2: PATH from shell; CLI env overrides config; empty values preserved
  - US3: PTY present in TTY contexts or forced by JSON; size hints recorded
  - US4: Exact error strings; exit code mapping including signals
- Suggested MVP scope: Phase 1–2 + Phase 3 (US1)
- Format validation: All tasks follow `- [ ] T### [P?] [US?] Description with file path` where applicable
