---

description: "Executable task list for implementing 'Close Spec Gap (Features Plan)'"
---

# Tasks: Close Spec Gap (Features Plan)

Input: Design documents from `/workspaces/deacon/specs/001-close-spec-gap/`
Prerequisites: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

Tests: Only include where explicitly listed by the user stories or FRs; otherwise focus on implementation with minimal unit tests that prove behavior.

Organization: Tasks are grouped by user story to enable independent implementation and testing of each story.

Notes:
- All file paths below are absolute.
- Checklist format strictly follows `- [ ] T### [P?] [US#?] Description with file path`.

## Phase 1: Setup (Shared Infrastructure)

Purpose: Align CLI help/docs and observability with the feature spec. No new crates/deps required.

- [ ] T001 [P] Add planning disclaimers to CLI help for `features plan` in `/workspaces/deacon/crates/deacon/src/cli.rs` (variable substitution not performed; feature IDs are opaque; options pass through).
- [ ] T002 [P] Add spec references in module docs for planner in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (link to `/workspaces/deacon/docs/subcommand-specs/features-plan/SPEC.md` and DATA-STRUCTURES.md).
- [ ] T003 [P] Add a short "Schema" note in `/workspaces/deacon/docs/subcommand-specs/features-plan/SPEC.md` referencing `/workspaces/deacon/specs/001-close-spec-gap/contracts/plan.schema.json` (no behavior change).

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core behaviors needed by all user stories (canonical IDs, precedence, error taxonomy wiring).

- [ ] T004 Implement `canonicalize_feature_id(&str) -> String` (trim only) in `/workspaces/deacon/crates/core/src/features.rs` and export for reuse.
- [ ] T005 [P] Apply canonicalization to feature IDs in planner before resolution in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (rekey merged map with trimmed keys; dedupe on collision by keeping last writer).
- [ ] T006 Set CLI-precedence for merge in planner by passing `prefer_cli_features = true` to `FeatureMergeConfig::new` in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (FR-006).
- [ ] T007 [P] Map OCI fetch failures to categorized messages (401/403 auth, 404 not found, network) in `/workspaces/deacon/crates/deacon/src/commands/features.rs` using error variants from `/workspaces/deacon/crates/core/src/errors.rs` (FR-011).
- [ ] T008 [P] Unit tests for canonicalization + precedence in `/workspaces/deacon/crates/core/src/features.rs` (trim behavior; merge with CLI precedence replacing objects/arrays wholesale per FR-006).

Checkpoint: Foundation ready — user stories can proceed.

---

## Phase 3: User Story 1 — Clear input validation for additional features (Priority: P1) 🎯 MVP

Goal: When `--additional-features` is provided, validate it is a JSON object; invalid inputs error clearly and stop before planning.

Independent Test: Run `features plan` with `--additional-features` set to non-object JSON (array/string/number) and to invalid JSON; expect a single descriptive error mentioning the flag and expected format; no plan output.

### Implementation for User Story 1

- [ ] T009 [US1] Ensure error message exactly references the flag and expected type in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (keep parse early; message format: "--additional-features must be a JSON object").
- [ ] T010 [P] [US1] Add unit tests for invalid JSON and non-object cases in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (tokio tests around `execute_features_plan`).
- [ ] T011 [US1] Add a short note to CLI help for the flag in `/workspaces/deacon/crates/deacon/src/cli.rs` clarifying accepted type (JSON object) and examples.

Checkpoint: US1 independently testable via CLI without any other story.

---

## Phase 4: User Story 2 — Explicit rejection of local feature paths (Priority: P2)

Goal: If any feature ID looks like a local path (./, ../, /, or Windows drive), fail fast with guidance to use registry references.

Independent Test: Provide a local path key via config OR `--additional-features` and verify planner exits with a clear message including the offending key and guidance.

### Implementation for User Story 2

- [ ] T012 [US2] Ensure pre-validation covers both config features and merged CLI additions in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (iterate keys before any fetches; keep existing `is_local_path`, extend message to include guidance per FR-002).
- [ ] T013 [P] [US2] Add tests for: (a) local-only, (b) mixed local+registry, (c) local in `--additional-features` in `/workspaces/deacon/crates/deacon/src/commands/features.rs`.

Checkpoint: US2 independently testable with just invalid inputs, no registry needed.

---

## Phase 5: User Story 3 — Deterministic order and complete graph output (Priority: P3)

Goal: Produce deterministic `order` and `graph` using direct dependencies = union(installsAfter, dependsOn), deduped and lexicographically sorted; tie-break independents lexicographically by canonical ID.

Independent Test: Provide synthetic features (no network) and verify consistent `order` across runs; graph lists direct dependencies only with stable, sorted arrays.

### Implementation for User Story 3

- [ ] T014 [P] [US3] Ensure adjacency arrays in graph builder are deduped and lexicographically sorted (BTreeSet ok) in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (verify already true; keep task to align names and add comments citing SPEC).
- [ ] T015 [P] [US3] Add unit tests covering: simple chain, fan-in union, duplicate deduplication, and deterministic ordering across runs in `/workspaces/deacon/crates/deacon/src/commands/features.rs`.
- [ ] T016 [US3] Ensure topological sort uses lexicographic tie-breakers for zero in-degree and neighbor processing in `/workspaces/deacon/crates/core/src/features.rs` (verify; add or adjust comments/tests if needed).

Checkpoint: US3 outputs deterministically with stable graph ordering.

---

## Phase N: Polish & Cross-Cutting Concerns

- [ ] T017 [P] Update feature docs with behavior notes (validation, local-path rejection, determinism) in `/workspaces/deacon/docs/subcommand-specs/features-plan/SPEC.md` and `/workspaces/deacon/docs/subcommand-specs/features-plan/DATA-STRUCTURES.md`.
- [ ] T018 [P] Add tracing fields and spans consistency (features.fetch_metadata, features.resolve_dependencies) with documented names in `/workspaces/deacon/crates/deacon/src/commands/features.rs`.
- [ ] T019 [P] Ensure redaction policies avoid logging feature option values in `/workspaces/deacon/crates/deacon/src/commands/features.rs` (audit debug! lines and update to structured fields without secrets).
- [ ] T020 Run quickstart validation commands described in `/workspaces/deacon/specs/001-close-spec-gap/quickstart.md` and capture expected outputs in comments within that file.

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
  - Task: "T010 [P] [US1] Add unit tests for invalid JSON and non-object cases in /workspaces/deacon/crates/deacon/src/commands/features.rs"
  - Task: "T011 [US1] Add a short note to CLI help for the flag in /workspaces/deacon/crates/deacon/src/cli.rs"

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

