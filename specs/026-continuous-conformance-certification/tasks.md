---

description: "Task list for 026-continuous-conformance-certification"
---

# Tasks: Continuous Conformance Operation & Release Certification

**Input**: Design documents from `/specs/026-continuous-conformance-certification/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: **MANDATORY.** FR-049–FR-060 require automated acceptance tests for each lane's inclusion rule,
stable/canary separation, drift-artifact completeness, every certification failure condition, scope exactness,
report reproducibility, and prevention of automatic snapshot blessing. Test tasks are first-class here, not
optional, and are written before the implementation they cover.

**Organization**: Grouped by user story so each is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US6, mapping to the six user stories in spec.md

## Path Conventions

Rust workspace. Hermetic logic in `crates/conformance/src/`, live logic in `crates/parity-harness/src/`,
test binaries in `crates/deacon/tests/`, data under `conformance/`, CI under `.github/workflows/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the new data roots and module skeletons so every later task has a home.

- [X] T001 Create `conformance/lanes/lanes.json` with the five `lane-` records per data-model.md §1 (`lane-pr-hermetic`, `lane-pr-docker`, `lane-nightly-stable`, `lane-canary`, `lane-release-certification`), each with `blocking`, `preconditions`, `nextestProfile`, `mayWriteRecord: false`, `includes`, and `excludes.rationale`
- [X] T002 [P] Create `conformance/drift/observations.json` with `schemaVersion`, an empty `records` array, and a null `lastCompletedRun` per data-model.md §4
- [X] T003 [P] Create `conformance/discovery/canary.json` with `schemaVersion` and an empty `records` array per data-model.md §6
- [X] T004 Add `pub mod lane;`, `pub mod manifest;`, `pub mod certification;`, `pub mod drift;` and the path constants `default_lanes_dir()`, `default_drift_dir()`, `default_canary_file()` to `crates/conformance/src/lib.rs`
- [X] T005 [P] Add `pub mod drift;` and `pub mod manifest_emit;` to `crates/parity-harness/src/lib.rs`
- [X] T006 [P] Create empty module files `crates/conformance/src/lane.rs`, `crates/conformance/src/manifest.rs`, `crates/conformance/src/certification.rs`, `crates/conformance/src/drift/mod.rs`, `crates/conformance/src/drift/check.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared derivation and violation-class plumbing that three or more stories build on.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T007 Add `V31`, `V32`, `V33` variants to the `Violation` enum in `crates/conformance/src/validate.rs` with dispatch stubs returning empty vectors, so each story can fill its own checker without contending on this file
- [X] T008 [P] Add the `D6` variant to the discovery violation enum in `crates/conformance/src/discovery/queue.rs` with a dispatch stub
- [X] T009 Implement `derive_execution_units()` in `crates/conformance/src/lane.rs` enumerating all four unit kinds per data-model.md §2 — validation classes from **both** the registry `Violation` enum and the discovery class enum, cases from `Registry::cases`, programs by scanning `crates/deacon/tests/*.rs` for `#[test]` functions (reuse the scanner pattern in `crates/conformance/src/baseline.rs`), and snapshot replay targets from the `conformance/snapshots/<os-arch>/<case-id>/` tree
- [X] T010 Implement `case_lane_membership()` in `crates/conformance/src/lane.rs` deriving `needs_oracle` from `oracleType` and `needs_container` from `resourceGroup` per research D9, with no new per-case field
- [X] T011 [P] Add `[profile.pr-docker]` and `[profile.canary]` to `.config/nextest.toml`, each with an explicit `binary(=…)` `default-filter` allow-list (empty for now), add exclusions for those binaries to the `default-filter` of all six existing profiles, and declare their test groups in **every** profile's overrides per the documented 3-spot rule — a binary excluded from a filter still needs its group declared wherever it could run
- [X] T012 [P] Add `lane` and `drift` command-group skeletons to `crates/conformance/src/bin/conformance.rs` with `--lanes <DIR>` and `--drift <DIR>` overrides mirroring the existing `--registry` pattern

**Checkpoint**: Denominator derivation, violation plumbing, and profile scaffolding exist — stories can begin.

---

## Phase 3: User Story 1 - Certify a release with an exact, honest scope (Priority: P1) 🎯 MVP

**Goal**: `certify --report-dir` emits a byte-reproducible report stating exactly what was certified, with an
explicit non-extension scope statement and an enumeration of what was not certified.

**Independent Test**: Run `certify --report-dir` against the current registry with the reference
implementation uninstalled and the network unavailable; confirm all sixteen FR-034 fields are present, the
scope names exactly one profile, and two runs on different machines produce identical bytes.

### Tests for User Story 1

- [X] T013 [P] [US1] Test that the report contains all sixteen FR-034 identity/environment/sourceScope/coverage/exception/provenance fields — and, separately, the four required-but-not-among-the-sixteen fields (`scope.profile`, `scope.doesNotCertify`, `evaluationDate`, `notCertified`) per data-model.md §7 — in `crates/deacon/tests/certification_gates.rs`
- [X] T014 [P] [US1] Test scope exactness (FR-053) — the report names exactly one profile and its `doesNotCertify` list covers other engines, operating systems, architectures, and oracle versions — in `crates/deacon/tests/certification_gates.rs`
- [X] T015 [P] [US1] Test byte reproducibility (FR-054, SC-005) by generating the report twice with a fixed `--today` and comparing bytes, in `crates/deacon/tests/certification_gates.rs`
- [X] T016 [P] [US1] Test that certification succeeds with no reference implementation on `PATH`, no container engine, and no network (FR-056, SC-013), in `crates/deacon/tests/certification_gates.rs`
- [X] T017 [P] [US1] Test that `notCertified` enumerates inactive profiles, `non-testable`/`not-applicable` units, and platforms with no committed snapshot (FR-037), in `crates/deacon/tests/certification_gates.rs`
- [X] T018 [P] [US1] Test that certifying the inactive `prof-linux-amd64-podman-0870` profile exits 1 with an explicit refusal rather than a vacuous pass (FR-045), in `crates/deacon/tests/certification_gates.rs`

### Implementation for User Story 1

- [X] T019 [US1] Define the `CertificationReport` struct and its nested `identity`/`environment`/`scope`/`sourceScope`/`coverage`/`exceptions` types per data-model.md §7 in `crates/conformance/src/certification.rs`
- [X] T020 [US1] Implement identity population (deacon revision from the build environment, oracle version and spec/schema revisions from `revisions.json`) in `crates/conformance/src/certification.rs`
- [X] T021 [US1] Implement environment population (platform, arch, container engine and version, Compose version) sourced from the execution manifest when present and the active profile otherwise, in `crates/conformance/src/certification.rs`
- [X] T022 [US1] Implement the scope block — one profile plus the explicit `doesNotCertify` enumeration and prose `statement` (FR-035) — in `crates/conformance/src/certification.rs`
- [X] T023 [US1] Implement `sourceScope` population (schema document count, prose document count, CLI surface, unclassified units) reusing `check_inventory` and `check_clause_inventory` rather than re-walking the inventories, in `crates/conformance/src/certification.rs`
- [X] T024 [US1] Implement `coverage` population (behavior count, context coverage from `evaluate_obligations`, observable coverage as per-channel covering-case counts) in `crates/conformance/src/certification.rs`
- [X] T025 [US1] Implement `exceptions` population (gaps, waivers, intentional divergences from `ext-` records) in `crates/conformance/src/certification.rs`
- [X] T026 [US1] Implement `snapshotProvenance` population, reporting per snapshot its evidence-determining inputs and recording platform, reusing `snapshot::Provenance::staleness_fields`, in `crates/conformance/src/certification.rs`
- [X] T027 [US1] Implement `notCertified` population (inactive profiles, non-testable/not-applicable units, no-reference-for-platform) in `crates/conformance/src/certification.rs`
- [X] T028 [US1] Implement the deterministic Markdown renderer for `certification.md` with stable ordering and no timestamps, absolute paths, or hostnames, in `crates/conformance/src/certification.rs`
- [X] T029 [US1] Wire `--report-dir <DIR>` into the `certify` subcommand in `crates/conformance/src/bin/conformance.rs`, writing atomically on both exit 0 and exit 1 so a blocked release still ships its report, and taking `evaluationDate` from the existing global `--today`
- [X] T030 [US1] Add the inactive-profile refusal path to `certify()` in `crates/conformance/src/certify.rs` so certifying an inactive profile exits 1 with a named refusal instead of passing over zero applicable units

**Checkpoint**: US1 delivers a complete, reproducible certification report against the existing verdict.

---

## Phase 4: User Story 2 - Refuse to certify when the evidence is incomplete (Priority: P1)

**Goal**: All nine failure conditions block a release, each naming its offending record, all reported in one
run, with no flag able to downgrade any of them.

**Independent Test**: Inject each condition into a fixture registry one at a time, confirm certification
flips to not-certified naming the injected record, and confirm removing it restores certification. Manifest
conditions are tested with hand-authored fixture manifests, so this story does not depend on US3 producing
real ones.

### Tests for User Story 2

- [X] T031 [P] [US2] Build fixture registries and fixture execution manifests for the nine conditions under `fixtures/conformance/certification/` (one directory per condition, each otherwise certifiable)
- [X] T032 [P] [US2] Positive-control test for condition (a) unclassified source change, in `crates/deacon/tests/certification_gates.rs`
- [X] T033 [P] [US2] Positive-control test for condition (b) applicable in-profile behavior uncovered, in `crates/deacon/tests/certification_gates.rs`
- [X] T034 [P] [US2] Positive-control test for condition (c) stale snapshot, asserting the report names the snapshot and the first drifted input, in `crates/deacon/tests/certification_gates.rs`
- [X] T035 [P] [US2] Positive-control test for condition (d) unknown runner omission, in `crates/deacon/tests/certification_gates.rs`
- [X] T036 [P] [US2] Positive-control test for condition (e) expired waiver, confirming the existing V6 disposition gate blocks and naming the waiver, in `crates/deacon/tests/certification_gates.rs`
- [X] T037 [P] [US2] Positive-control test for condition (f) unresolved gap, in `crates/deacon/tests/certification_gates.rs`
- [X] T038 [P] [US2] Positive-control test for condition (g) incorrect oracle, covering both a snapshot-provenance mismatch and a manifest mismatch, in `crates/deacon/tests/certification_gates.rs`
- [X] T039 [P] [US2] Positive-control test for condition (h) missing required execution across all four manifest rejection modes — absent, incomplete, revision-mismatched, hash-stale (FR-057, SC-014) — in `crates/deacon/tests/certification_gates.rs`
- [X] T040 [P] [US2] Positive-control test for condition (i) silently skipped case, covering an out-of-enumeration outcome and an `excluded` outcome with an unresolvable `excludedBy`, in `crates/deacon/tests/certification_gates.rs`
- [X] T041 [P] [US2] Test that several simultaneous conditions are all reported in one run (FR-043) and that each names a specific record rather than a count (FR-042), in `crates/deacon/tests/certification_gates.rs`
- [X] T042 [P] [US2] Test that no flag or environment variable downgrades any condition to a warning (FR-044), asserting the `certify` argument surface exposes no skip/allow/ignore flag, in `crates/deacon/tests/certification_gates.rs`
- [X] T043 [P] [US2] Test that a failing case in the manifest blocks certification as an ordinary case failure and is reported distinctly from the `V32` manifest-integrity violations, in `crates/deacon/tests/certification_gates.rs`
- [X] T044 [P] [US2] Test that manifest freshness and snapshot freshness are independent obligations (FR-033e) — a fresh manifest does not excuse a stale snapshot and a fresh snapshot does not excuse an absent manifest — in `crates/deacon/tests/certification_gates.rs`
- [X] T045 [P] [US2] Test that non-deterministic evidence contributes zero coverage to the verdict (FR-046, SC-012) — a fixture with discovery findings and corpus results present yields a verdict byte-identical to one without them — and that when a fully pinned, hermetic input *is* admitted the report records which inputs qualified and why (FR-047), in `crates/deacon/tests/certification_gates.rs`

### Implementation for User Story 2

- [X] T046 [US2] Define the `ExecutionManifest`, `ManifestCase`, and `ManifestEnvironment` types with strict-JSON deserialization per data-model.md §3 in `crates/conformance/src/manifest.rs`
- [X] T047 [US2] Implement `verify_manifest()` in `crates/conformance/src/manifest.rs` emitting the five distinct `V32` sub-cases (`absent`, `incomplete`, `revision`, `stale`, `unaccounted`) plus the environment-mismatch detail, evaluating all checks rather than short-circuiting
- [X] T048 [US2] Wire the `V32` checker into `validate_path_with_inventory` in `crates/conformance/src/validate.rs`, replacing the T007 stub
- [X] T049 [US2] Add the `StaleSnapshot` blocking kind to `certify()` in `crates/conformance/src/certify.rs`, comparing committed provenance via `snapshot::compare_staleness` and scoping the missing-snapshot block to the profile under certification per research D5
- [X] T050 [US2] Add the `MissingExecution` blocking kind, consuming `verify_manifest()` output, in `crates/conformance/src/certify.rs`
- [X] T051 [US2] Add the `IncorrectOracle` blocking kind comparing recorded oracle identity in both snapshot provenance and the manifest against the declared stable pin, in `crates/conformance/src/certify.rs`
- [X] T052 [US2] Add the `RunnerOmission` blocking kind reconciling the executed set (manifest cases ∪ replay results) against the declared applicable set, in `crates/conformance/src/certify.rs`
- [X] T053 [US2] Add the `SilentlySkippedCase` blocking kind for results outside the pass/fail/dispositioned-exclusion enumeration, in `crates/conformance/src/certify.rs`
- [X] T054 [US2] Ensure `certify()` in `crates/conformance/src/certify.rs` draws coverage exclusively from registry cases, snapshots, and the manifest — never from `conformance/discovery/` — and add the report's `admittedNonDeterministicInputs` field recording any fully pinned, hermetic input that was admitted, with the reason it qualified (FR-046, FR-047)
- [X] T055 [US2] Wire `--manifest <FILE>` into the `certify` subcommand in `crates/conformance/src/bin/conformance.rs`, defaulting to `target/conformance/execution-manifest.json`
- [X] T056 [US2] Extend the `blocking` array in the certification report to render each new condition with its condition name, offending record id, and detail, in `crates/conformance/src/certification.rs`

**Checkpoint**: All nine conditions block, name their records, and report together. US1 + US2 = a trustworthy gate.

---

## Phase 5: User Story 3 - Know which lane proved what (Priority: P1)

**Goal**: Five lanes with declared inclusion rules, full unit assignment enforced mechanically, and loud
failure on any missing precondition.

**Independent Test**: Run `lane check` against the lane records; confirm every derived unit is assigned,
introduce a new test binary and confirm validation fails without any hand edit to a unit list.

### Tests for User Story 3

- [X] T057 [P] [US3] Test each lane's inclusion rule (FR-049) — for all five lanes, assert both the units it selects and the units it excludes — in `crates/deacon/tests/lane_integrity.rs`
- [X] T058 [P] [US3] Test that the denominator is machine-derived (FR-059, SC-001) by adding a fixture test program and asserting `lane check` fails with `V31` without editing any unit list, in `crates/deacon/tests/lane_integrity.rs`
- [X] T059 [P] [US3] Test that programs and validation classes are selected by explicit allow-list with no pattern matching (FR-002), and that each lane's declared `nextestProfile` `default-filter` agrees with its declared programs, in `crates/deacon/tests/lane_integrity.rs`
- [X] T060 [P] [US3] Test that `blocking` is false for `lane-canary` and `lane-nightly-stable` and that `mayWriteRecord` is false for all five, in `crates/deacon/tests/lane_integrity.rs`
- [X] T061 [P] [US3] Test that the `includes`/`excludes` case predicates partition the case space with no overlap and no remainder (FR-002a), so derived selection can capture a new case but never drop one, in `crates/deacon/tests/lane_integrity.rs`
- [X] T062 [P] [US3] Test that a lane whose precondition is unavailable fails loudly rather than skipping (FR-004), using the harness fault-injection seam, in `crates/deacon/tests/lane_integrity.rs`
- [X] T063 [P] [US3] Test that `lane report`'s exit code reflects writability only and never what it reports, and that its output states both the units each lane ran and the units it deliberately excluded (FR-005), in `crates/deacon/tests/lane_integrity.rs`
- [X] T064 [P] [US3] Test that no unit reaches an ignored, pending, or conditionally skipped state (FR-006) — scan every lane's test programs for `#[ignore]` and for early-return skip patterns, asserting each unit resolves to passed, failed, or explicitly excluded — in `crates/deacon/tests/lane_integrity.rs`
- [X] T065 [P] [US3] Test the hermetic lane's own hermeticity (FR-008) — running it with no container engine, no reference implementation on `PATH`, and no network succeeds, and any unit that tries to resolve one of those fails loudly — in `crates/deacon/tests/lane_integrity.rs`
- [X] T066 [P] [US3] Test that no lane other than the reviewed record path can write a committed snapshot (FR-055, SC-006) by extending `only_the_refresh_bin_writes_committed_snapshots` to cover `conformance_replay`, `conformance_docker_pinned`, and `discovery_canary` behaviorally, plus a source scan asserting no lane binary references a snapshot writer, in `crates/deacon/tests/lane_integrity.rs`

### Implementation for User Story 3

- [X] T067 [US3] Define the `Lane` record type with strict-JSON deserialization and `IndexMap`-preserved order per data-model.md §1, and its loader, in `crates/conformance/src/lane.rs`
- [X] T068 [US3] Implement the `V31` checker in `crates/conformance/src/lane.rs` covering zero-assignment units, unknown class/program/profile references, overlapping or incomplete case predicates, wrong `blocking` values, `mayWriteRecord: true`, and profile-filter disagreement
- [X] T069 [US3] Wire the `V31` checker into `validate_path_with_inventory` in `crates/conformance/src/validate.rs`, replacing the T007 stub
- [X] T070 [US3] Implement `lane check` per `contracts/cli-lane.md` (stable violation ordering, `--json`, exit 0/1/2) in `crates/conformance/src/bin/conformance.rs`
- [X] T071 [US3] Implement `lane report` writing `target/conformance/lanes.{json,md}` atomically with a per-lane ran/excluded breakdown and the stated exclusion rationale (FR-005), and an exit code reflecting writability only, in `crates/conformance/src/bin/conformance.rs`
- [X] T072 [US3] Implement `lane scaffold` emitting an `UNREVIEWED`-sentinel skeleton to stdout and writing nothing, in `crates/conformance/src/bin/conformance.rs`
- [X] T073 [US3] Create the hermetic replay binary `crates/deacon/tests/conformance_replay.rs` driving the 45 oracle-free non-Docker cases (`spec-expectation`, `snapshot`, `invariant-metamorphic` with `resourceGroup` none or `fs-heavy`), failing loudly on any missing fixture and holding no write path to a committed snapshot
- [X] T074 [US3] Create the container binary `crates/deacon/tests/conformance_docker_pinned.rs` driving the 39 oracle-free Docker cases without resolving an oracle, reusing the `parity_conformance_docker` resource-group driver structure and its per-case and tier bounds, and relying on the existing V18 pinned-image class for FR-013 rather than re-implementing that check
- [X] T075 [US3] Implement `emit_manifest()` in `crates/parity-harness/src/manifest_emit.rs` writing the execution manifest atomically per `contracts/execution-manifest.md`, recording every required case including failures and dispositioned exclusions, computing hashes at execution time, and emitting on failed runs too
- [X] T076 [US3] Call `emit_manifest()` from `crates/deacon/tests/conformance_docker_pinned.rs` so the container lane always produces its receipt
- [X] T077 [US3] Add `conformance_replay`, `lane_integrity`, `certification_gates`, and `drift_hermetic` to the `default` and `dev-fast` profiles, add `conformance_docker_pinned` to the `pr-docker` allow-list with exclusions in all six other profiles, and declare every new binary's test group in **every** profile's overrides, in `.config/nextest.toml`
- [X] T078 [US3] Create `.github/workflows/conformance-docker.yml` running the `pr-docker` profile on pull requests, uploading the execution manifest as an artifact, with no `continue-on-error` anywhere
- [X] T079 [US3] Populate the five lane records in `conformance/lanes/lanes.json` with their final `includes`/`excludes` now that all binaries and profiles exist

**Checkpoint**: Lanes are data, fully assigned, and mechanically enforced. All three P1 stories complete.

---

## Phase 6: User Story 4 - Detect upstream drift without blessing it (Priority: P2)

**Goal**: Five upstream source kinds observed on a cadence, producing review artifacts only, gating nothing,
and structurally unable to write a pin, disposition, or snapshot.

**Independent Test**: Point `drift-scan` at an upstream state ahead of the pins; confirm it emits signals for
each changed kind, exits 0, and leaves every pin, disposition, snapshot, and waiver byte-identical.

### Tests for User Story 4

- [X] T080 [P] [US4] Test `V33` observation integrity — derived-id mismatch, unknown `kind`, and a `lastCompletedRun` missing a probed kind — in `crates/deacon/tests/drift_hermetic.rs`
- [X] T081 [P] [US4] Test that "no drift" (empty `records` with a complete `lastCompletedRun`) is distinguishable from "did not run" (missing or partial `lastCompletedRun`) per FR-025, in `crates/deacon/tests/drift_hermetic.rs`
- [X] T082 [P] [US4] Test the path allow-list (FR-058, SC-015) — a proposed diff touching a registry record, committed snapshot, or pin aborts the scan naming the path and writes nothing, rather than committing a narrowed diff — in `crates/deacon/tests/drift_hermetic.rs`
- [X] T083 [P] [US4] Test that `drift-scan` exits 0 when it finds drift and non-zero only on machinery failure (FR-026), using the fault-injection seam for an unreachable upstream, in `crates/deacon/tests/drift_hermetic.rs`
- [X] T084 [P] [US4] Test that `drift report`'s exit code reflects writability only, in `crates/deacon/tests/drift_hermetic.rs`

### Implementation for User Story 4

- [X] T085 [US4] Define the `DriftObservation` record with its substance-anchored `hash8` id, the five `kind` values, and the `lastCompletedRun` block per data-model.md §4, in `crates/conformance/src/drift/mod.rs`
- [X] T086 [US4] Implement the `V33` observation checker in `crates/conformance/src/drift/check.rs` and wire it into `validate.rs`, replacing the T007 stub
- [X] T087 [US4] Implement `drift check`, `drift report`, and `drift scaffold` per `contracts/cli-drift.md` in `crates/conformance/src/bin/conformance.rs`
- [X] T088 [US4] Implement the `spec-commit` and `upstream-test-or-changelog` probes via bounded `git ls-remote` and blob-filtered partial clone, reusing the subprocess pattern in `crates/parity-harness/src/discovery/corpus_fetch.rs`, in `crates/parity-harness/src/drift/scan.rs`
- [X] T089 [US4] Implement the `schema-change` probe comparing per-document SHA-256 against `conformance/schemas/<pin>/manifest.json`, in `crates/parity-harness/src/drift/scan.rs`
- [X] T090 [US4] Implement the `reference-release` probe via bounded `npm view @devcontainers/cli versions --json`, in `crates/parity-harness/src/drift/scan.rs`
- [X] T091 [US4] Implement the `cli-surface-change` probe comparing the candidate release's `--help` surface against the recorded CLI-surface revision, in `crates/parity-harness/src/drift/scan.rs`
- [X] T092 [US4] Implement the write path allow-list in `crates/parity-harness/src/drift/mod.rs`, permitting only `conformance/drift/observations.json` and `target/drift/*` and aborting with the attempted path on any other target
- [X] T093 [US4] Create the `drift-scan` binary in `crates/parity-harness/src/bin/drift-scan.rs` with `--kinds` and `--write`, writing observations atomically and exiting 0 regardless of findings
- [X] T094 [US4] Create `.github/workflows/drift.yml` running nightly and on manual dispatch, gating nothing, uploading `target/drift/` on every run

**Checkpoint**: Drift is observed and reviewable, and cannot bless anything.

---

## Phase 7: User Story 5 - Separate the stable reference from canary experiments (Priority: P2)

**Goal**: Canary comparison against immutably pinned development revisions, non-blocking everywhere, and
structurally incapable of touching stable data.

**Independent Test**: Run the canary lane against a pinned revision; confirm a separately labeled artifact,
non-blocking status, and a byte-identical stable data tree before and after.

### Tests for User Story 5

- [X] T095 [P] [US5] Test `D6` canary-pin integrity — a branch name, moving tag, or dist-tag is rejected at load (FR-018); duplicate and non-derived ids are rejected — in `crates/deacon/tests/discovery_hermetic.rs`
- [X] T096 [P] [US5] Test stable/canary separation (FR-050, SC-008) by hashing the stable data tree before and after a canary run and asserting byte-identity, in `crates/deacon/tests/lane_integrity.rs`
- [X] T097 [P] [US5] Test that the certification verdict is byte-identical with `canary.json` populated and with it absent (FR-060, SC-016), in `crates/deacon/tests/certification_gates.rs`
- [X] T098 [P] [US5] Source-scan test that no registry or snapshot writer references the canary pin file (FR-017a), mirroring `no_discovery_source_references_a_registry_or_snapshot_writer`, in `crates/deacon/tests/discovery_hermetic.rs`
- [X] T099 [P] [US5] Test that the nightly stable differential lane fails as a machinery error when the resolved reference is any version other than the declared stable pin, rather than reporting it as a divergence (FR-014), in `crates/deacon/tests/parity_harness_faults.rs`
- [X] T100 [P] [US5] Test that the nightly stable differential lane writes nothing (FR-016) — the committed snapshot, pin, disposition, waiver, gap, and registry trees are byte-identical before and after a nightly run — in `crates/deacon/tests/lane_integrity.rs`

### Implementation for User Story 5

- [X] T101 [US5] Define the `CanaryPin` record with its derived id and immutable-revision validation per data-model.md §6, in `crates/conformance/src/discovery/queue.rs`
- [X] T102 [US5] Implement the `D6` checker in `crates/conformance/src/discovery/queue.rs` and wire it into `discovery check`, replacing the T008 stub
- [X] T103 [US5] Create the canary campaign binary `crates/deacon/tests/discovery_canary.rs` comparing against the pinned canary revisions and writing results to a canary-labeled artifact under `target/discovery/canary/`
- [X] T104 [US5] Add `discovery_canary` to the `canary` profile allow-list with exclusions in all six other profiles, and declare its test group in every profile's overrides, in `.config/nextest.toml`
- [X] T105 [US5] Add the explicit stable-pin identity assertion to the nightly lane in `.github/workflows/parity.yml`, failing as machinery on any version other than the declared pin
- [X] T106 [US5] Create `.github/workflows/canary.yml` running on a schedule and manual dispatch, non-blocking in every context, uploading the canary artifact separately from any certification artifact

**Checkpoint**: Canary signal exists and provably cannot reach the stable record.

---

## Phase 8: User Story 6 - Propose a stable oracle upgrade as a reviewed change (Priority: P3)

**Goal**: A deterministic seven-section review bundle that a human, and only a human, can act on.

**Independent Test**: Request a proposal between two versions; confirm all seven sections are present,
regeneration is byte-identical, and no code path exists from the bin to any pin.

### Tests for User Story 6

- [X] T107 [P] [US6] Test drift-artifact completeness (FR-051) — a bundle missing any of the seven section keys is rejected, while a section with an empty `entries` array is clean — in `crates/deacon/tests/drift_hermetic.rs`
- [X] T108 [P] [US6] Test bundle determinism (FR-031, SC-007) by regenerating from the same before/after pins and comparing bytes, in `crates/deacon/tests/drift_hermetic.rs`
- [X] T109 [P] [US6] Source-scan test that no drift or proposal source references a pin writer, a disposition writer, or the snapshot refresh path (FR-028, SC-006), in `crates/deacon/tests/drift_hermetic.rs`
- [X] T110 [P] [US6] Test that a bundle computed against a dirty working tree records `inputState.worktreeClean: false` and is recognizable as such, in `crates/deacon/tests/drift_hermetic.rs`
- [X] T111 [P] [US6] Test that canary evidence is admitted to a bundle's `referenceBehaviorDrift` section only when every input was pinned by immutable identifier and the run was hermetic, and is otherwise recorded as informational only (FR-033), in `crates/deacon/tests/drift_hermetic.rs`

### Implementation for User Story 6

- [X] T112 [US6] Define the `UpgradeProposal` record with all seven required section keys and the `inputState` block per data-model.md §5, in `crates/conformance/src/drift/mod.rs`
- [X] T113 [US6] Implement `drift proposal check` validating section completeness and byte-stable regeneration in `crates/conformance/src/drift/check.rs` and wire it into `crates/conformance/src/bin/conformance.rs`
- [X] T114 [US6] Implement the `schemaDrift`, `specificationDrift`, and `cliSurfaceDrift` section builders in `crates/parity-harness/src/drift/proposal.rs`, reusing the T088–T091 probes
- [X] T115 [US6] Implement the `referenceBehaviorDrift` and `newlyFailingCases` section builders by running the declarative case set against both oracle versions, marking any non-hermetic or non-pinned canary input as informational rather than as evidence, in `crates/parity-harness/src/drift/proposal.rs`
- [X] T116 [US6] Implement the `snapshotDifferences` and `affectedDispositions` section builders in `crates/parity-harness/src/drift/proposal.rs`
- [X] T117 [US6] Create the `oracle-upgrade-propose` binary in `crates/parity-harness/src/bin/oracle-upgrade-propose.rs` with `--from`/`--to`, writing `target/drift/upgrade-proposal.{json,md}` atomically with stable entry ordering and no timestamps or absolute paths

**Checkpoint**: All six stories complete.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [X] T118 [P] Extend `crates/deacon/tests/parity_registry_check.rs` to assert that `deacon --help` gains nothing from the `lane`, `drift`, `drift proposal`, `drift-scan`, or `oracle-upgrade-propose` surfaces
- [X] T119 Add a `docker-execution` job to `.github/workflows/release.yml` that runs the `pr-docker` profile and uploads the execution manifest, and make the existing `verify` job consume that artifact before running `certify --report-dir`, keeping `verify` free of Docker, Node, and network
- [X] T120 [P] Upload `target/conformance/certification.{json,md}` as a release artifact in `.github/workflows/release.yml`, on both certified and blocked outcomes
- [X] T121 [P] Add `test-lanes`, `test-drift`, and `certify-report` targets to `Makefile` wrapping the new commands
- [X] T122 [P] Document violation classes V31–V33 and D6, and the lane/manifest/drift record kinds, in `conformance/RULES.md`, keeping it in lockstep with `validate.rs`
- [X] T123 [P] Add a "Continuous Conformance Operation" section to `CLAUDE.md` covering the five lanes, the two gates, the execution manifest, and the never-gates rule for drift and canary
- [X] T124 [P] Register `conformance_replay` coverage in the hermetic lane and extend the workflow path triggers to `conformance/lanes/**` and `conformance/drift/**`, in `.github/workflows/ci.yml`
- [X] T125 Run every workflow in `specs/026-continuous-conformance-certification/quickstart.md` end to end and correct any drift between the document and the implemented commands
- [X] T126 Run the full gate — `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest` — and fix all failures including any pre-existing ones

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — **blocks all user stories**
- **US1 (Phase 3)**, **US2 (Phase 4)**, **US3 (Phase 5)**: depend only on Foundational; independent of each other
- **US4 (Phase 6)**, **US5 (Phase 7)**: depend only on Foundational
- **US6 (Phase 8)**: depends on Foundational; reuses US4's probes (T088–T091), so scheduling it after US4 avoids duplicated work
- **Polish (Phase 9)**: depends on the stories whose surfaces it wires up

### User Story Independence

- **US1** produces the report from the existing verdict; it needs none of US2's new blockers.
- **US2** verifies manifests using hand-authored fixtures (T031), so it does not wait on US3 producing real ones. This is deliberate — the two halves of the manifest contract are testable separately.
- **US3** is pure lane machinery; its binaries are new files.
- **US4** and **US5** touch disjoint roots (`drift/` and `discovery/`).
- **US6** is the only story with a soft dependency, on US4's probes.

The one genuine sequencing constraint outside Foundational: **T077 and T104 both edit `.config/nextest.toml`
profile filters.** Serialize them, and expect a conflict if US3 and US5 land close together — resolve to the
union of both `binary(=…)` clauses, as the repository's existing guidance for that file requires.

### Within Each User Story

Tests before implementation. Within implementation: record types → checkers → command wiring → binaries →
CI workflows.

### Parallel Opportunities

- T002, T003 in Setup; T008, T011, T012 in Foundational
- All six US1 tests (T013–T018) — same file, so land as one commit or serialize the edits
- All fourteen US2 tests (T032–T045) after T031 builds the fixtures
- US1, US2, and US3 in parallel across three developers once Foundational completes
- US4 and US5 in parallel with each other and with the P1 stories

---

## Parallel Example: Foundational → three P1 stories

```bash
# After Phase 2 completes, three developers can start:
Developer A: T013–T030  (US1 — certification report)
Developer B: T031–T056  (US2 — refusal conditions)
Developer C: T057–T079  (US3 — lanes)

# Within US4, the four probe implementations share scan.rs;
# land them sequentially or split the module first:
T088 (spec-commit + changelog) → T089 (schema) → T090 (release) → T091 (cli-surface)
```

---

## Implementation Strategy

### MVP (User Story 1 only)

1. Phase 1 Setup → Phase 2 Foundational → Phase 3 US1
2. **STOP and VALIDATE**: `certify --report-dir` produces a complete, reproducible report with an honest
   scope statement, with no reference implementation installed.
3. This alone replaces "deacon is conformant" with a claim that states its own boundaries — the single
   highest-value increment.

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. **US1** → the report exists → demo
3. **US2** → the report cannot be produced on incomplete evidence → the gate is now trustworthy
4. **US3** → lanes are explicit and fully assigned → PR-time results become interpretable
5. **US4** → drift is observed → the record stops silently aging
6. **US5** → canary signal without contamination
7. **US6** → pin upgrades become reviewable

US1 + US2 together are the meaningful release milestone: a report that states what it certifies and refuses
to exist when it cannot.

### Notes

- `[P]` means different files and no dependency on incomplete work.
- Several test tasks share one file (`certification_gates.rs`, `lane_integrity.rs`, `drift_hermetic.rs`);
  they are marked `[P]` because they are independent in content, but the edits must be serialized or landed
  together.
- Every new test binary needs entries in **all** nextest profiles — the allow-list in its own profile, an
  exclusion in the six others, **and** a test-group declaration in every profile. `lane check` (T068)
  enforces the filter half, so a missed entry fails structurally rather than in review.
- Never add a flag that downgrades a certification failure (T042 asserts this), and never let a reporting
  command's exit code depend on what it reports (T063, T084).

## Deferred Work

None. Every functional requirement in spec.md is scheduled above. If work is deferred during implementation,
it must be recorded here with a task ID continuing the sequence, a reference to the `research.md` decision
that created the deferral, and specific acceptance criteria — per constitution Principle I, this feature is
not complete while any deferral remains open.
