---

description: "Task list to implement 'features package' single + collection parity"
---

# Tasks: 002 — Features Package GAP Closure

**Input**: Design documents from `/specs/002-features-package-collection/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, contracts/

Notes
- Tests are included because the specification mandates coverage (FR-8). They are scoped and minimal.
- Tasks are grouped by user story and are independently testable.

## Format: `[ID] [P?] [Story] Description`

- [P]: Can run in parallel (different files, no unresolved dependencies)
- [Story]: User story label (US1, US2, ...)
- Every task includes an exact file path

---

## Phase 1: Setup (Shared Infrastructure)

Purpose: Prepare dependencies and skeletons to keep the build green while implementing.

- [x] T001 Add gzip dependency for packaging in crates/deacon/Cargo.toml (flate2 = "1")
- [x] T002 Verify tar/sha2/serde_json deps in crates/deacon/Cargo.toml (no changes if present)
- [x] T003 [P] Create placeholder docs reference in specs/002-features-package-collection/contracts/openapi.yaml (no code changes; for schema mapping)  

---

## Phase 2: Foundational (Blocking Prerequisites)

Purpose: Core helpers and data types used by all user stories. CRITICAL: complete before story work.

- [x] T004 Define `CollectionMetadata` and `FeatureDescriptor` structs in crates/deacon/src/commands/features.rs (near package logic)
- [x] T005 Implement helper `detect_mode(target: &Path) -> Result<Single|Collection>` in crates/deacon/src/commands/features.rs
- [x] T006 Implement helper `validate_single(target: &Path) -> Result<FeatureMetadata>` in crates/deacon/src/commands/features.rs
- [x] T007: Implement helper `enumerate_and_validate_collection(src: &Path) -> Result<Vec<(feature_id, path, FeatureMetadata)>>`
- [x] T008: Implement helper `write_collection_metadata(metadata: &CollectionMetadata, dest: &Path) -> Result<()>`
- [x] T009: Implement helper `create_feature_tgz(src: &Path, dest: &Path) -> Result<String>`
- [x] T010: Create comprehensive unit tests for all helper functions

Checkpoint: Foundation ready — proceed to user stories.

---

## Phase 3: User Story 1 — Single Feature Packaging (Priority: P1) 🎯 MVP

Goal: Package a single feature directory into a .tgz and emit devcontainer-collection.json for that single feature.

Independent Test: Running `deacon features package <feature-path> -o ./output` produces one `.tgz` and a valid `devcontainer-collection.json`; exit code 0.

### Tests (required by spec FR-8)

- [x] T011 [P] [US1] Integration test: single feature packaging in crates/deacon/tests/integration_features_package.rs

### Implementation

- [x] T012 [US1] Wire CLI defaults: make `path` positional default to `.`; `--output` default `./output`; remove `--json` for package in crates/deacon/src/cli.rs
- [x] T013 [US1] Implement single-mode flow in execute_features_package: detect single, validate metadata, create `.tgz`, build descriptors, write `devcontainer-collection.json` in crates/deacon/src/commands/features.rs
- [x] T014 [US1] Text-only output: print human-readable list of created artifacts; ensure no JSON mode for package in crates/deacon/src/commands/features.rs

Checkpoint: US1 packaging works and is independently testable.

---

## Phase 4: User Story 2 — Collection Packaging (Priority: P1)

Goal: Package all valid features under `src/<featureId>`; write one `devcontainer-collection.json` listing all packaged features.

Independent Test: Running `deacon features package <collection-root> -o ./output` produces one `.tgz` per feature and a valid collection metadata file; exit code 0.

### Tests

- [x] T015 [P] [US2] Integration test: package a multi-feature collection with valid features in crates/deacon/tests/integration_features_package.rs

### Implementation

- [x] T016 [US2] Implement collection mode detection and enumeration in crates/deacon/src/commands/features.rs
- [x] T017 [US2] For each feature: validate metadata, create `.tgz`, accumulate descriptors; write single `devcontainer-collection.json` in crates/deacon/src/commands/features.rs
- [x] T018 [US2] Ensure deterministic artifact naming and stable ordering of features in crates/deacon/src/commands/features.rs

Checkpoint: US2 collection packaging works and is independently testable.

---

## Phase 5: User Story 3 — Force Clean Output (Priority: P2)

Goal: When `--force-clean-output-folder` is set, empty output directory before writing artifacts.

Independent Test: Pre-populated output folder is emptied; only new artifacts remain; exit code 0.

### Tests

- [X] T019 [P] [US3] Integration test: pre-fill output folder, run with `--force-clean-output-folder`, assert only new artifacts in crates/deacon/tests/integration_features_package.rs

### Implementation

- [X] T020 [US3] Add `--force-clean-output-folder` flag to feature package CLI in crates/deacon/src/cli.rs
- [X] T021 [US3] Implement clean step (safe delete of contents) before packaging in crates/deacon/src/commands/features.rs

---

## Phase 6: User Story 4 — Defaults and Messages (Priority: P2)

Goal: Default target path to `.` when omitted and produce clear, text-only messages per spec.

Independent Test: Omitting target uses `.`; logs state single vs collection mode and lists artifacts; exit code 0.

### Tests

- [x] T022 [P] [US4] Integration test: omit target; verify text output and artifacts in crates/deacon/tests/integration_features_package.rs

### Implementation

- [x] T023 [US4] Ensure positional `path` is optional with default `.` for package in crates/deacon/src/cli.rs
- [x] T024 [US4] Add explicit log lines: mode detection and artifact listing (stdout) in crates/deacon/src/commands/features.rs

---

## Phase 7: User Story 5 — Error Handling (Priority: P1)

Goal: Fail fast for invalid single feature, empty/invalid collection, and mixed valid/invalid subfolders.

Independent Test: Each error scenario returns non-zero exit and prints actionable error with context.

### Tests

- [x] T025 [P] [US5] Unit tests: invalid single (missing/corrupt devcontainer-feature.json) in crates/deacon/tests/unit_features_package.rs
- [x] T026 [P] [US5] Unit tests: empty collection under src/ in crates/deacon/tests/unit_features_package.rs
- [x] T027 [P] [US5] Integration test: mixed valid/invalid subfolders → fail, list invalids, no artifacts in crates/deacon/tests/integration_features_package.rs

### Implementation

- [x] T028 [US5] Return structured errors for invalid single feature in crates/deacon/src/commands/features.rs
- [x] T029 [US5] Detect empty collections and error with guidance in crates/deacon/src/commands/features.rs
- [x] T030 [US5] Mixed collection behavior: abort run, list invalid subfolders, produce no artifacts in crates/deacon/src/commands/features.rs

---

## Phase 8: User Story 6 — Spec Conformance & Contract (Priority: P1)

Goal: Ensure `devcontainer-collection.json` matches contract schemas and includes `sourceInformation.source = "devcontainer-cli"`.

Independent Test: Validate produced JSON structure matches `contracts/openapi.yaml` shapes; includes all required fields for each feature.

### Tests

- [x] T031 [P] [US6] Unit test: `devcontainer-collection.json` structure and fields in crates/deacon/tests/unit_features_package.rs

### Implementation

- [x] T032 [US6] Populate `FeatureDescriptor` fields (id, version, name, description, options, installsAfter, dependsOn) in crates/deacon/src/commands/features.rs
- [x] T033 [US6] Write `sourceInformation.source = "devcontainer-cli"` in metadata in crates/deacon/src/commands/features.rs

---

## Phase N: Polish & Cross-Cutting Concerns

Purpose: Small refinements to improve quality and maintainability.

- [x] T034 [P] Update quickstart docs with final CLI usage in specs/002-features-package-collection/quickstart.md
- [x] T035 Code cleanup and comments for new helpers in crates/deacon/src/commands/features.rs
- [x] T036 [P] Ensure CI green: fmt, clippy, tests pass (no warnings) across workspace (Makefile targets)

---

## Phase N+1: Determinism & Output Contracts

Purpose: Ensure byte-for-byte reproducibility and explicit output-mode behavior; finalize artifact naming.

- [x] T037 Deterministic tar headers: normalize `mtime=0`, `uid/gid=0`, empty `uname/gname`, and normalized modes; sort entries lexicographically in crates/deacon/src/commands/features.rs
- [x] T038 Deterministic gzip: set gzip `mtime=0` and fixed compression level/strategy in crates/deacon/src/commands/features.rs
- [x] T039 [P] Unit test: two consecutive runs produce identical SHA256 for same inputs (single and collection) in crates/deacon/tests/unit_features_package.rs
- [x] T040 CLI guard: detect global JSON mode and exit with error "JSON output is not supported for features package" in crates/deacon/src/cli.rs
- [x] T041 [P] Integration test: invoking with global `--json` fails with the prescribed message in crates/deacon/tests/integration_features_package.rs
- [x] T042 Implement artifact name builder with sanitizer (`<featureId>-<version>.tgz`; sanitize `[a-z0-9-]`) in crates/deacon/src/commands/features.rs
- [x] T043 [P] Unit tests: naming cases (mixed case, invalid chars collapse, leading/trailing hyphens trim, missing version → error) in crates/deacon/tests/unit_features_package.rs
- [x] T044 [P] Integration test: Non‑ASCII filenames round-trip in archive (success path) in crates/deacon/tests/integration_features_package.rs
- [x] T045 [P] Unit test: read-only output directory → error `Output folder not writable: <path>` in crates/deacon/tests/unit_features_package.rs
- [x] T046 [P] Integration test: deep nesting → success or explicit path-length error identifying offending path in crates/deacon/tests/integration_features_package.rs

---

## Dependencies & Execution Order

Phase dependencies
- Setup (P1): none
- Foundational (P2): depends on Setup
- US1 (P3): depends on Foundational — MVP deliverable
- US2 (P4): depends on Foundational
- US3 (P5): depends on US1/US2 (clean step applies to both)
- US4 (P6): depends on US1/US2 (messaging/defaults built on CLI wiring)
- US5 (P7): depends on Foundational; independent of US3/US4
- US6 (P8): depends on US1/US2 (needs descriptors in place)

User story dependency graph
- US1 → US3, US4, US6
- US2 → US3, US4, US6
- US5 independent (after foundational)

---

## Parallel opportunities per story

- US1: T011 can run in parallel with T012 (tests vs CLI wiring)
- US2: T015 can run in parallel with T016 (tests vs detection/enumeration)
- US3: T019 can run in parallel with T020 (test vs flag plumbing)
- US4: T022 can run in parallel with T023 (test vs CLI default)
- US5: T025–T027 are independent and parallelizable; T028–T030 can be implemented in sequence
- US6: T031 in parallel with T032/T033 (test vs fields)

---

## Implementation strategy

MVP first (US1)
1) Complete Setup + Foundational
2) Implement US1 (single feature packaging) and its test → validate outputs

Incremental delivery
- Add US2 (collection) next, then US3 (force clean), US4 (defaults/messages)
- Add US5 (errors) and US6 (contract conformance)

---

## Format validation

All tasks follow: `- [ ] T### [P?] [USn?] Description with file path`.
