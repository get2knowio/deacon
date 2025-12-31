---

description: "Tasks to reach spec parity for read-configuration"
---

# Tasks: Read-Configuration Spec Parity

Input: Design documents from `/specs/001-read-config-parity/`
Prerequisites: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

Tests: Tests are OPTIONAL in this list. The repo prefers tests for new logic; minimal tests are included where they add clarity.

Organization: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- [P]: Can run in parallel (different files, no dependencies)
- [Story]: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

Purpose: Ensure flags, logging, and scaffolding align with the spec and repo rules

- [X] T001 [P] Audit and update CLI flag help text for read-configuration in `crates/deacon/src/cli.rs` (include --include-merged-configuration, --include-features-configuration, --container-id, --id-label, --terminal-rows/columns pairing, --user-data-folder notes)
- [X] T002 [P] Add spec reference comment header to `crates/deacon/src/commands/read_configuration.rs` pointing to `docs/subcommand-specs/read-configuration/SPEC.md` and `specs/001-read-config-parity/spec.md`
- [X] T003 [P] Confirm logging writes to stderr only in both text and JSON modes in `crates/core/src/logging.rs`; add a brief module doc note about stdout/stderr separation
- [X] T004 [P] Create integration test scaffold `crates/deacon/tests/integration_read_configuration.rs` with helper to run the command and parse stdout JSON safely

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core validation and output contract must be solid before story work

- [X] T005 Enforce single-JSON-to-stdout for read-configuration by routing all prints via `deacon_core::io::Output` in `crates/deacon/src/commands/read_configuration.rs` (verify no stray println! or eprintln! on success path)
- [X] T006 [P] Align selector requirement error message to spec wording in `crates/deacon/src/commands/read_configuration.rs` (FR-001)
- [X] T007 [P] Validate `--id-label` format with `<name>=<value>` and precise error text in `crates/core/src/container.rs::ContainerSelector::parse_labels` (FR-002)
- [X] T008 [P] Enforce terminal dimension pairing and positive values in `crates/deacon/src/cli.rs` and `crates/deacon/src/commands/read_configuration.rs` (FR-003)
- [X] T009 Compute `${devcontainerId}` deterministically from sorted labels (order-insensitive) using `compute_dev_container_id` in `crates/core/src/container.rs`; ensure call sites set it pre-container in `crates/deacon/src/commands/read_configuration.rs` (FR-005)
- [X] T010 [P] Set `containerWorkspaceFolder` and `${containerEnv:*}` substitution context when a container is selected in `crates/deacon/src/commands/read_configuration.rs` (FR-006)
- [X] T011 [P] Confirm stdout contract fields and omissions: always `configuration`, optional `featuresConfiguration`, optional `mergedConfiguration` in `crates/deacon/src/commands/read_configuration.rs` (FR-007/FR-008/FR-009/FR-010)

Checkpoint: Foundation ready — user story implementation can now begin

---

## Phase 3: User Story 1 - Emit Spec-Compliant JSON (Priority: P1) 🎯 MVP

Goal: Resolve configuration and emit a spec-compliant single JSON document to stdout; logs only to stderr; include optional sections only when requested

Independent Test: Run with `--workspace-folder` only, then add `--include-merged-configuration`, and `--include-features-configuration`; verify fields and strict stdout/stderr separation

### Tests for User Story 1 (OPTIONAL)

- [X] T012 [P] [US1] Add acceptance test: stdout contains only `{ configuration: ... }` when run with `--workspace-folder` in `crates/deacon/tests/integration_read_configuration.rs`
- [X] T013 [P] [US1] Add acceptance test: stdout contains `configuration` + `mergedConfiguration` when `--include-merged-configuration` is provided in `crates/deacon/tests/integration_read_configuration.rs`

### Implementation for User Story 1

- [X] T014 [US1] Ensure `ReadConfigurationOutput` omits absent sections and serializes with camelCase in `crates/deacon/src/commands/read_configuration.rs`
- [X] T015 [US1] Verify `Output::write_json` is the only stdout writer and logs use tracing (stderr) in `crates/deacon/src/commands/read_configuration.rs`
- [X] T016 [US1] Update CLI help and docs for the subcommand in `crates/deacon/src/cli.rs` to reflect exact flags and behavior per SPEC.md

Checkpoint: US1 independently functional and demoable

---

## Phase 4: User Story 2 - Container-Aware Resolution (Priority: P2)

Goal: Support `--container-id` / `--id-label` to enable `${devcontainerId}` before-container substitution and container-based metadata for merged outputs

Independent Test: With a running container and labels, run with `--container-id` or `--id-label` and verify `${devcontainerId}` and merged behavior

### Tests for User Story 2 (OPTIONAL)

- [X] T017 [P] [US2] Add test: label order does not change `${devcontainerId}` in `crates/deacon/tests/integration_read_configuration.rs`
- [X] T018 [P] [US2] Add test: with `--container-id` and `--include-merged-configuration`, error if inspect fails (no fallback) in `crates/deacon/tests/integration_read_configuration.rs`

### Implementation for User Story 2

- [X] T019 [P] [US2] Prefer `--container-id` over `--id-label` in `crates/core/src/container.rs::resolve_container` and ensure consistent behavior (FR-001 precedence)
- [X] T020 [P] [US2] Apply beforeContainerSubstitute to set `${devcontainerId}` then containerSubstitute for `${containerEnv:*}`/`${containerWorkspaceFolder}` in `crates/deacon/src/commands/read_configuration.rs` (ensure order)
- [X] T021 [US2] Implement `containerWorkspaceFolder` derivation from container mounts/config in `crates/core/src/docker.rs` and wire into substitution context in `crates/deacon/src/commands/read_configuration.rs`
- [X] T022 [US2] When `--include-merged-configuration` with a selected container, compose merged metadata using container inspect; on inspect failure, return error (FR-009 failure mode) in `crates/deacon/src/commands/read_configuration.rs`

