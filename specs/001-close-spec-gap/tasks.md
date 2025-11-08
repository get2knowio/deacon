---

description: "Executable task list for implementing 'Close Spec Gap (Features Plan)'"
---

# Tasks: Close Spec Gap (Features Plan)

Input: Design documents from `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/`
Prerequisites: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

Tests: Only include where explicitly listed by the user stories or FRs; otherwise focus on implementation with minimal unit tests that prove behavior.

Organization: Tasks are grouped by user story to enable independent implementation and testing of each story.

Notes:
- All file paths below are absolute.
- Checklist format strictly follows `- [ ] T### [P?] [US#?] Description with file path`.

## Phase 1: Setup (Shared Infrastructure)

Purpose: Align CLI help/docs and observability with the feature spec. No new crates/deps required.

- [X] T001 [P] Add planning disclaimers to CLI help for `features plan` in `/workspaces/001-features-plan-cmd/crates/deacon/src/cli.rs` (variable substitution not performed; feature IDs are opaque; options pass through).
- [X] T002 [P] Add spec references in module docs for planner in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (link to `/workspaces/001-features-plan-cmd/docs/subcommand-specs/features-plan/SPEC.md` and DATA-STRUCTURES.md).
- [X] T003 [P] Add a short "Schema" note in `/workspaces/001-features-plan-cmd/docs/subcommand-specs/features-plan/SPEC.md` referencing `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/contracts/plan.schema.json` (no behavior change).

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core behaviors needed by all user stories (canonical IDs, precedence, error taxonomy wiring).

- [X] T004 Implement `canonicalize_feature_id(&str) -> String` (trim only) in `/workspaces/001-features-plan-cmd/crates/core/src/features.rs` and export for reuse.
- [X] T005 [P] Apply canonicalization to feature IDs in planner before resolution in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (rekey merged map with trimmed keys; dedupe on collision by keeping last writer).
- [X] T006 [P] Set CLI-precedence for merge in planner by passing `prefer_cli_features = true` to `FeatureMergeConfig::new` in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (FR-006).
- [X] T007 [P] Map OCI fetch failures to categorized FeatureError variants (Authentication, Download, Oci)
- [X] T008 [P] Unit tests for canonicalization + precedence in `/workspaces/001-features-plan-cmd/crates/core/src/features.rs` (trim behavior; merge with CLI precedence replacing objects/arrays wholesale per FR-006).

Checkpoint: Foundation ready — user stories can proceed.

---

## Phase 3: User Story 1 — Clear input validation for additional features (Priority: P1) 🎯 MVP

Goal: When `--additional-features` is provided, validate it is a JSON object; invalid inputs error clearly and stop before planning.

Independent Test: Run `features plan` with `--additional-features` set to non-object JSON (array/string/number) and to invalid JSON; expect a single descriptive error mentioning the flag and expected format; no plan output.

### Implementation for User Story 1

- [X] T009 [US1] Ensure error message exactly references the flag and expected type in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (keep parse early; message format: "--additional-features must be a JSON object").
- [X] T010 [P] [US1] Add unit tests for invalid JSON and non-object cases in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (tokio tests around `execute_features_plan`).
- [X] T011 [US1] Add a short note to CLI help for the flag in `/workspaces/001-features-plan-cmd/crates/deacon/src/cli.rs` clarifying accepted type (JSON object) and examples.

Checkpoint: US1 independently testable via CLI without any other story.

---

## Phase 4: User Story 2 — Explicit rejection of local feature paths (Priority: P2)

Goal: If any feature ID looks like a local path (./, ../, /, or Windows drive), fail fast with guidance to use registry references.

Independent Test: Provide a local path key via config OR `--additional-features` and verify planner exits with a clear message including the offending key and guidance.

### Implementation for User Story 2

