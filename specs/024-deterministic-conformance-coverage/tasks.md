---

description: "Task list for 024-deterministic-conformance-coverage"
---

# Tasks: Deterministic Conformance Coverage

**Input**: Design documents from `/specs/024-deterministic-conformance-coverage/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Test tasks are **MANDATORY** for this feature. FR-071 makes every acceptance
scenario an automated test, and FR-072 restricts execution to nextest under a named profile.
Test tasks are therefore not optional and are written before the implementation they cover.

**Organization**: Grouped by user story. Phase 2 is split into two blocks — hermetic core
(blocks everything) and live infrastructure (blocks US3–US6 only), so the US1 MVP does not
pay for machinery it never uses.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US6)
- Every task names its exact file path

## Path Conventions

- Hermetic data/validation/reporting: `crates/conformance/src/`
- Live execution/observation/injection: `crates/parity-harness/src/`
- Live test drivers: `crates/deacon/tests/`
- Records: `conformance/registry/`, `conformance/obligations/`, `conformance/fixtures/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Wire the new module and command surfaces so later tasks land in real files

- [X] T001 Confirm the pre-change baseline is green: run `cargo run -p deacon-conformance -- validate` and `cargo run -p deacon-conformance -- certify` and record both exit codes in the PR body — both exit 0 (validate: "ok: registry is valid"; certify: "certified: prof-linux-amd64-docker-0870 (0 blocking, 10 waived)")
- [X] T002 [P] Create empty module stubs `scenario.rs`, `obligation.rs`, `coverage_report.rs`, `regression.rs` in `crates/conformance/src/` and declare them in `crates/conformance/src/lib.rs`
- [X] T003 [P] Add the `Coverage` command enum variant and `CoverageCommand` subcommands (`Generate`, `Check`, `Report`, `Scaffold`) to `crates/conformance/src/bin/conformance.rs` per `contracts/coverage-cli.md`, dispatching to `todo!()` stubs
- [X] T004 [P] Add the V26–V30 rows to the violation-class index table in `conformance/RULES.md` with links to the sections added in later tasks
- [X] T005 [P] Add `target/conformance/coverage-*.json`, `target/conformance/coverage-*.md`, and `target/conformance/regressions.json` to `.gitignore` — already covered by the existing wholesale `target` ignore (verified no negation rule un-ignores it); no edit needed
- [X] T006 [P] Create the `conformance/obligations/` directory with a `.gitkeep` so the machine-owned tree exists before generation writes to it

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ CRITICAL**: Block A blocks every user story. Block B blocks US3–US6 only and is **not**
required for the US1/US2 MVP.

### Block A — Hermetic core (blocks ALL stories)