Checkpoint: US2 independently functional and demoable

---

## Phase 5: User Story 3 - Feature Resolution Options (Priority: P3)

Goal: Support `--include-features-configuration`, `--additional-features <JSON>`, and `--skip-feature-auto-mapping`; compute featuresConfiguration when requested

Independent Test: With a config referencing Features, run with `--include-features-configuration` and optional `--additional-features` JSON and verify output

### Tests for User Story 3 (OPTIONAL)

- [X] T023 [P] [US3] Add test: `featuresConfiguration` present when `--include-features-configuration` is set in `crates/deacon/tests/integration_read_configuration.rs`
- [X] T024 [P] [US3] Add test: deep-merge `--additional-features` with precedence over base in `crates/deacon/tests/integration_read_configuration.rs`

### Implementation for User Story 3

- [X] T025 [P] [US3] Validate and reject non-object `--additional-features` JSON early with clear error in `crates/deacon/src/commands/read_configuration.rs` (FR-008)
- [X] T026 [US3] Implement deep-merge semantics for additional features via `FeatureMerger` (CLI values take precedence) in `crates/deacon/src/commands/read_configuration.rs` and `crates/core/src/features.rs`
- [X] T027 [US3] Honor `--skip-feature-auto-mapping` for legacy string feature IDs in `crates/deacon/src/commands/read_configuration.rs`
- [X] T028 [US3] When both merged and no container selected, derive merged metadata from features (imageBuildInfo → metadata) in `crates/deacon/src/commands/read_configuration.rs` (FR-009 non-container path)

Checkpoint: US3 independently functional and demoable

---

## Phase N: Polish & Cross-Cutting Concerns

Purpose: Documentation, schema, smoke test updates, and hardening

- [X] T029 [P] Update `docs/subcommand-specs/read-configuration/SPEC.md` references if flags/wording adjusted (docs-only change)
- [X] T030 Update or add smoke assertions in `crates/deacon/tests/smoke_basic.rs` to reflect strict stdout JSON and new flags when relevant
- [X] T031 [P] Ensure contracts alignment with `specs/001-read-config-parity/contracts/read-configuration.schema.json` in serialization of `ReadConfigurationOutput` (camelCase, optional fields)
- [X] T032 Add brief note to `examples/observability/json-logs/` README if log behavior changed (stderr-only)

---

## Dependencies & Execution Order

Phase Dependencies

- Setup (Phase 1): No dependencies — can start immediately
- Foundational (Phase 2): Depends on Setup completion — BLOCKS all user stories
- User Stories (Phase 3+): Depend on Foundational completion
  - Stories can then proceed in parallel (capacity permitting)
  - Or sequentially in priority order (P1 → P2 → P3)
- Polish (Final Phase): After chosen stories complete

User Story Dependencies

- User Story 1 (P1): No dependencies beyond Phase 2
- User Story 2 (P2): No dependencies beyond Phase 2; independent of US1 output
- User Story 3 (P3): No dependencies beyond Phase 2; independent of US1/US2

Within Each User Story

- Optional tests → models/services (where applicable) → endpoints/CLI emitters → integration wiring
- Ensure each story compiles and runs independently

Parallel Opportunities

- All [P] tasks can run in parallel (different files, no ordering)
- After Phase 2 completes, US2 and US3 can proceed in parallel with US1 if team capacity allows
- Tests labeled [P] within a story can be implemented and executed in parallel

---

## Parallel Example: User Story 1

- Run tests in parallel (if included):
  - T012: acceptance test for configuration-only
  - T013: acceptance test for merged output
- Implement concurrently (separate files):
  - T014: Output shape in `read_configuration.rs`
  - T016: CLI help in `cli.rs`

---

## Parallel Example: User Story 2

- Run tests in parallel (if included):
  - T017: label order stability for `${devcontainerId}`
  - T018: merged with `--container-id` errors on inspect failure
- Implement concurrently (separate files):
  - T019: Precedence in `container.rs::resolve_container`
  - T020: Substitution ordering in `read_configuration.rs`
  - T021: Derive containerWorkspaceFolder in `docker.rs`

---

## Parallel Example: User Story 3

- Run tests in parallel (if included):
  - T023: `featuresConfiguration` inclusion
  - T024: `--additional-features` deep-merge precedence
- Implement concurrently (separate files):
  - T025: Early validation in `read_configuration.rs`
  - T026: Merge semantics via `features.rs` + `read_configuration.rs`
  - T027: Skip auto-mapping in `read_configuration.rs`

---

## Implementation Strategy

MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: US1 emit spec-compliant JSON
4. Stop and validate: parse stdout JSON, ensure stderr-only logs

Incremental Delivery

1. Add US2 container-aware resolution (error on inspect failure; no fallback)
2. Add US3 features resolution options with deep-merge precedence
3. Polish and harden; update smoke tests and docs

Parallel Team Strategy

- After Phase 2: 
  - Dev A: US1 (output contract)
  - Dev B: US2 (container-aware)
  - Dev C: US3 (features resolution)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Error on unimplemented paths rather than silently omitting requested data (No Silent Fallbacks)
- Keep build green at all times (fmt, clippy, tests)