- [X] T012 [US2] Ensure pre-validation covers both config features and merged CLI additions in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (iterate keys before any fetches; keep existing `is_local_path`, extend message to include guidance per FR-002).
- [X] T013 [P] [US2] Add tests for: (a) local-only, (b) mixed local+registry, (c) local in `--additional-features` in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs`.

Checkpoint: US2 independently testable with just invalid inputs, no registry needed.

---

## Phase 5: User Story 3 — Deterministic order and complete graph output (Priority: P3)

Goal: Produce deterministic `order` and `graph` using direct dependencies = union(installsAfter, dependsOn), deduped and lexicographically sorted; tie-break independents lexicographically by canonical ID.

Independent Test: Provide synthetic features (no network) and verify consistent `order` across runs; graph lists direct dependencies only with stable, sorted arrays.

### Implementation for User Story 3

- [X] T014: Implement graph deduplication and sorting using BTreeSet
- [X] T015: Add unit tests for graph determinism requirements
- [X] T016: Implement topological sort with lexicographic tie-breakers

Checkpoint: US3 outputs deterministically with stable graph ordering.

---

## Phase N: Polish & Cross-Cutting Concerns

- [X] T017 [P] Update feature docs with behavior notes (validation, local-path rejection, determinism) in `/workspaces/001-features-plan-cmd/docs/subcommand-specs/features-plan/SPEC.md` and `/workspaces/001-features-plan-cmd/docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`.
- [X] T018 [P] Add tracing fields and spans consistency (features.fetch_metadata, features.resolve_dependencies) with documented names in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs`.
- [X] T019 [P] Ensure redaction policies avoid logging feature option values in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (audit debug! lines and update to structured fields without secrets).
- [X] T020 Run quickstart validation commands described in `/workspaces/001-features-plan-cmd/specs/001-close-spec-gap/quickstart.md` and capture expected outputs in comments within that file.

---

## Phase 6: Error Handling and Edge Cases (FR-005, FR-010, FR-011)

Purpose: Close coverage gaps for cycle detection/reporting, option type pass-through, categorized registry failures, and empty input behavior.

- [X] T021 [FR-005] Implement and/or surface dependency cycle detection with human-readable cycle participants; add tests in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs` (unit) and `/workspaces/001-features-plan-cmd/crates/deacon/tests/` (integration if applicable). Ensure no partial plan is emitted.
- [X] T022 [FR-010] Add unit tests proving planner passes through option value types unchanged (object, array, number, boolean, string) in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs`.
- [X] T023 [FR-011] Add tests that categorize registry fetch failures (401/403 auth, 404 not found, transient network) with distinct messages and a single fatal error (no partial plan) in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs`.
- [X] T024 [Edge] Add test for empty features map producing `{ order: [], graph: {} }` with no warnings in `/workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs`.

Checkpoint: Error handling and edge cases covered; FR-005, FR-010, FR-011 now test-backed.

---

## Dependencies & Execution Order

Phase dependencies:
- Setup → Foundational → User Stories (US1, US2, US3 can proceed after Foundational)
- Polish after desired stories complete

User story dependencies:
- US1 (P1): none besides Foundational
- US2 (P2): none besides Foundational (independent of US1)
- US3 (P3): none besides Foundational (independent of US1/US2)

Within each story:
- Write or extend minimal unit tests first where listed
- Keep changes small and verify build green (fmt, clippy, tests)

### Parallel Opportunities

- T001, T002, T003 can run in parallel
- In Foundational: T004, T006, T007, T008 can run in parallel (T005 depends on T004)
- Story phases: US1, US2, and US3 can be developed in parallel after Foundational
- Within US3 tests (T015) can be written in parallel with comment-only T014

---

## Parallel Example: User Story 1

- Launch in parallel:
  - Task: "T010 [P] [US1] Add unit tests for invalid JSON and non-object cases in /workspaces/001-features-plan-cmd/crates/deacon/src/commands/features.rs"
  - Task: "T011 [US1] Add a short note to CLI help for the flag in /workspaces/001-features-plan-cmd/crates/deacon/src/cli.rs"

---

## Implementation Strategy

MVP First (US1 only):
1) Complete Foundational tasks T004–T008
2) Complete US1 tasks T009–T011
3) Validate independently by running `features plan` with bad `--additional-features` inputs

Incremental Delivery:
- Add US2 (local path rejection) → demonstrate with invalid inputs
- Add US3 (determinism + graph) → demonstrate with synthetic features and snapshot tests

---

## Format Validation

- All tasks follow the required checklist format with Task IDs, [P] parallel marker where applicable, and [US#] labels for story phases.