- [X] T007 Migrate `conformance/registry/cases.json` to `conformance/registry/cases/<area>.json`, splitting by the area of each case's first linked behavior, in its **own commit** with no content changes (research Decision 7) — landed as its own commit; verified byte-identical records (concatenate + id-sort reproduces the original record list exactly). Per-area counts: build 1, exec 4, host-ca 1, observable-state 9, ports 1, profiles 1, read-configuration 65, secrets 1, trust 1, up 4 = 88
- [X] T008 Add directory-aware case loading for `cases/<area>.json` to `crates/conformance/src/load.rs`, preserving id-sorted deterministic ordering across files — `load_case_files` reads every `cases/*.json` in sorted-filename order, keeps each record's origin file + in-file index (so `check_case_shapes`/`check_allowed_differences` still name a reviewable `cases/<area>.json:records[i]`), then stable-sorts the concatenation by case id. The 24 fixture registries under `fixtures/conformance/` moved to the same layout. `validate`/`certify` still exit 0
- [X] T009 Add a hermetic guard test in `crates/conformance/tests/registry_valid.rs` asserting the loaded case count is exactly 88 after migration, so the split cannot silently lose records — `every_migrated_case_survives_the_per_area_split` asserts the count against `MIGRATED_CASE_COUNT = 88`, that the concatenated set is id-sorted, and that no two records share an id across files
- [X] T010 Add the `scenario_context: IndexMap<String, String>` field to `TestCase` in `crates/conformance/src/model.rs` with `#[serde(default, skip_serializing_if = "IndexMap::is_empty")]` — round-trips as `scenarioContext` via the struct-level `rename_all`; a sibling of `context` (environment), never a replacement. Contents are unvalidated here by design: V26/V16 are US1 (T041)
- [X] T011 Include `scenarioContext` in the case-hash input in `crates/conformance/src/case_hash.rs`, so changing what a case exercises re-records its snapshot — added to the explicit allow-list document. **Omitted when the assignment is empty**: an empty map declares nothing, and hashing it would have marked the one committed snapshot (`case-readconfig-snapshot`) stale for a record whose bytes did not change, making staleness mean "the hash function grew a field" instead of "the evidence-determining inputs drifted". Unit tests cover: adding an assignment re-records, changing a value re-records, reordering keys does not, an empty assignment does not. `snapshot check` still reports 1 fresh, 0 stale
- [X] T012 [P] Add the `obl-bhv-`/`obl-cmb-` id constructors to `crates/conformance/src/obligation.rs`, hashing over sorted assignment keys via the existing `hash8` helper per `contracts/obligation.md` — `behavior_obligation_id(behavior, &[Condition])` and `combination_obligation_id(operation, assignment)`. The `hash8` helper is **local**, not the private one in `inventory.rs`/`clause.rs`: those hash a schema pointer and a prose excerpt, so coupling to either would tie obligation identity to an unrelated record's field set. Only the truncation convention (first 8 lowercase-hex of SHA-256) and the `0x1f` separator are shared. `canonical_context` sorts conditions AND each condition's value subset, both of which are sets and so must not perturb the id
- [X] T013 [P] Add a unit test in `crates/conformance/src/obligation.rs` proving two assignments differing only in key order produce the same id — `assignment_key_order_does_not_change_the_combination_id` builds the two assignments from `IndexMap`, which PRESERVES insertion order, so a function that failed to sort internally forks visibly (a `BTreeMap` would have sorted on the caller's behalf and made the test vacuous); it also asserts the fixtures really do iterate differently, and re-checks agreement through a `Vec` in a third order and a `BTreeMap`. Five further tests cover substance-tracking, separator injectivity, cosmetic context reordering, and prefix disjointness — 6/6 pass
- [X] T014 Update the hermetic-conformance convention comment block in `.config/nextest.toml` (currently lines 43–73) to enumerate this feature's new binaries, recording the live/hermetic classification each one gets (Constitution VII; FR-073, FR-074) — documentation only: a 024 section appended to the existing block, forward-referencing binaries later tasks create (the same style the block already uses). No real overrides added; `parity_conformance_docker`'s wiring stays T020/T021's job. Also records that the `coverage` command group is a bin surface, not a test binary. All six profiles still parse (`cargo nextest list --profile <p>` exits 0 for default/dev-fast/full/ci/mvp-integration/parity)
  - **Hermetic, NO override** (run in every profile by the standing convention): `coverage_model`, `obligation_gating`, `workflow_coverage`, `error_path_coverage`, `denormalized_coverage` (conformance crate)
  - **Hermetic with an `fs-heavy` override** mirroring `observation_faults`: `injection_faults`, `prereq_faults` (parity-harness crate)
  - **Live, per-profile overrides in ALL profiles**: `parity_conformance_docker` only (T021)

**Checkpoint A**: US1 and US2 can begin.

### Block B — Live execution infrastructure (blocks US3–US6 only)

- [ ] T015 Reshape `crates/deacon/tests/parity_conformance_runner.rs` from one test function into one driver function per `resourceGroup` value, each filtering the declarative case set by group (research Decision 4)
- [ ] T016 Create `crates/deacon/tests/parity_conformance_docker.rs` driving the Docker-backed resource groups, including the error-path tier
- [ ] T017 Add a per-case `tokio::time::timeout` of 5 minutes to the case loop in `crates/parity-harness/src/runner.rs`, failing loudly with the case id on expiry (FR-077b)
- [ ] T018 Add bounded concurrency via `JoinSet` plus a semaphore to the Docker driver, routing every blocking `docker inspect` through `spawn_blocking` so the executor is never blocked (Principle V)
- [ ] T019 Add the 30-minute tier wall-clock assertion to `crates/deacon/tests/parity_conformance_docker.rs`, reporting elapsed time and the slowest cases on failure (FR-077a, research Decision 10)
- [ ] T020 Register `parity_conformance_docker` in `fixtures/parity-corpus/registry.json` with `docker_required: true`, and correct the stale `docker_required: false` on `parity_conformance_runner`
- [ ] T021 Add `parity_conformance_docker` to the `[profile.parity]` `default-filter` and to the exclusion filters in `[profile.default]`, `[profile.dev-fast]`, `[profile.full]`, `[profile.ci]`, and `[profile.mvp-integration]` in `.config/nextest.toml`
- [ ] T022 Extend `crates/deacon/tests/parity_registry_check.rs` to assert registry ↔ `tests/*.rs` ↔ `.config/nextest.toml` agreement for the new binary and that no `coverage`/`regressions` command leaks into the shipped `deacon` CLI

**Checkpoint B**: US3–US6 can begin.

---

## Phase 3: User Story 1 - See the shape of the hole (Priority: P1) 🎯 MVP

**Goal**: A constrained context model with applicability rules, plus generated reports that
divide the valid combination space into covered and uncovered — delivering value with zero
new conformance cases.

**Independent Test**: Run `coverage report` against the existing, unchanged 88-case set and
confirm it enumerates the valid space, marks what those cases cover, and lists the remainder.

### Tests for User Story 1

- [X] T023 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: the report lists every valid combination and omits every rule-forbidden one (scenario 1) — `the_report_lists_every_valid_combination_and_omits_every_forbidden_one` compares the report against an INDEPENDENT re-enumeration (`brute_force_valid_pairs`, a second reading of the same records) in BOTH directions; calling the production evaluator to check the production evaluator would have been a tautology
- [X] T024 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: an excluded combination appears in neither population and names its excluding rule id (scenario 2) — `an_excluded_combination_is_absent_from_both_populations_and_names_its_rule` pins `read-configuration × sdim-container-state=running`: present in `excluded` with a resolvable rule id, absent from every pair AND from the whole inventory
- [X] T025 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: two generations from an unchanged record are byte-identical (scenario 3, SC-010) — byte-compares two generations of the obligations, all three report JSON documents, and the rendered Markdown
- [X] T026 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: a value permitted by no rule is reported as a dead value (scenario 4, FR-010) — one ADDED rule strands `sdim-features: lockfile` under every operation; asserts both the report's `deadValues` and the V26 violation, and that the unmodified registry has none (so the assertion is not vacuous)
- [X] T027 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: obligations of a modelled-but-inactive environment are enumerated as `inactive-environment`, counted as neither covered nor gap (scenario 5) — a behavior constrained to `dim-runtime: podman` under the active Docker profile buckets `inactive-environment`, claims no evidence, stays IN the denominator, and the six buckets partition the obligation set
- [X] T028 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: marking a fixture profile active re-buckets its obligations with zero changes to model, rules, or cases (scenario 6, SC-015, FR-004b) — the ONLY edit is which profile is `active`; asserts `scenario.json`, `applicability.json`, and every `cases/<area>.json` are byte-identical across the two runs while the obligation re-buckets from `inactive-environment` to `covered`
- [X] T029 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: the full Cartesian product is never materialized — obligation count equals the enumerated valid-pair count, not the product of all dimension sizes (FR-013) — 718 combination obligations vs a 4,800-value Cartesian product; also asserts every combination's pinned-dimension count equals its declared arity
- [X] T030 [P] [US1] Test in `crates/conformance/tests/coverage_model.rs`: `coverage generate` writes **only** `conformance/obligations/obligations.json` — it never modifies a file under `conformance/registry/obligation-dispositions/`, a case, a behavior, a waiver, or a gap. Assert by fingerprinting the registry tree before and after a generation run (FR-018, the invariant `only_the_refresh_bin_writes_committed_snapshots` guards for snapshots) — fingerprints the whole `conformance/` tree before and after a real `coverage generate` subprocess run, so the guarantee covers files the test never thought to name; `obligations/obligations.json` is the single changed path

### Implementation for User Story 1

- [X] T031 [US1] Implement the `sdim-` dimension and `rule-` record models in `crates/conformance/src/scenario.rs`, reusing `model::Condition` verbatim for rule conditions — `ScenarioDimension`/`ApplicabilityRule`/`HighRiskTriple` + the `ApplicabilityFile` two-record-kind document (the `MappingFile` precedent, since `Collection`'s `deny_unknown_fields` admits only `schemaVersion`+`records`). The four new id prefixes also join `RecordType`, so V2's grammar/uniqueness/prefix↔type checks cover them
- [X] T032 [US1] Implement the applicability evaluator in `crates/conformance/src/scenario.rs` per `contracts/scenario-model.md`: a combination is invalid iff some rule's conditions are all satisfied; rules are pure exclusions with no ordering dependence — a rule constrains only the dimensions it names; a PARTIAL combination missing a named dimension is inconclusive, never excluded; a conditionless rule excludes nothing (treating it as excluding everything would turn one malformed record into an empty denominator)
- [X] T033 [US1] Implement dimension-level pruning in `crates/conformance/src/scenario.rs`: a dimension all of whose values are excluded under an operation is inapplicable and contributes no pairs — `applicable_dimensions` returns `(dimension, permitted values)` so the caller never recomputes the filter it just paid for
- [X] T034 [US1] Add loading of `scenario.json` and `applicability.json` to `crates/conformance/src/load.rs` — `scenario.json` via the existing `load_collection`; `applicability.json` via its own `load_applicability_file` (two record kinds in one document). A malformed file yields empty AND a located `SchemaError`, never a silently partial rule set that would quietly widen the valid space
- [X] T035 [US1] Author `conformance/registry/scenario.json` with the six required dimensions and the minimum value sets from data-model.md §1 (FR-003, FR-005 – FR-009) — six dimensions, exactly the FR-005 – FR-009 minimum value sets
- [X] T036 [US1] Author `conformance/registry/applicability.json` with the exclusion rules, each carrying a `ground` — at minimum: no container state for operations that create no container; no Features for `down`/`doctor`; no structured output mode for operations emitting no document — **8 rules**, each grounded in a mechanism. Deliberate divergence from data-model.md §2's illustrative rule recorded in RULES.md: it excludes only the three container-ful states and NOT `none`, because excluding `none` too would leave `read-configuration`/`build`/`doctor` with no valid total assignment at all while data-model.md §3 requires a case to assign every dimension — the operation would become unrepresentable
- [X] T037 [US1] Implement pairwise obligation generation in `crates/conformance/src/obligation.rs` per `contracts/obligation.md`, partitioned by operation, excluding environment dimensions, emitting directly from the two-dimension cross product — 718 pairs. Pair emission also re-checks the 3-element combination, so a future rule naming BOTH pair members is honoured without changing the loop
- [X] T038 [US1] Implement behavior-obligation generation in `crates/conformance/src/obligation.rs` from each behavior's `applicability` — **exactly one obligation per behavior**: an empty applicability is one universal context (zero would erase the behavior from the denominator), and a non-empty one IS the context, because a condition pins a value SUBSET meaning "any of these" — expanding it per value would multiply the two kinds, which FR-019 forbids and research D2 rejected on arithmetic
- [X] T039 [US1] Implement `coverage generate` in `crates/conformance/src/bin/conformance.rs` with atomic temp-file + `fs::rename` write, id-sorted output, and `--out` redirection — V26 runs first and, on any violation, reports and writes NOTHING (contracts/coverage-cli.md "reported before any write"). Default path resolves as a registry sibling via the new `obligations_file_for`, mirroring `clause_paths_for`/`migration_paths_for`
- [X] T040 [US1] Implement `coverage check` in `crates/conformance/src/bin/conformance.rs`, regenerating in memory and byte-comparing, naming the first differing unit id on drift — exit 0 match / 1 drift (naming the first differing unit id and whether added, removed, or changed) / 2 unreadable committed file
- [X] T041 [US1] Implement V26 (scenario-model integrity: dead values, unknown dimension/value in a rule, rule with no ground, partial or invalid case `scenarioContext`) in `crates/conformance/src/validate.rs` as a new function, not inline growth — a free function, not a `Checker` method. Also covers empty/duplicated value sets, missing required dimensions, rule arity <2, and malformed triples. A registry declaring no scenario dimensions opts out entirely, so the pre-024 fixture registries stay silent
- [X] T042 [US1] Implement V27 (obligation provenance: committed ≠ regenerated, revision mismatch, obligation referencing a removed value) in `crates/conformance/src/validate.rs` — runs alongside V14/V17 in `validate_path_with_inventory` (it needs the registry's `obligations/` sibling). A declared model with NO committed inventory is a violation, not a skip: an absent file would otherwise read as "nothing to check"
- [X] T043 [US1] Implement coverage matching in `crates/conformance/src/coverage.rs`: a pair is covered iff a declarative case's `scenarioContext` matches both values under the same operation and the case is executable — a legacy carrier counts only while an open residual names it, so coverage decays with the residual instead of outliving it. Behavior obligations reuse `Coverage::evaluate` rather than a second evaluator, so the two denominators can never disagree
- [X] T044 [P] [US1] Implement the `coverage-pairwise` renderer in `crates/conformance/src/coverage_report.rs` per `contracts/coverage-report.md` §1, including the `excluded` list with rule attribution and the `deadValues` list — buckets are the five FR-026 ones plus `undispositioned` (the honest sixth state until US2's disposition records exist). `excluded` carries single-dimension prunes AND pair-level exclusions, both attributed
- [X] T045 [P] [US1] Implement the `coverage-operations` renderer in `crates/conformance/src/coverage_report.rs` per §3, including `missingInputClasses` and `missingConfigSources` — `inputClasses` are DERIVED (the record has no input-class field until US3); `boundary`/`unsupported` have no derivable signal and are therefore always reported missing. Cases are attributed by the subcommands they actually invoke, which keeps the report informative for the 88 pre-scenario-model cases (read-configuration 65, up 12, exec 4, seven operations at zero)
- [X] T046 [P] [US1] Implement the `coverage-observables` renderer in `crates/conformance/src/coverage_report.rs` per §4, including `channelsBelowFloor` and `unscopedNormalizationRules` — `fields` counts what is COMPARED (a `jsonSubset` leaf, or an allowed-difference path, which exists only because the comparison reaches it), never what is captured. Reports 6 channels below the three-case floor and `chan-file-content` at zero
- [X] T047 [US1] Implement the Markdown renderers in `crates/conformance/src/coverage_report.rs`, rendering from the same ordered in-memory model as the JSON so the two can never disagree — rendered from the same in-memory model in the same order; T025 byte-compares them
- [X] T048 [US1] Implement `coverage report` in `crates/conformance/src/bin/conformance.rs`, read-only with respect to the record, exiting non-zero only on load/model failure (FR-063) — read-only; exit code never reflects coverage (reporting never gates, gating never reports)
- [X] T049 [US1] Add a hermetic determinism test in `crates/conformance/tests/registry_valid.rs` running `coverage check` against the committed obligations — `committed_obligations_match_a_fresh_regeneration` in the existing hermetic PR gate
- [X] T050 [US1] Add the scenario-model and obligation-provenance sections to `conformance/RULES.md`, keeping the `validate.rs` ↔ `RULES.md` lockstep the index exists to make checkable — new `## Scenario model and obligation provenance (V26 – V27)` section at the anchor the T004 index rows already link to
- [X] T051 [US1] Commit the generated `conformance/obligations/obligations.json` and record the resulting obligation count in the PR body — **745 obligations: 718 combination (all arity 2; no `hrt-` triples until US3) + 27 behavior**. `validate` and `certify` both still exit 0, and `certify`'s output is unchanged (0 blocking, 10 waived)

**Checkpoint**: The denominator exists, is deterministic, and the shape of the hole is visible
with zero new cases. This is the MVP.

---

## Phase 4: User Story 2 - Nothing applicable stays unclassified (Priority: P1)

**Goal**: Every applicable obligation carries exactly one disposition, and gaps and expired
waivers block strict certification.

**Independent Test**: Introduce an undispositioned obligation into a fixture registry, confirm
`certify` fails naming it, then apply each of the four dispositions and confirm two permit
release and two block.

### Tests for User Story 2

- [ ] T052 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: an undispositioned obligation fails `certify` naming the obligation, behavior, and context (scenario 1)
- [ ] T053 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: a `gap` disposition blocks `certify` (scenario 2)
- [ ] T054 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: a waiver expiring before `--today` blocks and is named (scenario 3, SC-009)
- [ ] T055 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: unexpired `waived` and `non-testable` do not block and are enumerated (scenario 4)
- [ ] T056 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: two dispositions on one obligation fail validation (scenario 5)
- [ ] T057 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: a filler rationale is rejected (scenario 6, FR-025)
- [ ] T058 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: adding a behavior without dispositioning its generated obligations is rejected (SC-014)
- [ ] T059 [P] [US2] Test in `crates/conformance/tests/obligation_gating.rs`: a disposition whose obligation no longer resolves is reported stale (FR-024)
- [ ] T060 [P] [US2] Create the fixture registries under `crates/conformance/tests/fixtures/obligation-gating/` covering each blocking and non-blocking disposition (FR-078)

### Implementation for User Story 2

- [ ] T061 [US2] Implement the `odp-` disposition record model in `crates/conformance/src/obligation.rs` with deserialize-time exactly-one-of enforcement across `cases`/`rationale`/`waiver`/`gap`
- [ ] T062 [US2] Add loading of `conformance/registry/obligation-dispositions/<area>.json` to `crates/conformance/src/load.rs`
- [ ] T063 [US2] Implement disposition resolution in `crates/conformance/src/obligation.rs`: explicit only, no inheritance, no default
- [ ] T064 [US2] Implement V28 (zero or >1 dispositions on an applicable obligation) in `crates/conformance/src/validate.rs`
- [ ] T065 [US2] Implement V29 (filler rationale, triple dispositioned by rationale/waiver, stale disposition) in `crates/conformance/src/validate.rs`, reusing the ground-naming test V23 already applies to `outOfScopeRationale`, and reject a `waived` disposition whose backing waiver scope is blanket or bare-channel rather than specific observable content (FR-023, mirroring the FR-032 rule V19 already enforces)
- [ ] T066 [US2] Add `BlockingKind::Obligation` with a `code` field to `crates/conformance/src/certify.rs`, mirroring the existing `Constraint`/`Clause` shape so the output format does not fork
- [ ] T067 [US2] Wire V28/V29 and expired-waiver dispositions into the blocking set in `crates/conformance/src/certify.rs`
- [ ] T068 [US2] Add the five-bucket reporting (`covered`/`waived`/`non-testable`/`gap`/`inactive-environment`) to `crates/conformance/src/certify.rs`, never folded together (FR-026)
- [ ] T069 [US2] Implement `coverage scaffold` in `crates/conformance/src/bin/conformance.rs`, emitting `UNREVIEWED`-sentinel skeletons to **stdout** only, never writing the registry
- [ ] T070 [US2] Add loader rejection of the `UNREVIEWED` sentinel in `crates/conformance/src/load.rs`, so a scaffold committed unedited fails
- [ ] T071 [US2] Author the initial `conformance/registry/obligation-dispositions/<area>.json` files, dispositioning every obligation generated in T051
- [ ] T072 [US2] Add the obligation-disposition sections to `conformance/RULES.md`, including the gap-vs-waiver-vs-rationale distinction and the triple restriction

**Checkpoint**: Undispositioned work now blocks the release. US1 + US2 are the complete
hermetic foundation.

---

## Phase 5: User Story 3 - Deterministic coverage of the shared consumer workflow (Priority: P2)

**Goal**: Deterministic cases across the whole consumer workflow, spanning all five input
classes, for all ten operations.

**Independent Test**: Run the case set against an unmodified tree and confirm it passes; then
perturb one workflow stage and confirm failure attributed to that stage.

**Depends on**: Phase 2 Block B (T015–T022).

### Tests for User Story 3

- [ ] T073 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: every case reaches a definite verdict and none is skipped, ignored, or conditionally excluded (scenario 1, SC-012)
- [ ] T074 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: each workflow stage shows ≥1 valid-behavior case and ≥1 case per permitted input class (scenario 2, FR-040)
- [ ] T075 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: a reference-lenient case is paired with a spec-expectation twin pinning the direction (scenario 3, FR-043)
- [ ] T076 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: an operation with no pinned-reference equivalent uses `spec-expectation` and the report states the substitution (scenario 4, Assumption 5)
- [ ] T077 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: a case whose observable output could vary between runs produces the same verdict on repeated execution, asserted per case rather than only in aggregate (scenario 5)
- [ ] T078 [P] [US3] Test in `crates/conformance/tests/workflow_coverage.rs`: every high-risk triple is covered by an executable case, never by rationale or waiver (SC-003, FR-015)

### Implementation for User Story 3

- [ ] T079 [US3] Author the `hrt-` high-risk triples in `conformance/registry/applicability.json`, at least twelve, each with a `reason` (SC-003, FR-014, FR-016)
- [ ] T080 [US3] Implement the `coverage-triples` renderer in `crates/conformance/src/coverage_report.rs` per `contracts/coverage-report.md` §2, carrying `reason` into the report
- [ ] T081 [US3] Extend V29 in `crates/conformance/src/validate.rs` to reject a triple dispositioned by rationale or waiver
- [ ] T082 [P] [US3] Add configuration-discovery and JSON/JSONC parsing cases to `conformance/registry/cases/read-configuration.json` with fixtures under `conformance/fixtures/` (FR-027, FR-028)
- [ ] T083 [P] [US3] Add variable-substitution and validation-timing cases to `conformance/registry/cases/read-configuration.json` (FR-029, FR-030)
- [ ] T084 [P] [US3] Add extends-chain and merge-precedence cases, including conflicts, cycles, and missing parents, to `conformance/registry/cases/read-configuration.json` (FR-031)
- [ ] T085 [P] [US3] Add Feature resolution and install-order cases — declared order, dependency-derived order, CLI override — to `conformance/registry/cases/up.json` (FR-032)
- [ ] T086 [P] [US3] Add lockfile production and consumption cases to `conformance/registry/cases/up.json` and `conformance/registry/cases/outdated.json` (FR-033)
- [ ] T087 [P] [US3] Add image-reference and Dockerfile build cases, including build arguments and build-time failure, to `conformance/registry/cases/build.json` (FR-034)
- [ ] T088 [P] [US3] Add Compose cases, including multi-service shapes and created project resources, to `conformance/registry/cases/up.json` (FR-035)
- [ ] T089 [P] [US3] Add container creation and setup cases covering identity, labels, mounts, environment, and user to `conformance/registry/cases/up.json` (FR-036)
- [ ] T090 [P] [US3] Add lifecycle-execution cases covering hook ordering, Feature-contributed hooks, and hook failure to `conformance/registry/cases/run-user-commands.json` (FR-037)
- [ ] T091 [P] [US3] Add restart and resume cases distinguishing first creation from re-entry to `conformance/registry/cases/up.json` using `invariant-metamorphic` (FR-038)
- [ ] T092 [P] [US3] Add exec, outdated, upgrade, down, and cleanup cases — including resources each removes and leaves behind — to the matching `conformance/registry/cases/<area>.json` files (FR-039)
- [ ] T093 [P] [US3] Add `templates apply` and `doctor` cases to `conformance/registry/cases/templates.json` and `conformance/registry/cases/doctor.json` (FR-005)
- [ ] T094 [US3] Add the behaviors these cases require to `conformance/registry/behaviors/<area>.json`, each traced to a named source unit per research Decision 9, and disposition their generated obligations
- [ ] T095 [US3] Re-review the `non-testable` clause classifications whose ground was the absence of a container-backed tier, updating `conformance/registry/clause-classifications/*.json` where the ground no longer holds

**Checkpoint**: All ten operations carry cases across the permitted input classes.

---

## Phase 6: User Story 4 - Parity does not stop at acceptance (Priority: P2)

**Goal**: A container-backed error-path tier whose cases begin from inputs configuration read
accepts on both sides.

**Independent Test**: Take an input both sides accept at read time, run the tier, and confirm
it reports the later-stage outcome for both sides with a definite verdict.

**Depends on**: Phase 2 Block B (T015–T022).

### Tests for User Story 4

- [ ] T096 [P] [US4] Test in `crates/deacon/tests/parity_conformance_docker.rs`: an error-path case records the failing stage and each side's outcome (scenario 1)
- [ ] T097 [P] [US4] Test in `crates/parity-harness/tests/prereq_faults.rs`: absent Docker or a mismatched oracle fails with a cause-specific error, never a skip or a pass (scenario 2, FR-044)
- [ ] T098 [P] [US4] Test in `crates/deacon/tests/parity_conformance_docker.rs`: every container, network, volume, and temp dir is reclaimed on success and on unwind (scenario 3, FR-045)
- [ ] T099 [P] [US4] Test in `crates/deacon/tests/parity_conformance_docker.rs`: two concurrent cases observe none of each other's resources (scenario 4)
- [ ] T100 [P] [US4] Test in `crates/conformance/tests/error_path_coverage.rs`: no error-path case reaches its verdict at configuration read (SC-007)

### Implementation for User Story 4

- [ ] T101 [US4] Add the `errorPathTier` marker and failure-stage assertion to the declarative case shape in `crates/conformance/src/model.rs`, reusing the existing `expect_failure_phase` rather than adding a predicate
- [ ] T102 [US4] Extend V16 in `crates/conformance/src/validate.rs` to require an error-path case to declare a later-stage failure phase and to reject one whose verdict is reachable at configuration read
- [ ] T103 [P] [US4] Add build-stage error-path cases to `conformance/registry/cases/build.json` with pinned image inputs (FR-042, FR-046)
- [ ] T104 [P] [US4] Add container-creation error-path cases to `conformance/registry/cases/up.json`
- [ ] T105 [P] [US4] Add Feature-installation error-path cases to `conformance/registry/cases/up.json`
- [ ] T106 [P] [US4] Add lifecycle-execution error-path cases to `conformance/registry/cases/run-user-commands.json`
- [ ] T107 [P] [US4] Add teardown error-path cases to `conformance/registry/cases/down.json`
- [ ] T108 [US4] Add direction-pinning companion cases for every error-path case whose sides disagree, so the differential is never the only evidence (FR-043, the 023 T074 lesson)
- [ ] T109 [US4] Verify V18 rejects any unpinned image among the new fixtures, and pin any that fail

**Checkpoint**: Parity no longer stops where the reference is most lenient.

---

## Phase 7: User Story 5 - Fields that broad normalization used to hide (Priority: P2)

**Goal**: The twelve named fields are compared, not merely captured.

**Independent Test**: For each named field, confirm a case observes it and that changing only
that field makes the case fail.

**Depends on**: Phase 2 Block B (T015–T022).

### Tests for User Story 5

- [ ] T110 [P] [US5] Test in `crates/conformance/tests/denormalized_coverage.rs`: every named field appears in `denormalizedFields` with ≥1 covering case (scenario 1, SC-008)
- [ ] T111 [P] [US5] Test in `crates/parity-harness/tests/normalize_consistency.rs`: null, empty, and omitted produce three distinguishable observations (scenario 2, FR-055)
- [ ] T112 [P] [US5] Test in `crates/deacon/tests/parity_conformance_docker.rs`: array-form and object-form lifecycle hooks are both observed (scenario 3)
- [ ] T113 [P] [US5] Test in `crates/parity-harness/tests/normalize_consistency.rs`: an unscoped normalization rule is rejected (scenario 4, V24, FR-056)
- [ ] T114 [P] [US5] Test in `crates/conformance/tests/denormalized_coverage.rs`: an ambiguous Feature install order is either pinned or dispositioned (scenario 5)

### Implementation for User Story 5

- [ ] T115 [P] [US5] Add lifecycle array-vs-object and command/entrypoint cases to `conformance/registry/cases/up.json`, including entrypoints chained by multiple Features (FR-047, FR-048)
- [ ] T116 [P] [US5] Add environment merge-precedence and PATH-construction cases to `conformance/registry/cases/up.json` (FR-049, FR-050)
- [ ] T117 [P] [US5] Add user and UID/GID cases, including a non-root user and a Feature-created user, to `conformance/registry/cases/up.json` (FR-051)
- [ ] T118 [P] [US5] Add metadata-label namespace cases, including labels one side emits and the other does not, to `conformance/registry/cases/up.json` (FR-052)
- [ ] T119 [P] [US5] Add mount and mount-source cases distinguishing a differing source path from a differing mount shape to `conformance/registry/cases/up.json` (FR-053)
- [ ] T120 [P] [US5] Add network and Compose project-resource cases to `conformance/registry/cases/up.json` (FR-054)
- [ ] T121 [P] [US5] Add null/empty/omitted cases to `conformance/registry/cases/read-configuration.json` (FR-055)
- [ ] T122 [US5] Add the derived observer fields these comparisons need to `crates/parity-harness/src/observe/`, never a predicate or query in the assertion language (the 023 hard line)
- [ ] T123 [US5] Audit `crates/parity-harness/src/normalize.rs` for rules that remove or collapse observable content; narrow or retire each, and bump `NORMALIZER_VERSION` in `crates/conformance/src/snapshot.rs`
- [ ] T124 [US5] Refresh the committed snapshots invalidated by the normalizer bump via `cargo run -p parity-harness --bin conformance-snapshot refresh`, reviewing the resulting git diff as the review surface
- [ ] T125 [US5] Characterize any divergence the de-suppression surfaces: record the behavior with all three axes, add a scoped `wvr-` or an `ext-` record, and file a `parity-drift` issue cross-linked from the behavior's `notes` for the fix-flavored ones (spec Assumption 2)

**Checkpoint**: The fields where false equivalence survives longest are now compared.

---

## Phase 8: User Story 6 - Prove the suite can fail (Priority: P3)

**Goal**: Each observable channel is demonstrably live.

**Independent Test**: Run the injected-regression harness and confirm every channel has ≥1
detected regression and that `inertCount` is zero.

**Depends on**: Phase 2 Block B, and on US3–US5 for the cases that do the detecting.

### Tests for User Story 6

- [ ] T126 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: an injected regression makes the suite fail with the failure naming the channel (scenario 1)
- [ ] T127 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: a channel with no detecting regression is reported inert and fails the run (scenario 2, FR-067)
- [ ] T128 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: the tree is unmodified after a run, including after an unwind (scenario 3, FR-066)
- [ ] T129 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: the detected/inert classification is identical across two runs (scenario 4, FR-069)
- [ ] T130 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: a **dead observer** — one returning a constant regardless of input — is correctly reported inert, proving the injection point is upstream of the observer (research Decision 5, FR-065b)
- [ ] T131 [P] [US6] Test in `crates/parity-harness/tests/injection_faults.rs`: an ordinary run of either conformance driver cannot apply a regression — the injector is unreachable outside the `coverage-regressions` bin, asserted structurally rather than by convention (FR-070)

### Implementation for User Story 6

- [ ] T132 [US6] Implement the `reg-` record model in `crates/conformance/src/regression.rs` with the closed perturbation-kind set from `contracts/regression-harness.md`
- [ ] T133 [US6] Implement V30 (declared channel with no record; record targeting a channel with no observer) in `crates/conformance/src/validate.rs`
- [ ] T134 [US6] Implement evidence-source-boundary injection in `crates/parity-harness/src/inject.rs`, applying perturbations to the raw captured artifact before any observer runs
- [ ] T135 [US6] Add a compile-time or runtime guard in `crates/parity-harness/src/inject.rs` preventing injection into an observer's returned `RawChannelEvidence` (FR-065b)
- [ ] T136 [US6] Implement apply/revert with unwind safety in `crates/parity-harness/src/inject.rs` using an RAII guard, mirroring the existing workspace cleanup guard
- [ ] T137 [US6] Implement the `coverage-regressions` bin in `crates/parity-harness/src/bin/coverage-regressions.rs`, writing the byte-stable `target/conformance/regressions.json` and exiting non-zero on any inert channel
- [ ] T138 [US6] Author `conformance/registry/regressions.json` with ≥1 record per declared channel, covering all eleven (FR-065)
- [ ] T139 [US6] Add `make test-parity-regressions` to the `Makefile` and wire the run into `.github/workflows/parity.yml`

**Checkpoint**: Every channel is proven able to fail. The green results of US1–US5 now mean
something.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [ ] T140 [P] Update the "Conformance Registry" and "Parity & Conformance" sections of `CLAUDE.md` with the scenario model, obligation kinds, and V26–V30
- [ ] T141 [P] Add the coverage workflows to `conformance/RULES.md`: the drift workflow, the five reporting buckets, and the reporting-never-gates rule
- [ ] T142 [P] Verify SC-005 by confirming `channelsBelowFloor` is zero in `target/conformance/coverage-observables.json`
- [ ] T143 [P] Verify SC-006 by confirming `inertCount` is zero in `target/conformance/regressions.json` and that every one of the eleven declared channels appears with verdict `detected`
- [ ] T144 [P] Verify SC-004 by confirming `missingConfigSources` is empty for all ten operations
- [ ] T145 Verify SC-011 by running the hermetic set ten consecutive times and the live set three consecutive times, confirming identical verdicts and zero flakes
- [ ] T146 Verify SC-002 and SC-001 by confirming `summary.undispositioned` is zero in `target/conformance/coverage-pairwise.json`
- [ ] T147 Run the full quality gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`, `validate`, `certify`
- [ ] T148 Walk `specs/024-deterministic-conformance-coverage/quickstart.md` end to end and correct any drift between it and the shipped commands

---

## Deferred Work

Per the constitution's Deferral Tracking rule, every deferral recorded in `research.md` or
`spec.md` gets an entry here with acceptance criteria. A specification is **not** complete
while deferred tasks remain unresolved.

- [ ] T149 [Deferral] Activate a second environment profile (alternative runtime and/or non-Linux platform) per spec Assumption 10
  - **Decision**: spec FR-004a/FR-004b, clarification session Q1 — model now, activate later
  - **Rationale**: activating a runtime lane multiplies live cost; the model keeps the backlog visible in the meantime via the `inactive-environment` bucket
  - **Acceptance**: marking the profile `active` in `conformance/registry/profiles.json` re-buckets its obligations with **zero** changes to `scenario.json`, `applicability.json`, or any case (SC-015 already tests this against a fixture; this task does it for real)
- [ ] T150 [Deferral] Re-review the remaining `non-testable` clause classifications not covered by T095
  - **Decision**: research Decision 9 — the 156 `non-testable` clauses are the real reservoir of new behaviors; only those blocked by the absent container tier are in scope for T095
  - **Acceptance**: every remaining `non-testable` classification either keeps its ground with a restated rationale, or becomes `behavior-mapped` with a behavior and dispositioned obligations

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational Block A (T007–T014)**: blocks **all** user stories
- **Foundational Block B (T015–T022)**: blocks **US3–US6 only** — not required for the MVP
- **US1 (Phase 3)**: after Block A
- **US2 (Phase 4)**: after Block A; independent of US1 in principle, but its fixtures are far
  easier to author once US1's generator exists
- **US3, US4, US5 (Phases 5–7)**: after Block A + Block B; mutually independent
- **US6 (Phase 8)**: after Block B, and needs US3–US5's cases to have something to detect with
- **Polish (Phase 9)**: after all desired stories

### Critical Path

```text
T007 → T008 → T010 → T012 → [US1: T031 → T037 → T039 → T044 → T048] → [US2: T061 → T064 → T066]
                          └→ T015 → T017 → T020 → [US3/US4/US5 cases] → [US6: T134 → T137]
```

### Ordering Rules That Are Not Negotiable

1. **T007 lands alone.** The `cases.json` split is a mechanical migration that can silently
   lose records; T009's count assertion is the guard, and folding a content change into the
   same commit hides it.
2. **T064/T065 (V28/V29) land before T094.** The guard against diluting the denominator must
   exist before behaviors are added to it, or the first bulk addition slips through
   undispositioned.
3. **T123 before T124.** Refreshing snapshots before narrowing the normalizer records the
   suppressed values as if they were reviewed.
4. **T108 before US4 is considered complete.** A differential alone proves disagreement, not
   direction.

### Parallel Opportunities

- Phase 1: T002–T006 all parallel
- US1 tests: T023–T030 all parallel
- US1 renderers: T044, T045, T046 parallel (distinct functions, one file — serialize the
  final commit)
- US2 tests: T052–T060 all parallel
- US3 case authoring: T082–T093 all parallel (distinct `cases/<area>.json` files, which is
  precisely what research Decision 7's split buys)
- US4 case authoring: T103–T107 all parallel
- US5 case authoring: T115–T121 all parallel
- US3, US4, US5 can proceed concurrently by different people once Block B lands

---

## Parallel Example: User Story 3 case authoring

```bash
# Distinct files, no shared state — the payoff from splitting cases.json by area:
Task: "Add Feature install-order cases to conformance/registry/cases/up.json"
Task: "Add build cases to conformance/registry/cases/build.json"
Task: "Add lifecycle cases to conformance/registry/cases/run-user-commands.json"
Task: "Add teardown cases to conformance/registry/cases/down.json"
Task: "Add templates apply cases to conformance/registry/cases/templates.json"
```

---

## Implementation Strategy

### MVP (US1 only)

1. Phase 1 Setup
2. Phase 2 **Block A only** — skip Block B entirely
3. Phase 3 US1
4. **STOP and VALIDATE**: `coverage report` against the unchanged 88-case set enumerates the
   valid space and lists the remainder

The MVP delivers the spec's stated highest-value increment — a measured, reviewable backlog —
with zero new conformance cases and no Docker.

### Incremental Delivery

1. Setup + Block A → hermetic foundation
2. US1 → the hole is visible (MVP)
3. US2 → the hole blocks the release
4. Block B → live infrastructure can scale
5. US3 → the workflow is covered
6. US4 → parity survives past configuration read
7. US5 → suppressed fields are compared
8. US6 → the suite is proven able to fail

Each step is its own small, CI-gated PR with a Conventional-Commit title
(`feat`/`fix`/`chore` — never `test`/`style`, which the PR-title check rejects).

---

## Notes

- **Every task is a data or code edit in a named file.** Adding a case, assertion, fixture,
  dimension value, or applicability rule requires **zero** new test functions (SC-013) — the
  only new test functions in this list are the acceptance tests FR-071 mandates and the
  per-resource-group drivers of research Decision 4.
- **No `#[ignore]`, no env-var opt-in, no silent skip** anywhere (FR-075). A missing
  prerequisite fails loudly.
- Live tasks run only under `--profile parity`; hermetic tasks run on every PR.
- Commit after each task or logical group; stop at any checkpoint to validate independently.
