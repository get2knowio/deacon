# Tasks: Test Parallelization with cargo-nextest

**Input**: Design documents from `/specs/001-nextest-parallel-tests/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Only include when explicitly specified (not requested in this feature).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: User story label (US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare repository structure for nextest tooling artifacts.

- [X] T001 Create helper scripts directory scaffold in scripts/nextest/README.md
- [X] T002 [P] Track timing artifacts directory by adding artifacts/nextest/.gitkeep
- [X] T003 [P] Allow committing timing data by updating .gitignore with !artifacts/nextest/

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core tooling required before any nextest-enabled workflow.

- [X] T004 Implement cargo-nextest preflight script in scripts/nextest/assert-installed.sh
- [X] T005 [P] Author reusable timing capture helper in scripts/nextest/capture-timing.sh
- [X] T006 Establish base .config/nextest.toml with declared test-groups and shared metadata

**Checkpoint**: Nextest scripts and base configuration exist; user story work can begin.

---

## Phase 3: User Story 1 – Local developer speeds up feedback loop (Priority: P1) 🎯 MVP

**Goal**: Provide fast parallel test commands for local contributors with clear fallback behavior.

**Independent Test**: Run `make test-nextest-fast` on a workstation and confirm the suite finishes faster than the serial baseline while reporting applied groups.

### Implementation

- [X] T007 [US1] Populate .config/nextest.toml selectors for docker-exclusive, docker-shared, fs-heavy, unit-default, smoke, and parity groups
- [X] T008 [US1] Define dev-fast and full profiles in .config/nextest.toml with appropriate filters and overrides
- [X] T009 [US1] Add make target test-nextest-fast in Makefile invoking scripts/nextest/assert-installed.sh and cargo nextest run --profile dev-fast
- [X] T010 [US1] Add make target test-nextest in Makefile for the full profile while preserving serial fallback guidance
- [X] T011 [US1] Wire timing capture into local targets via scripts/nextest/capture-timing.sh to write artifacts/nextest/dev-fast-timing.json and full-timing.json
- [X] T012 [US1] Update README.md development workflow section with cargo-nextest usage and fallback instructions

**Checkpoint**: Local developers can run fast and full nextest profiles with automated timing capture.

---

## Phase 4: User Story 2 – CI pipeline keeps deterministic outcomes (Priority: P2)

**Goal**: Run the full suite in CI with controlled concurrency, artifact capture, and clear failure signals.

**Independent Test**: Trigger the CI workflow on a feature branch and confirm the nextest-based job completes using the conservative profile and uploads timing data.

### Implementation

- [X] T013 [US2] Extend .config/nextest.toml with ci profile overrides that enforce serial smoke/parity execution and conservative concurrency
- [X] T014 [US2] Add make target test-nextest-ci in Makefile invoking cargo nextest run --profile ci with JSON reporter output to artifacts/nextest/ci-timing.json
- [X] T015 [US2] Update .github/workflows/ci.yml to install cargo-nextest via taiki-e/install-action@v2
- [X] T016 [US2] Update .github/workflows/ci.yml to run make test-nextest-ci and upload artifacts/nextest/ci-timing.json
- [X] T017 [US2] Update .github/workflows/ci-other-os.yml to reuse make test-nextest-ci and publish timing artifacts
- [X] T018 [US2] Update .github/workflows/build-macos.yml to reuse make test-nextest-ci and publish timing artifacts
- [X] T019 [US2] Emit runtime comparison summary for SC-007 in scripts/nextest/capture-timing.sh or the CI workflow job summary

**Checkpoint**: CI executes nextest with deterministic concurrency and exposes timing metrics.

---

## Phase 5: User Story 3 – Maintainer categorizes new tests (Priority: P3)

**Goal**: Provide guidance and tooling so maintainers can classify tests into the right concurrency groups.

**Independent Test**: Follow the documentation to classify a new filesystem-heavy integration test, update .config/nextest.toml, and verify assignment via the audit command.

### Implementation

- [X] T020 [US3] Create docs/testing/nextest.md with classification workflow, group definitions, and remediation steps
- [X] T021 [US3] Link new guide from docs/CLI_PARITY.md testing section and describe Docker-exclusive remediation steps
- [X] T022 [US3] Add make target test-nextest-audit in Makefile to run cargo nextest list --status --profile full
- [X] T023 [US3] Update scripts/nextest/assert-installed.sh messaging to reference installation docs and docs/testing/nextest.md
- [X] T024 [US3] Update README.md troubleshooting/testing sections with classification checklist and audit target usage

**Checkpoint**: Maintainers can categorize and audit tests confidently using documented procedures.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Finalize documentation and developer experience across stories.

- [X] T025 Document timing artifact interpretation and baseline comparison process in artifacts/nextest/README.md
- [X] T026 Update Makefile help output to list new nextest targets for discoverability

---

## Dependencies & Execution Order

- **Setup (Phase 1)** → Must finish before scripts/configuration work.
- **Foundational (Phase 2)** → Depends on Phase 1; blocks all user stories.
- **User Stories** → Execute in priority order (US1 → US2 → US3) after foundational tasks. Stories are independently testable once their phase completes.
- **Polish (Phase 6)** → Runs after desired user stories are complete.

Within each user story:
- Update .config/nextest.toml before Makefile targets.
- Makefile targets should exist before CI and documentation changes that reference them.

## Parallel Opportunities

- Phase 1 tasks T002 and T003 can proceed in parallel after T001.
- Phase 2 task T005 can proceed alongside T004 once scripts/nextest/ exists.
- After Phase 2, separate user stories can be staffed in parallel as long as shared files are coordinated.
- During US2, workflow updates to ci-other-os and build-macos (T017–T018) can proceed in parallel once T014–T016 define the shared target.

## Implementation Strategy

1. Complete Phases 1–2 to establish reusable scripts and base configuration.
2. Deliver **MVP (US1)** to unlock faster local feedback; validate against the independent test.
3. Extend to **US2** to integrate CI concurrency and timing instrumentation.
4. Finalize **US3** documentation and audit tooling so the suite remains maintainable.
5. Finish Polish tasks to keep artifacts interpretable and commands discoverable.
