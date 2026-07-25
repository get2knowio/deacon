# Tasks: Migrate Parity Assets into the Declarative Conformance System

**Input**: Design documents from `/specs/023-migrate-parity-to-conformance/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: **REQUIRED.** FR-046 mandates automated acceptance tests for seven areas and FR-047 requires each to be *demonstrated to fail* on the violation it guards. Test tasks are therefore first-class, not optional.

**Organization**: Grouped by user story so each is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US7, mapping to spec.md user stories

## Path Conventions

Rust workspace. Hermetic tooling in `crates/conformance/` (package `deacon-conformance`), live tooling in `crates/parity-harness/`, test binaries in `crates/deacon/tests/`, committed data under `conformance/`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the data locations and module skeletons every story depends on.

- [X] T001 Create the committed data directory `conformance/migration/` with a `README.md` stating that `baseline.json` is machine-owned (regenerate, never hand-edit) and `mapping.json` is hand-authored (generation never writes it), per data-model.md §"Ownership rule"
- [X] T002 [P] Add `pub mod baseline;`, `pub mod mapping;`, `pub mod conservation;`, `pub mod residual;` to `crates/conformance/src/lib.rs` with empty module files, keeping module names distinct from the existing `coverage.rs` (behavior coverage) per contracts/cli-commands.md
- [X] T003 [P] Add `pub mod equivalence;` to `crates/parity-harness/src/lib.rs` with an empty module file
- [X] T004 [P] Add `target/conformance/` and `target/parity/` derived-report paths to `.gitignore` if not already covered

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Shared types, loaders, and the CLI skeleton that every story needs.

**⚠️ CRITICAL**: No user story work can begin until this phase completes.

- [X] T005 Define `BaselineUnit`, `UnitCategory`, and the `BaselineFile` envelope (serde, `IndexMap` for order) in `crates/conformance/src/baseline.rs` per data-model.md §1
- [X] T006 [P] Define `MigrationMapping`, `Disposition`, and `FixtureMapping` types in `crates/conformance/src/mapping.rs` per data-model.md §2
- [X] T007 [P] Define `ResidualRecord` and its loader in `crates/conformance/src/residual.rs` per data-model.md §3, with `missingCapability` and `followUp` non-empty at deserialize time (FR-013, FR-055)
- [X] T008 [P] Define `EquivalenceEntry` and `Relation` types in `crates/parity-harness/src/equivalence.rs` per data-model.md §5
- [X] T009 Extend the `Registry` loader in `crates/conformance/src/load.rs` to load `residuals.json` and the two `conformance/migration/*.json` files, failing loud with cause-specific errors on malformed input (Constitution IV)
- [X] T010 Add the `baseline` and `migration` command groups to `crates/conformance/src/bin/conformance.rs` following the existing `snapshot`/`clause`/`inventory` two-level pattern, per contracts/cli-commands.md
- [X] T011 Add nextest group overrides for this feature's new hermetic test binaries under `crates/conformance/tests/` and `crates/parity-harness/tests/` to **all** profiles in `.config/nextest.toml`, assigning no Docker group so they run in `dev-fast` and gate every change (Constitution VII, FR-048)
- [X] T012 Add an atomic strict-JSON writer helper (temp file + `fs::rename`, byte-stable, sorted keys) in `crates/conformance/src/baseline.rs`, reusing the existing snapshot write pattern rather than introducing a second one

> **T010 note (as implemented)**: only the `baseline` group was added. The `migration`
> group is added by the tasks that give it working handlers — `migration scaffold` in
> T034 and `migration report` / `migration check` in T073 — rather than shipping a
> subcommand that returns "not implemented" (constitution IV: no silent no-ops).
>
> **T002 note (as implemented)**: a fifth module, `crates/conformance/src/parity_corpus.rs`,
> was added alongside the four named ones. It holds the parity-registry data model and
> the **production** corpus discovery functions, moved down from
> `parity_harness::registry` (which now re-exports them) because `parity-harness`
> depends on `deacon-conformance`, so the hermetic baseline enumerator could not
> otherwise call the same discovery the live runners execute — the D1 requirement.

**Checkpoint**: Types, loaders, and CLI skeleton exist — user stories can begin.

---

## Phase 3: User Story 1 - Establish and freeze the migration baseline (Priority: P1) 🎯 MVP

**Goal**: A committed, deterministic, mechanically enumerated inventory of all 111 executable baseline units plus 33 recorded-only external entries, with a drift check that fails naming any changed item.

**Independent Test**: Run `baseline generate` on the unmodified repo, then `baseline check` twice — output is byte-identical and the check is clean. Add a parity test function, re-run `baseline check` — it fails naming that unit.

### Tests for User Story 1 ⚠️ Write FIRST, confirm they FAIL

- [X] T013 [P] [US1] Determinism test — `baseline generate` twice on identical input yields byte-identical output — in `crates/conformance/tests/baseline_determinism.rs` (FR-003, SC-012)
- [X] T014 [P] [US1] Drift-detection test — mutate a fixture copy of the baseline (add/remove/change one unit) and assert `baseline check` fails naming that specific unit and its change kind — in `crates/conformance/tests/baseline_drift.rs` (FR-004, FR-047)
- [X] T015 [P] [US1] Enumeration-source test — assert the Tier-1 count comes from `discover_tier1_cases` and equals **24**, not a directory listing, guarding research D1's off-by-one — in `crates/conformance/tests/baseline_enumeration.rs` (FR-049)

### Implementation for User Story 1

- [X] T016 [US1] Implement live-per-case enumeration in `crates/conformance/src/baseline.rs`: read `fixtures/parity-corpus/registry.json` and call the production discovery functions for the corpus carriers, per contracts/baseline-inventory.md §"Enumeration rules" (FR-001, FR-002)
- [X] T017 [US1] Implement scenario-program case-id extraction for `parity_read_configuration` (2), `parity_exec` (4), `parity_build` (6), `parity_up_exec` (1), `parity_observable_state` (7), `parity_state_diff` (8) in `crates/conformance/src/baseline.rs`, sourced from each program's declared case constants (FR-002, FR-049)
- [X] T018 [US1] Implement hermetic-guard enumeration (one unit per test function: `parity_harness_faults` 10, `parity_registry_check` 6) and internal-consistency enumeration (`consistency_env_probe_flag` 2, `consistency_remote_env_flags` 2) in `crates/conformance/src/baseline.rs` (FR-002, FR-049)
- [X] T019 [US1] Implement `external-corpus-entry` enumeration for the 33 pinned manifest entries in `crates/conformance/src/baseline.rs` per research D8 (FR-002)
- [X] T020 [US1] Implement `baseline generate [--freeze <sha>] [--force]` in `crates/conformance/src/bin/conformance.rs`, refusing to overwrite a frozen baseline without `--force` (FR-045, FR-052)
- [X] T021 [US1] Implement `baseline check` (recompute in memory, byte-compare, never write) in `crates/conformance/src/bin/conformance.rs`
- [X] T022 [US1] Add violation class **V25** (baseline provenance: committed ≠ regenerated, or `revision` mismatch) to `crates/conformance/src/validate.rs` and document it in `conformance/RULES.md` in the same change (RULES.md/validate.rs lockstep)
- [X] T023 [US1] Run `baseline generate --freeze $(git rev-parse --short HEAD)` and commit `conformance/migration/baseline.json`; author each unit's `assertion` (one sentence, immutable thereafter per contracts/baseline-inventory.md), `errorPath`, `channels`, and `diffClasses` fields
- [X] T024 [US1] Correct the stale Tier-1 corpus counts in `fixtures/parity-corpus/README.md` and `REPORT.md` to 24, treating the enumerated baseline as authoritative (FR-005)

**Checkpoint**: The baseline is frozen, committed, and drift-gated. Every later claim is measurable against it.

---

## Phase 4: User Story 2 - Migrate every case with stable identity and complete mapping (Priority: P1)

**Goal**: Every baseline unit maps to a case, a deduplication, a residual, or an explicit retirement — with one-to-one fixture correspondence and orphans impossible in both directions.

**Independent Test**: Map one carrier's units end-to-end; `validate` passes. Delete one mapping entry — V21 fires naming the orphan unit. Point two `fixtureMapping.from` entries at one `to` — V22 fires.

### Tests for User Story 2 ⚠️ Write FIRST, confirm they FAIL

- [X] T025 [P] [US2] V21 orphan test — an unmapped baseline unit and a mapped-but-nonexistent case id each fail validation naming the item, in `crates/conformance/tests/mapping_orphans.rs` (FR-011, FR-047)
- [X] T026 [P] [US2] V22 fixture test — a merged (two `from` → one `to`), split, and dropped fixture each fail validation, **and** a migrated fixture referenced by no case fails as an orphan, in `crates/conformance/tests/mapping_fixtures.rs` (FR-010, FR-012, FR-047)
- [X] T027 [P] [US2] V23 residual well-formedness test — missing `followUp`, vague `missingCapability`, and unresolvable `blockedCarrier` each fail, in `crates/conformance/tests/residual_validation.rs` (FR-055, FR-047)
- [X] T028 [P] [US2] Identity stability test — editing a case's `notes` changes neither its id nor its committed snapshot provenance, in `crates/conformance/tests/case_identity_stable.rs` (FR-007, FR-050)
- [X] T029 [P] [US2] Exception-migration test — a baseline exception mapped to zero or to more than one mechanism fails, and a migrated exception whose tolerated scope or direction is broader than its pre-migration form fails — in `crates/conformance/tests/exception_migration.rs` (FR-027, FR-051, FR-047)

### Implementation for User Story 2

- [X] T030 [US2] Implement mapping resolution and bidirectional orphan detection in `crates/conformance/src/mapping.rs` (unit→case and case→unit) (FR-006, FR-011)
- [X] T031 [US2] Implement one-to-one `fixtureMapping` verification **and** unreferenced-fixture orphan detection in `crates/conformance/src/mapping.rs` (FR-010, FR-012)
- [X] T032 [US2] Add violation classes **V21** (orphan, both directions), **V22** (fixture correspondence and unreferenced fixtures), **V23** (malformed residual — including `blockedCarrier` absent on a non-`external-corpus-entry` residual) to `crates/conformance/src/validate.rs` and to `conformance/RULES.md` in lockstep
- [X] T033 [US2] Validate that every migrated case resolves to at least one behavior **and** at least one observable channel, with dangling identifiers rejected, in `crates/conformance/src/validate.rs` (FR-008, FR-009, SC-002)
- [X] T034 [US2] Implement `migration scaffold` in `crates/conformance/src/bin/conformance.rs` — emit skeleton mapping/residual records to **stdout** with `"UNREVIEWED"` sentinels the loader rejects; never writes the registry
- [X] T035 [US2] Make `certify` in `crates/conformance/src/certify.rs` list the residual queue as **non-blocking** information, keeping gaps blocking (FR-054)
- [X] T036 [P] [US2] Migrate the 9 error-corpus units: move fixtures `fixtures/parity-corpus/errors/*` → `conformance/fixtures/`, author declarative cases in `conformance/registry/cases.json`, and record the 1:1 `fixtureMapping` (lowest risk — already 1:1 today per research D2)
- [X] T037 [US2] Migrate the 2 `parity_read_configuration` units: fixtures `fixtures/config/{basic,with-variables}` → `conformance/fixtures/`, cases authored, mapping recorded
- [X] T038 [US2] Migrate the 24 `parity_corpus_tier1` units: fixtures → `conformance/fixtures/`, one declarative `read-configuration` case per corpus case, mapping recorded
- [X] T039 [US2] Migrate the 24 `parity_corpus_merged` units as **variants** of the tier-1 behaviors (`--include-merged-configuration` mode) in `conformance/registry/cases.json` + `conformance/migration/mapping.json`, coordinating with T049 so the behavior denominator does not grow
- [X] T040 [US2] Migrate the 4 `parity_exec` and 1 `parity_up_exec` units into `conformance/registry/cases.json`, converting their inline code-authored fixtures into directories under `conformance/fixtures/` with correspondence recorded in `conformance/migration/mapping.json`
- [X] T041 [US2] Migrate the 6 `parity_build` units into `conformance/registry/cases.json` with fixtures under `conformance/fixtures/`; record any unit needing image-discovery-by-label in `conformance/registry/residuals.json` rather than forcing it
- [X] T042 [US2] Author residual records in `conformance/registry/residuals.json` for every unit that cannot be expressed as data — expected to concentrate in `parity_state_diff` (8) and `parity_observable_state` (7) per research D4 — each naming a specific missing capability and a tracked follow-up issue
- [X] T043 [US2] Author the 33 `external-corpus-entry` residual record(s) in `conformance/registry/residuals.json` per research D8 (no `blockedCarrier` — they block no program), so the manifest is inventoried without ever counting as migrated
- [X] T044 [US2] Map all 16 baseline characterized exceptions (10 `wvr-` + 6 `ext-`) to exactly one preserved mechanism each under `conformance/registry/`, recording every correspondence in `conformance/migration/mapping.json` — mechanisms are never merged (FR-024, FR-051)
- [X] T045 [US2] Implement direction-and-scope preservation for migrated exceptions in `crates/conformance/src/mapping.rs` — an exception tolerating a broader difference than its pre-migration form fails validation (FR-027)
- [X] T046 [US2] Confirm migrated exceptions remain self-invalidating by extending the stale-detection coverage in `crates/parity-harness/src/waiver.rs`, so an exception whose difference stops reproducing is reported stale rather than silently retained (FR-026)
- [X] T047 [US2] Explicitly disposition any baseline exception with no post-migration counterpart concept in `conformance/registry/`, never dropping it (FR-028)
- [X] T048 [US2] Extend the legacy-location scan in `crates/deacon/tests/parity_registry_check.rs` to assert characterized exceptions resolve from exactly one authoritative location, so a second source cannot be reintroduced (FR-025)

> **T041 note (as implemented)**: all 6 `parity_build` units are recorded as residuals
> under two capability-specific records (`res-build-image-discovery`,
> `res-build-tolerant-outcome`), not migrated. `chan-image` inspects the container a case
> created and `build` produces an image with no container, so image-discovery-by-label is
> unobservable; and three units assert a JSON result shape that must hold whether the
> operation succeeds or fails, which no single assertion expresses. Migrating them at
> reduced fidelity would be a silently more-permissive replacement. Tracked as T108.
>
> **T040 note (as implemented)**: 3 of 4 `parity_exec` units and the 1 `parity_up_exec`
> unit are migrated; `parity_exec::env-propagation` is a residual
> (`res-exec-per-side-argv`) because it compares deacon's `--remote-env` against the
> reference's `--env` and a declarative operation carries ONE argv both sides run.
> Tracked as T107.
>
> **T036 note (as implemented)**: the 5 `deacon-stricter` error-corpus cases are
> `spec-expectation`, not `live-differential`. `chan-exit-code` evidence is a SCALAR, so a
> differential divergence there yields the bare channel path, and a bare-channel
> `observablePath` is rejected fail-loud as a global ignore (FR-032) — the tolerance
> needed to express "deacon rejects, the reference accepts" cannot be written. The
> reference-side characterization stays in the preserved `wvr-` record (T044).
>
> **T032 note (as implemented)**: exception-correspondence violations (zero/many
> mechanisms, a missing entry, a broadened direction/scope) are reported under **V21** as
> mapping integrity; `mapping.json` gained an `exceptions` collection for them, which
> data-model §2 does not describe but FR-024/FR-027/FR-051 require.

**Checkpoint**: Every baseline unit has a destination. `validate` is clean. Orphans are structurally impossible.

---

## Phase 5: User Story 3 - Represent duplicate coverage as variants (Priority: P2)

**Goal**: Duplicate coverage becomes variants of one behavior; the behavior denominator never inflates.

**Independent Test**: Migrate the tier-1/merged duplicate pair — case count rises by 24, behavior count is unchanged. Introduce a near-duplicate behavior — the duplicate detector reports it.

### Tests for User Story 3 ⚠️ Write FIRST, confirm they FAIL

- [X] T049 [P] [US3] Denominator test — mapping the merged-mode units adds cases without adding behaviors; a variant wrongly authored as a new behavior fails, in `crates/conformance/tests/behavior_denominator.rs` (FR-014, SC-005, FR-047)
- [X] T050 [P] [US3] Duplicate-detection test — two behaviors with indistinguishable descriptions and mappings are reported as suspected duplicates, in `crates/conformance/tests/behavior_duplicates.rs` (FR-016)

### Implementation for User Story 3

- [X] T051 [US3] Implement variant representation in `crates/conformance/src/mapping.rs`: distinct cases sharing one behavior, recording what distinguishes them (context, oracle type, channel, input shape) per FR-015
- [X] T052 [US3] Implement indistinguishable-behavior detection in `crates/conformance/src/conservation.rs`, reporting suspected duplicates for merge or explicit differentiation (FR-016)
- [X] T053 [US3] Emit separate behavior-level and variant-level counts in the migration report totals in `crates/conformance/src/conservation.rs` (FR-017)
- [X] T054 [US3] Resolve the `parity_up_exec` inverse defect (research D2) in `conformance/registry/cases.json` — two cases currently claim two behaviors from one reported outcome; either add real evidence for the second or record it in `conformance/registry/residuals.json`

**Checkpoint**: Case count rises while behavior count holds at ≤ 25.

---

## Phase 6: User Story 4 - Preserve full failure diagnosis including deacon-only (Priority: P2)

**Goal**: Every difference class and process-level cause remains independently diagnosable, and no deacon-only data is discarded by a blanket rule.

**Independent Test**: Inject one instance of each class hermetically and confirm each is reported with its own classification. Register a blanket normalization rule — V24 fires.

### Tests for User Story 4 ⚠️ Write FIRST, confirm they FAIL

- [X] T055 [P] [US4] Extend `crates/deacon/tests/parity_harness_faults.rs` with one hermetic case per **difference** class — reference-only, deacon-only, value, accept-vs-reject with direction — using the existing stub-executable/synthetic-evidence pattern (FR-018, FR-056, research D9)
- [X] T056 [P] [US4] Extend `crates/deacon/tests/parity_harness_faults.rs` with one hermetic case per **process-level** cause not yet covered: the declarative `AllowedDifference`, `NoReferenceForPlatform`, and `Stale` outcomes (FR-019, FR-023, FR-056, research D9 "gap to close")
- [X] T057 [P] [US4] V24 test — a rule with no scope, an "all" scope, or a `drop` without field-specific justification each fail validation, in `crates/conformance/tests/normalization_rules.rs` (FR-021, FR-047)
- [X] T058 [P] [US4] Raw-vs-normalized separation test — both are preserved and separately locatable for every compared case, in `crates/parity-harness/tests/raw_outputs.rs` (extend existing) (FR-022)

### Implementation for User Story 4

- [X] T059 [US4] Implement the `NormalizationRule` registry (name, scope, action, justification) in `crates/conformance/src/conservation.rs` per data-model.md §6
- [X] T060 [US4] Add violation class **V24** (unscoped or unjustified normalization rule) to `crates/conformance/src/validate.rs` and `conformance/RULES.md` in lockstep
- [X] T061 [US4] Register the compliant existing rules — `path_token`, `null_preserving`, `label_semantic`, `mount_source_canonical`, `path_env_segmented`, `drop_noise_env`, `strip_intentional_labels` — with their scopes and justifications in `crates/conformance/src/conservation.rs`
- [X] T062 [US4] Retire `prune` from the config comparison path in `crates/parity-harness/src/normalize.rs`, replacing it with `null_preserving` semantics — this is research D3's central defect and will surface previously hidden differences
- [X] T063 [US4] Replace `sanitize_dynamic_values`/`replace_hex12` with a narrow field-scoped rule in `crates/parity-harness/src/normalize.rs`, since "any 12-char lowercase-hex run" can mask a genuine difference between two distinct hex values (research D3)
- [X] T064 [US4] Characterize every difference newly surfaced by T062/T063 in `conformance/registry/cases.json` or `conformance/registry/waivers/` (or a tracked fix issue) — never by reinstating a blanket rule (FR-036); expect a cluster across the 48 corpus units (depends on T038/T039 having migrated the corpus cases, or on a legacy-path run in the parity lane, to observe them)
- [X] T065 [US4] Raise `DiffKind::DeaconOnly` out of its "usually default noise" lowest rank in `crates/parity-harness/src/normalize.rs`, so deacon-only data is reported rather than de-prioritized (FR-020)

**Checkpoint**: Every failure class is independently reproducible hermetically; no blanket rule remains registrable.

---

## Phase 7: User Story 5 - Prove no coverage was lost (Priority: P2)

**Goal**: A deterministic before-and-after report that accounts for every baseline item and fails naming anything unaccounted.

**Independent Test**: Produce the report; remove one migrated case; the report fails naming exactly that item.

### Tests for User Story 5 ⚠️ Write FIRST, confirm they FAIL

- [X] T066 [P] [US5] No-loss test — removing one migrated case makes the report fail naming that item, its origin program, and what it asserted, in `crates/conformance/tests/conservation_report.rs` (FR-041, FR-047)
- [X] T067 [P] [US5] Error-path preservation test — a counterpart that loses rejection direction or diagnostic expectation fails, in `crates/conformance/tests/conservation_error_paths.rs` (FR-042)
- [X] T068 [P] [US5] Report determinism test — two runs on unchanged inputs are byte-identical with no timestamps or absolute paths, in `crates/conformance/tests/conservation_determinism.rs` (FR-043, SC-012)
- [X] T069 [P] [US5] Anti-gaming test — lowering the baseline to satisfy the report surfaces as a V25 failure rather than a green report, in `crates/conformance/tests/conservation_antigaming.rs` (FR-045)

### Implementation for User Story 5

- [X] T070 [US5] Implement the accounting computation (migrated / deduplicated / residual / retired / unaccounted) in `crates/conformance/src/conservation.rs` per contracts/migration-report.md, including the before/after totals for cases, variants, behaviors, channels, fixtures, and exceptions (FR-039, FR-040, FR-044)
- [X] T071 [US5] Implement the eight failure conditions from contracts/migration-report.md in `crates/conformance/src/conservation.rs`, each naming the specific item and category
- [X] T072 [US5] Implement error-path direction and diagnostic-expectation comparison in `crates/conformance/src/conservation.rs` (FR-042)
- [X] T073 [US5] Implement `migration report [--format json|md] [--out-dir]` and `migration check` in `crates/conformance/src/bin/conformance.rs`, honoring the stdout/stderr contract (Constitution VI)
- [X] T074 [US5] Implement the deterministic Markdown rendering as a pure function of the JSON in `crates/conformance/src/conservation.rs` (FR-043, SC-015)
- [X] T075 [US5] Emit the residual queue with blocked carriers and the `deletableCarriers` list in `crates/conformance/src/conservation.rs` (FR-055, feeding US7's deletion predicate)

> **T070 note (as implemented)**: `totals.before` carries two documented additions the
> contract predates — `unitsAtBranchPoint` (111, research §1) alongside `units` (118 after
> US4's T055/T056 added 7 guard units), and `recordedOnlyUnits` (33), which are inventoried
> but excluded from the accounting counters per research D8. Reporting one number would
> either hide the growth or lose SC-005's benchmark.
>
> **T075 note (as implemented)**: `deletionBlockers` was added alongside
> `deletableCarriers` — the contract says which carriers may go but not why the others may
> not, and "why" is the actionable half for US7. With no equivalence ledger present, NO
> carrier is deletable: unproven is not the same as safe. A third addition landed in T102's
> reviewer read: `deletedCarriers`. A carrier that has already been deleted is absent from
> the live registry, so it falls out of BOTH lists — leaving the report to print "No carrier
> is deletable yet" over a diff that removes four carriers and fifty files. True of the
> survivors, thoroughly misleading about the change.
>
> **T072 note (as implemented)**: the report found three real Phase-4 authoring defects
> — `parity_corpus_errors::{bad-config-path,missing-config}` and
> `parity_corpus_merged::extends-child` had counterparts that pinned no decision. Fixed at
> the root (two new decision-pinning cases; `case-merged-decl-extends-child` re-pointed to
> the spec-expectation oracle), not by relaxing the check.

**Checkpoint**: Conservation is provable and the report gates deletion.

---

## Phase 8: User Story 7 - Retire superseded machinery after proving equivalence (Priority: P3)

**Goal**: A live ledger proving the replacement is never more permissive, and a deletion predicate that blocks otherwise.

> Sequenced before US6 because the cut-over (US6) removes surfaces that the equivalence ledger must first clear.

**Independent Test**: Run the ledger over one carrier; force a more-permissive outcome and confirm deletion is blocked naming the unit and condition.

> **FR-031 binding**: every deletion task below MUST land in the **same change** as that carrier's reference updates — `.config/nextest.toml` (all profiles), `fixtures/parity-corpus/registry.json`, `Makefile`, `.github/workflows/parity.yml`, and docs. Phase 9 is the end-state sweep, not the first time those files are touched. A deletion that leaves a dangling reference for even one commit violates FR-031.

### Tests for User Story 7 ⚠️ Write FIRST, confirm they FAIL

- [X] T076 [P] [US7] Relation-classification unit test — `equivalent` / `stricter` / `more-permissive` classified on outcome, not message text, in `crates/parity-harness/tests/equivalence_relations.rs` (spec A-002)
- [X] T077 [P] [US7] Deletion-predicate test — a single `more-permissive` unit, or a residual naming the carrier, each block deletion naming the unsatisfied condition, in `crates/parity-harness/tests/equivalence_gate.rs` (FR-034, FR-037, FR-047)
- [X] T078 [P] [US7] Strictness-improvement test — a `stricter` relation without `characterizedAs` fails, in `crates/parity-harness/tests/equivalence_relations.rs` (FR-036)
- [X] T079 [P] [US7] Legacy-carrier ratchet test — a legacy carrier's mapped-unit count may only decrease once migration begins, in `crates/conformance/tests/legacy_ratchet.rs` (research D5, bounding the Constitution VIII exception)

### Implementation for User Story 7

- [X] T080 [US7] Implement relation classification and the ledger record in `crates/parity-harness/src/equivalence.rs` per contracts/equivalence-ledger.md (FR-033, FR-034)
- [X] T081 [US7] Implement the deletion predicate (all units `equivalent`/`stricter`, no residual naming the carrier, report accounts for every unit) in `crates/parity-harness/src/equivalence.rs` (FR-035, FR-038)
- [X] T082 [US7] Implement the `equivalence-report [--carrier <name>]` bin in `crates/parity-harness/src/bin/equivalence-report.rs`, failing loud on missing oracle/Docker — never skipping to pass (FR-023, FR-033, Constitution IV)
- [X] T083 [US7] Register `equivalence-report` in the parity lane: add nextest overrides in **all** profiles and an entry in `fixtures/parity-corpus/registry.json`, or `parity_registry_check` fails
- [X] T084 [US7] Run `equivalence-report` for the corpus carriers and characterize every `stricter` relation produced by T062/T063 in `conformance/registry/cases.json` or `conformance/registry/waivers/`
- [X] T085 [US7] Delete `crates/deacon/tests/parity_corpus_tier1.rs`, `parity_corpus_merged.rs`, `parity_corpus_errors.rs`, and `crates/deacon/tests/corpus_runner/mod.rs` once their units clear the predicate
- [X] T086 [US7] Delete `crates/deacon/tests/parity_read_configuration.rs` once its 2 units clear the predicate
- [X] T087 [US7] Delete the migrated `fixtures/parity-corpus/` case directories once `fixtureMapping` is verified one-to-one, keeping `fetch_realworld_corpus.py` per research D8
- [X] T088 [US7] Delete `crates/deacon/tests/parity_exec.rs`, `parity_build.rs`, `parity_up_exec.rs` **only if** residual-free after T040/T041; otherwise leave them with their residual records recorded

> **T085/T086/T087 DONE**, after the coordinator pulled **T099** forward to unblock them
> (see the T099 note). The deletion predicate held for all four carriers on evidence
> stronger than the contract requires: 59/59 units `equivalent` under a classifier that
> ALSO demanded the replacement flag every observable the legacy path flagged — outcome
> equality alone is blind exactly where 51 of those units sat (`(diverge, diverge)`), so
> that strengthening is what the decision actually rests on. Deleted in one pass with
> their reference updates (FR-031): 5 test sources, 35 fixture directories, 13 legacy
> pointer cases, 4 `live_binaries` entries, both `corpora` entries, and 7
> `.config/nextest.toml` override blocks.
>
> **T087 rationale (supersedes the earlier T114 concern)**: quickstart.md §2's worked
> example deletes the migrated fixture directories in the same `git rm` as the binary —
> git history is the re-verification path, not a live copy in the tree.
> `fetch_realworld_corpus.py` is kept unchanged (research D8).
>
> **T083 note (as implemented)**: `equivalence-report` is a BIN, not a test binary.
> Registering it in `live_binaries` would fail `parity_registry_check`
> (`check_test_files` requires `crates/deacon/tests/<name>.rs`), and a nextest override
> would select nothing. It follows the established convention for `parity-report` /
> `conformance-snapshot`: reached through `make test-parity-equivalence` and the parity
> workflow.
>
> **T082 note (as implemented, revised)**: the bin runs each superseded carrier's OWN test
> binary and reads the `ReportFragment` it already writes, rather than re-orchestrating
> its comparison — so it re-implements nothing (FR-030) and works for every carrier,
> Docker-backed ones included.
>
> **T088 (as implemented, final)**: `parity_build.rs` and `parity_exec.rs` are correctly
> residual-blocked (`res-build-image-discovery` / `res-build-tolerant-outcome`;
> `res-exec-per-side-argv`). `parity_up_exec.rs` is **also kept**, on a second and
> independent ground found by attempting its deletion: its equivalence verdict is clean
> (`equivalent`, 1/1, no residual, on the CORRECTED binary — see T115), but its legacy case
> is the ONLY evidence for `bhv-exec-container-id-metadata` (research D2's inverse defect,
> characterized in T054). Deleting it made `validate` report **V5** and **V8**, so the
> deletion was reverted. The unit-level predicate could not see this: a unit maps one-to-one
> to its replacement case, but a legacy case may claim SEVERAL behaviors from one reported
> outcome, and the replacement inherits only what its own case declares. `deletion_status`
> now blocks a carrier that is the sole evidence for any behavior, so the next attempt is
> refused BEFORE the irreversible act. Unblocking it is T110's job.

**Checkpoint**: Superseded carriers are gone or explicitly residual-blocked, each deletion evidence-backed.

---

## Phase 9: User Story 6 - Cut every invocation surface over in one step (Priority: P3)

**Goal**: One authoritative runner reachable through surfaces that decide nothing, with no compatibility layer and no dangling references.

**Independent Test**: Invoke every documented surface; confirm results come from the authoritative runner and no reference points at a removed surface.

> **Note**: per-carrier reference updates already landed alongside each deletion in Phase 8 (FR-031). These tasks are the end-state sweep and verification, confirming the surviving set is coherent.

### Tests for User Story 6 ⚠️ Write FIRST, confirm they FAIL

- [X] T089 [P] [US6] Dangling-reference test — a reference in the Makefile, workflow, nextest config, parity registry, or docs to a removed surface fails, in `crates/deacon/tests/parity_registry_check.rs` (extend) (FR-032, FR-047)
- [X] T090 [P] [US6] Single-implementation test — a second implementation of any comparison or normalization rule fails, in `crates/parity-harness/tests/normalize_consistency.rs` (extend) (FR-029, FR-030, Constitution VIII)
- [X] T091 [P] [US6] Consumer-scope test — `deacon --help` gains no subcommand from this feature, in `crates/deacon/tests/parity_registry_check.rs` (extend) (Constitution II)

### Implementation for User Story 6

- [X] T092 [US6] Update `.config/nextest.toml` in all profiles to drop the deleted binaries, resolving to the UNION of `binary(=…)` clauses on conflict
- [X] T093 [US6] Update `fixtures/parity-corpus/registry.json` `live_binaries` and `corpora` to match the surviving set
- [X] T094 [US6] Update `make test-parity` in `Makefile` and the `parity / live-certification` lane in `.github/workflows/parity.yml` to select the surviving runner, keeping them thin delegations
- [X] T095 [US6] Update `CLAUDE.md`, `fixtures/parity-corpus/README.md`, and `conformance/RULES.md` in lockstep with the cut-over, leaving no reference to a removed surface

> **T092/T093 (as implemented)**: verification pass only — the per-carrier reference
> updates landed inside Phase 8 alongside each deletion, per FR-031's binding note. The
> sweep found nothing missed: `.config/nextest.toml` has zero references to the five
> removed surfaces across every profile, and `registry.json` is 6 live binaries with an
> empty `corpora` (the corpora retired with the binaries that drove them).
>
> **T090 (as implemented)**: the single-implementation test forced a rename.
> `replace_hex12` survived as the helper the NARROWED `devcontainer_id_token` rule calls,
> but the name reads as a document-wide replacement — exactly the blanket behavior 023
> T063 retired. It is now `tokenize_hex12`, so "the retired blanket rules do not exist" is
> literally true rather than true-if-you-read-the-call-site.
>
> **T095 (as implemented)**: the dangling-reference test holds docs to a softer rule than
> machine-consumed files — a doc line may name a removed surface only while saying it is
> gone. History is worth keeping; a doc describing a deleted binary as current
> architecture is a lie with a long half-life.

**Checkpoint**: One runner, one normalizer, zero compatibility surfaces.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [X] T096 [P] Verify all seven FR-046 acceptance areas have a test under `crates/conformance/tests/` or `crates/parity-harness/tests/` **and** that each was demonstrated to fail on the violation it guards (FR-047); record the demonstration in the PR body

> **FR-046/FR-047 audit — all seven areas covered, each demonstrated to fail.** The
> demonstration column is the important one: a guard nobody has watched fail is a guard
> nobody has tested.
>
> | FR-046 area | Test | Demonstrated failure |
> |---|---|---|
> | baseline enumeration + determinism | `baseline_determinism.rs`, `baseline_archive.rs` | Written before `baseline.json` existed and watched fail: 6 tests errored with *"committed baseline … is unreadable (No such file or directory)"*. The enumeration leg additionally failed on a real off-by-one — a stray `.deacon/` run artifact made the naive listing 26 vs the expected 25 — proving the D1 leg is live, not a tautology. |
> | one-to-one fixture migration | `mapping_fixtures.rs` | Each of merge / split / drop / unreferenced-orphan constructed and asserted. Live proof: V22 caught a real defect in my own baseline — `parity_exec`'s four units all declared `inline:parity_exec`, making one-to-one impossible (fixed to `inline:<program>#<case>`). |
> | behavior deduplication | `behavior_denominator.rs`, `behavior_duplicates.rs` | `a_variant_wrongly_authored_as_a_new_behavior_is_reported` and `two_indistinguishable_cases_sharing_a_behavior_are_rejected` both fail on the inflation they guard; the duplicate detector's fixtures initially failed on real inflection differences ("rejects" vs "rejected"), which is why `stem` exists. |
> | preserved failure classification, each class independently | `parity_harness_faults.rs` legs (k)–(q) | One hermetic case per difference class (ref-only / deacon-only / value / accept-vs-reject with direction) and per previously-unproven declarative outcome (allowed-difference / no-reference-for-platform / stale). Extends the existing stub-executable mechanism — no second mechanism (FR-056). |
> | invocation-surface delegation + cut-over completeness | `parity_registry_check.rs` (`no_surface_references_a_removed_binary`, `no_surface_globs_a_removed_path`, `the_surviving_set_is_mutually_consistent`, `the_shipped_cli_gained_no_subcommand_from_this_feature`) and `normalize_consistency.rs` | Both dangling-reference tests failed on REAL drift on first run — the name check on `CLAUDE.md` + `fixtures/parity-corpus/README.md`, the path check on the two silently-rotted workflow globs (T116). The single-implementation test failed on a real duplicate-looking name and forced `replace_hex12` → `tokenize_hex12`. |
> | stricter-difference detection | `equivalence_relations.rs`, `equivalence_gate.rs` | Relation classification proven on outcome not message text; `a_stricter_relation_without_a_characterization_is_a_defect` fails on the suppression it guards. Live proof: the gate refused to clear `parity_up_exec` while its `stricter` verdict was uncharacterized. |
> | no-coverage-loss report | `conservation_report.rs`, `conservation_error_paths.rs`, `conservation_determinism.rs`, `conservation_antigaming.rs` | Removing a mapping entry fails naming the unit, its program and its assertion; removing a case fails naming the case. Live proof: on first run the report found **three real Phase-4 authoring defects** (two both-reject error cases and `extends-child` pinned no decision), fixed at the root rather than by relaxing the check. |

- [X] T097 [P] Verify every test binary this feature added has nextest overrides in **all** profiles of `.config/nextest.toml`, and that every live binary also has an entry in `fixtures/parity-corpus/registry.json`, or `parity_registry_check` fails (Constitution VII)
- [X] T098 [P] Confirm every hermetic test runs with no Docker and no network by checking the `dev-fast` selection in `.config/nextest.toml` (FR-048, SC-017), including on the Windows lane

> **T097 audit (clean)**: every hermetic test binary this feature added — `baseline_determinism`,
> `baseline_archive`, `mapping_orphans`, `mapping_fixtures`, `residual_validation`,
> `case_identity_stable`, `exception_migration`, `behavior_denominator`,
> `behavior_duplicates`, `normalization_rules`, `conservation_report`,
> `conservation_error_paths`, `conservation_determinism`, `conservation_antigaming`,
> `legacy_ratchet` (conformance) and `equivalence_relations`, `equivalence_gate`
> (parity-harness) — carries **no** nextest override, which is the standing convention for
> hermetic conformance tests and is what makes them run in EVERY profile. The comment block
> in `.config/nextest.toml` names all of them. `equivalence-report` is a BIN, not a test
> binary, so it is deliberately absent from `live_binaries` (registering it would fail
> `parity_registry_check::check_test_files`); it is reached through
> `make test-parity-equivalence`. `parity_registry_check` is green at 10/10, which is the
> machine-checked form of this audit.
>
> **T098 audit (clean)**: no new hermetic test uses a unix-only API — `std::os::unix`,
> `PermissionsExt` and `#![cfg(unix)]` appear only in the PRE-EXISTING
> `runner_record_replay.rs` / `aggregator.rs` / `raw_outputs.rs` / `docker_channels.rs`,
> and `raw_outputs.rs` (which T058 extended) is `#![cfg(unix)]`-gated at file scope so the
> additions inherit it. Grepping all 17 new hermetic test files for
> `Command::new("docker")`, `reqwest`, `TcpStream` and literal URLs returns **zero** hits
> in every one. They read committed JSON and evaluate pure functions, so the Windows
> `dev-fast` lane exercises them identically.

- [X] T099 Remove the V25 baseline drift gate from `crates/conformance/src/validate.rs` and `conformance/RULES.md`, retaining `baseline.json` and the final migration report as evidence (FR-053); do this only when the deletion predicate holds for every non-residual carrier
  - **REORDERED — executed in Phase 8, not Phase 10.** The gate and the first deletion are mutually exclusive: `baseline generate` enumerates live carriers from `fixtures/parity-corpus/registry.json`, so deleting a proven-safe carrier necessarily drops its units from the regenerated baseline and fails V25 permanently (verified: dropping `parity_corpus_tier1` produced 24 `removed:` findings). Waiting for the original precondition — "the deletion predicate holds for EVERY non-residual carrier" — would have left carriers that had already cleared the predicate undeleted indefinitely, for no safety benefit: the risk V25 protects against is silent, unreviewed baseline drift, and nothing here is merged unreviewed. Retired at the point the first four carriers cleared. `conformance/migration/baseline.json` is retained untouched as evidence; only the live checking gate is gone. The regeneration-dependent tests (`baseline_drift.rs`, the frozen-match leg of `baseline_determinism.rs`, the discovery legs of `baseline_enumeration.rs`) retired with it; `baseline_archive.rs` replaces them with archival-integrity checks that survive carrier deletion.
- [X] T100 [P] Update `conformance/RULES.md` with the residual-vs-gap distinction and the V21–V24 classes, keeping validate.rs/RULES.md lockstep
- [X] T101 Run `quickstart.md` end-to-end and correct any drift between it and the shipped commands
- [X] T102 Full gate from the repository root: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`, `make test-parity`

> **Results.** First complete (non-`dev-fast`) run of this feature.
>
> | Gate | Result |
> |---|---|
> | `cargo fmt --all -- --check` | clean |
> | `cargo clippy --all-targets --all-features -D warnings` | clean |
> | `make test-nextest` (profile `full`) | **3836 passed, 0 failed**, 38 skipped, 850 s |
> | `cargo run -p deacon-conformance -- validate` | clean |
> | `cargo run -p deacon-conformance -- certify` | **certified** (0 blocking, 10 waived) — the only conformance gate in the release path |
> | `cargo run -p deacon-conformance -- migration check` | 118 units, 0 unaccounted, 26/26 error paths preserved, 0 violations |
> | `make test-parity` | **RED** — see below |
>
> **`make test-parity` is red, and that is the correct end state, not an unfixed failure.**
> Of the six live binaries, five pass completely (18/18 tests, re-run with
> `--no-fail-fast` to get past fail-fast cancellation). The sixth,
> `parity_conformance_runner`, reports **20 agree / 51 diverge** across the declarative
> cases — every one of the 51 in the T113 families and nothing else, verified by
> tabulating every diverging path in the run. Making it green would require either fixing
> deacon's `read-configuration` output shape (six families, reaching container identity and
> `up`; a feature, not a polish task) or waiving fix-flavored divergences, which is
> precisely the move the conformance model exists to prevent. The parity lane is not in the
> release path — `certify` is, and `certify` is green.
>
> T113's enumeration was **incomplete** and was corrected here from the full run: it named
> four families from an earlier occurrence-count pass, and two more only appear once every
> case runs — `workspace.workspaceFolder` (43 cases) and `workspace.workspaceMount` (2),
> both VALUE divergences rather than presence ones, both reproducible by hand. Root cause
> diagnosed and written up in T113 and in the two behavior records: when the workspace
> folder is a subdirectory of a git root, deacon reports `workspaceFolder` as the mount
> root and loses the subdirectory, where the reference reports the subdirectory inside the
> mount.
>
> **Four fixes landed during this gate**, each a real defect the earlier phases missed:
> `.config/nextest.toml`'s comment block still named the deleted `baseline_drift` /
> `baseline_enumeration` (now caught structurally — both added to
> `no_surface_references_a_removed_binary`'s `REMOVED` set); `fixtures/parity-corpus/errors/README.md`
> still described the deleted `parity_corpus_errors` as the current runner (rewritten, and
> the file added to the dangling-reference scan so it cannot rot again);
> `.github/workflows/parity.yml`'s `pull_request` trigger listed only the OLD paths, so a
> `cases.json` or `conformance/fixtures/` edit could change what the live lane runs without
> running it; and `scripts/parity/prepull-fixture-images.sh` warmed base images but never
> BUILT the one `:local` fixture image CI builds, so a local `make test-parity` would fail
> where CI passed.
>
> **The migration report was also corrected here**, from a reviewer read rather than a test
> failure. It rendered "No carrier is deletable yet." over a diff that deletes four carriers
> and fifty files: a deleted carrier is absent from the live registry and so fell out of both
> `deletableCarriers` and `deletionBlockers`. Added `deletedCarriers` (contract updated),
> plus three explanatory lines for the numbers a reviewer legitimately stops on — why the
> `after` fixture count is lower, why the accounting counters and the residual queue use
> different denominators, and why em-dashed rows do not subtract.

---

## Deferred Work

Per Constitution I (Deferral Tracking); rationale in [research.md §4](./research.md).

- [ ] T103 [Deferral, research D4] Migrate `crates/deacon/tests/parity_state_diff.rs`'s 8 units into `conformance/registry/cases.json`
  - **Decision**: Recorded as residuals in this feature; the declarative runner lacks cross-CLI state-snapshot comparison and a reference-free intra-deacon comparison mode.
  - **Acceptance**: All 8 units declarative, `parity_state_diff` deleted, equivalence ledger clean, residual records removed.
- [ ] T104 [Deferral, research D4] Migrate `crates/deacon/tests/parity_observable_state.rs`'s 7 units into `conformance/registry/cases.json`
  - **Decision**: Recorded as residuals; requires container-handoff and rendered-compose observation capabilities.
  - **Acceptance**: All 7 units declarative, program deleted, residual records removed.
- [ ] T105 [Deferral, research D8] Decide the long-term disposition of the 33 entries in `fixtures/parity-corpus/fetch_realworld_corpus.py`, recording it in `conformance/registry/residuals.json`
  - **Decision**: Retained as recorded-only residuals; vendoring third-party workspaces is out of scope.
  - **Acceptance**: An explicit registry disposition (vendor, prune the manifest, or permanent documented residual), not an open queue entry.
- [ ] T106 [Deferral, research D3] Complete characterization in `conformance/registry/` of every difference surfaced by retiring `prune` from `crates/parity-harness/src/normalize.rs`
  - **Decision**: T064 characterizes those found during migration; a long tail may surface as more units migrate.
  - **Acceptance**: Zero uncharacterized differences across the corpus units; each is a case, a waiver, or a tracked fix issue.

- [ ] T107 [Deferral, US2 residual `res-exec-per-side-argv`] Give a declarative operation per-side argument vectors, then migrate `parity_exec::env-propagation`
  - **Decision**: The unit compares deacon's `--remote-env FOO=BAR` against the reference's `--env FOO=BAR`; a declarative `operation` carries exactly ONE argv that both sides run, so the unit is recorded as a residual rather than forced into a shape that would compare the wrong flags.
  - **Acceptance**: The operation model expresses a per-side argv (or the flags converge), `parity_exec::env-propagation` is a declarative case, and `res-exec-per-side-argv` is deleted in the same change.

- [ ] T108 [Deferral, US2 residuals `res-build-image-discovery` / `res-build-tolerant-outcome`] Extend observation and assertion so the 6 `parity_build` units become declarative
  - **Decision**: `chan-image` inspects the container a case created, and `build` produces an image with no container, so image-discovery-by-label is unobservable; and three of the units assert a JSON result SHAPE that must hold whether the operation succeeds or fails, which no single assertion expresses. Recorded as residuals per T041 rather than migrated at reduced fidelity.
  - **Acceptance**: All 6 `parity_build` units are declarative cases, `parity_build` clears the equivalence gate, and both `res-build-*` records are deleted.

- [ ] T111 [Deferral, US4 T062] Omit absent optional properties from `read-configuration` output (`skip_serializing_if`), then delete the `drop_absent_optional` rule
  - **Decision**: deacon serializes every modeled optional property of `devcontainer.json` unconditionally (explicit `null` / `[]` / `{}`) while the reference omits unauthored keys — measured as ~2,500 spurious divergences across the 24 Tier-1 workspaces in both modes, which buried every real difference. The two documents describe the SAME resolved configuration in different JSON shapes. Fixing it is a change to the shipped `deacon` CLI, which this feature explicitly does not make (plan.md Constitution Check I). The named, enumerated, justified `drop_absent_optional` rule (46 key names, value-guarded) makes the comparison meaningful in the meantime — it is NOT a blanket rule: an unlisted property still surfaces.
  - **Acceptance**: `DevContainerConfig` (and the merged-configuration document) omit absent optionals, `normalize::ABSENT_OPTIONAL_KEYS` and the `drop_absent_optional` registry entry are deleted, and the corpus comparisons stay clean without them.

- [X] T112 [Deferral, US4 T061] Narrow or retire `strip_intentional_labels` — **RETIRED** (024 Phase 4)
  - **Decision**: the rule subtracts labels by four PREFIX matches (`devcontainer.`, `com.docker.`, `desktop.`, `dev.containers.`), an open-ended removal set that FR-021 forbids and data-model §6 names specifically. It is registered as `known_non_compliant` with this reason rather than dressed up as enumerated — narrowing it needs the live label sets both CLIs actually stamp (including the full `com.docker.compose.*` set), which belongs with its carrier's migration. It is scoped to the legacy `chan-container-state` channel and retires with `parity_state_diff` / `parity_observable_state`.
  - **Acceptance**: either an enumerated label list replaces the prefixes, or the rule is deleted with its carriers; `declared_non_compliant_rules` returns empty.
  - **Resolved (024 Phase 4)**: RETIRED, not narrowed. `normalize::container_state` now captures every label verbatim (`is_intentional_label` / `INTENTIONAL_LABEL_PREFIXES` deleted) and the registry entry is gone, so `declared_non_compliant_rules` returns empty and `certify` reports `nonCompliantRules: 0`. The tolerance moved to where it is visible: a scoped, backed `allowedDifference` on the declarative cases that compare labels (024 Phase 5), and — until those land — a named `INTENTIONAL_LABEL_FIELDS` allowance inside `parity_state_diff`, the one legacy carrier that still diffs labels, which dies with it. Narrowing was rejected: an enumerated list of the labels both CLIs happen to stamp today would still go silently wrong the day either adds one.

- [ ] T113 [Deferral, US4 T064] Align the document-shape divergences retiring `prune` surfaced
  - **Decision**: with the blanket `prune` gone, genuine deacon-behind-reference divergences are REPORTED rather than hidden. No tolerance was authored and no blanket rule reinstated (FR-036); each is characterized on `bhv-readconfig-tier1-corpus` / `bhv-readconfig-merged-configuration` as `reference: divergent`, `decision: follow-spec` — deacon should change, so a `wvr-` waiver would be the wrong instrument.
  - **Measured at T102** over the full declarative run (71 cases: **20 agree, 51 diverge**), counted as *cases affected*, not raw occurrences. Rows 1 and 5 are one family seen in two output modes, so this is six distinct families across seven diverging paths. The original entry named four from an earlier occurrence-count pass; rows 3 and 7 were only visible once every case ran:

    | # | Diverging path | Cases | What differs |
    |---|---|---:|---|
    | 1 | `configuration.configFilePath` | 51 | reference-only — the reference emits it, deacon does not |
    | 2 | `workspace.configFolderPath`, `workspace.rootFolderPath` | 51 | deacon-only — deacon emits them, the reference does not |
    | 3 | `workspace.workspaceFolder` | 43 | **value** — see below |
    | 4 | `mergedConfiguration.{onCreate,postCreate,postStart,postAttach,updateContent}Commands` | 7–21 each | reference-only — the reference's merged config carries PLURAL command arrays; deacon emits only the singular forms |
    | 5 | `mergedConfiguration.configFilePath` | 23 | reference-only (family 1 in merged mode) |
    | 6 | `featuresConfiguration` / `.dstFolder` / `.featureSets` | 11–12 | shape — `dstFolder` reference-only, `featureSets` value-divergent |
    | 7 | `workspace.workspaceMount` | 2 | **value** — see below |

  - **Families 3 and 7 are newly identified and are the most interesting**, because both are *value* divergences rather than presence ones, and both reproduce by hand in three commands. They surface whenever the workspace folder is a SUBDIRECTORY of a git root — which is every in-repo fixture, and every CI checkout:

    ```bash
    deacon read-configuration --workspace-folder conformance/fixtures/fx-tier1-go-minimal
    #   workspace.workspaceFolder = /workspaces/deacon                 <- the git ROOT
    devcontainer read-configuration --workspace-folder conformance/fixtures/fx-tier1-go-minimal
    #   workspace.workspaceFolder = /workspaces/deacon/conformance/fixtures/fx-tier1-go-minimal
    ```

    Both CLIs agree on `workspaceMount` here (`source=/workspaces/deacon,target=/workspaces/deacon`), so the reference is self-consistent — it mounts the git root and points `workspaceFolder` at the subdirectory INSIDE that mount. deacon collapses `workspaceFolder` to the mount root, which loses the subdirectory. Family 7 is the same seam from the other side: with an explicit config `workspaceFolder`, deacon renders `workspaceMount` as `target=<that folder>` while the reference renders `target=<source path>`.

    This is adjacent to the documented `--mount-workspace-git-root` default (see the canary note in `CLAUDE.md`), but the *default* is about what gets MOUNTED; deriving the reported `workspaceFolder` from the mount root rather than from the requested workspace folder is a separate, unintended consequence. Diagnosis only — no fix attempted here, since changing path derivation reaches `up` and container identity, far outside a polish phase.

  - **Consequence, stated plainly**: `make test-parity` is **RED** on this branch and will stay red until this is resolved. That is the designed outcome of retiring a blanket normalizer, not a regression this feature introduced: pre-migration these differences existed and were *silently pruned away*. Per `CLAUDE.md`, the parity lane is not in the release path — `certify` is, and `certify` is green. Making the lane green by waiving fix-flavored divergences is the one move the model exists to prevent.
  - **Acceptance**: each family is fixed in deacon or characterized as an intentional divergence with a `wvr-` record; the declarative corpus comparisons report zero uncharacterized differences (this is research D3's T106 acceptance).

- [X] T114 [Deferral, US7 T087] — **CLOSED as a rationale note.** quickstart.md §2's worked example deletes the migrated fixture directories in the SAME `git rm` as the binary: **git history is the re-verification path**, not a retained live copy. Note: `fixtures/config/{basic,with-variables}` were deleted and then RESTORED — `crates/core/tests/integration_variable_substitution.rs` reads them, so they were never exclusively the migrated carrier's. `fixtureMapping` records which carrier consumed a fixture, not that it was the only consumer; Phase 9's dangling-reference sweep should close that gap.

- [X] T115 — **RETRACTED: the `parity_up_exec` `stricter` verdict was a harness artifact, not a behavioral difference.**
  - **Root cause**: `equivalence-report`'s `deacon_binary()` preferred `target/release/deacon` over `target/debug/deacon` purely on file existence. A release artifact left over from three days earlier satisfied that check, so the ledger judged a **stale deacon** against the current oracle. Verified directly: the stale `target/release/deacon` (2026-07-22) prints `env=[tkn-42]` for the `${containerEnv:VAR}` lifecycle marker; the freshly built binary prints `env=[]`, matching the reference. There is **no #332 regression** — that framing is withdrawn.
  - **Why the two invocation paths disagreed**: every parity TEST binary uses `env!("CARGO_BIN_EXE_deacon")`, the artifact cargo just compiled. A bin has no such macro, and mine guessed instead of establishing the equivalent.
  - **Fix**: `deacon_binary()` now runs `cargo build -p deacon --message-format json` and takes the executable path cargo reports, failing loud if cargo reports none. `DEACON_PARITY_DEACON_BIN` still overrides deliberately. Re-run: `parity_up_exec::traditional` is **`equivalent`** (legacy `pass`, replacement `agree`).
  - **The dangerous mirror image**: a stale build that happened to AGREE with the reference would have produced a false `equivalent` and authorized deleting real coverage. A gate for an irreversible act must not guess which binary it is judging.

- [X] T116 [US6 follow-up] — **`case-merged-decl-universal-jsonc`'s "hang" is a 9.93 GB first-pull, and I broke the CI step that prevented it.**
  - **Not a deacon bug.** The debug log's "TCP connect then nothing" is the handoff from deacon's HTTP client to Docker. The OCI manifest fetch for `ghcr.io/devcontainers/features/docker-in-docker:2` COMPLETED in 145 ms (`Manifest fetched with digest: 25b9f057…`); the configured 2 s request timeout was never violated. Execution then reached `ensure_image_available`, which pulls the image so `--include-merged-configuration` can read its `devcontainer.metadata` label — matching the reference CLI (#307, REPORT.md). `mcr.microsoft.com/devcontainers/universal:2-linux` is **9.93 GB**. Not the environment either: ghcr.io TLS answers in 95 ms via curl. **Cached, the same command completes in 0 s; uncached it exceeds the harness's 120 s bound.**
  - **The real defect, and it is mine.** `.github/workflows/parity.yml` already had two steps guarding exactly this — "Build corpus fixture images" and "Pre-pull corpus base images", the latter citing the `universal` image and the 120 s bound by name. Both globbed `fixtures/parity-corpus/*/…`, which US7 deleted. A glob that matches nothing is not an error, so both steps kept succeeding while doing nothing and the protection silently evaporated. Repointed at `conformance/fixtures/*/…`.
  - **Fix**: both workflow globs repointed; new `scripts/parity/prepull-fixture-images.sh` gives `make test-parity` the same warm-cache step CI has, so a local run behaves like CI; and T089 gained a THIRD direction (`no_surface_globs_a_removed_path`) — the name-based check could never have caught this, because a rotted *path* fails more quietly than a rotted *name*.
  - **Accounting, corrected**: nothing was ever silently dropped. With the cache warm, a full `parity_conformance_runner` run puts **`case-merged-decl-universal-jsonc` in the diverge list**, and merged is still 23-of-24 — because the 24th, `case-merged-decl-extends-child`, **agrees**: T072 re-pointed it to the `spec-expectation` oracle, since a live-differential inverted its `reference-stricter` expectation into a guaranteed failure. So 23 diverging + 1 agreeing = all 24 accounted for. The earlier "merged ×23" figure was complete; the suspicion that a case had been excluded from the denominator was reasonable but does not hold. Separately, the 59-unit equivalence run reported no `UNCLASSIFIABLE`/`UNVERDICTED` lines and exited 0, and `run_case` propagates a timeout as `Err` rather than a verdict — so every one of those 59 comparisons genuinely completed.

- [ ] T110 [Deferral, US3 T054] Give `bhv-exec-container-id-metadata` its own independently-reported case
  - **Decision**: `parity_up_exec` asserts both up/exec parity AND `exec --container-id` metadata recovery but emits ONE `CaseResult`, so the two behaviors shared one reported outcome (research D2's inverse defect). T054 resolved the registry's over-claim by merging the two pointer cases into one truthful record; it did not create the missing evidence, because a declarative case cannot address a container the previous operation created — there is no runtime-resolved container-id token in an operation's argv.
  - **Acceptance**: A declarative case evidences `bhv-exec-container-id-metadata` independently (its own reported outcome), and `case-up-exec-parity` no longer claims two behaviors from one outcome.

- [ ] T109 [Deferral, US2 residuals `res-harness-*` / `res-consistency-*`] Decide the long-term disposition of the 27 harness/internal-consistency self-test units
  - **Decision**: The 23 hermetic-guard units (17 `parity_harness_faults` + 6 `parity_registry_check`, after US4's T055/T056 added 7) and 4 internal-consistency units observe the *harness* and deacon's own internal agreement, not a devcontainer, so they have no observable channel and no reference to compare against. They are recorded as residuals whose carriers (`parity_harness_faults`, `parity_registry_check`, `consistency_env_probe_flag`, `consistency_remote_env_flags`) this feature never intends to delete — per plan.md those two guard programs are EXTENDED, not retired.
  - **Acceptance**: An explicit registry disposition — either a permanent, documented residual class for "guards the machinery, not the behavior", or a non-residual concept for units whose carrier is intentionally permanent — not an open queue entry that implies pending work.

- [ ] T114 [Deferral, 024 Phase 5] Characterize the two container-state divergences the first live `chan-container-state` differential surfaced
  - **Found by**: `case-up-container-identity-labels` on its first live run against the pinned oracle 0.87.0 (fixture `fx-up-container-labels`, `debian:bookworm-slim`). Neither was visible before 024 Phase 4, because the retired `strip_intentional_labels` rule dropped the whole `devcontainer.*` namespace and no declarative case observed container state at all.
  - **(a) `labels.devcontainer.metadata`** — the reference ALWAYS stamps the label, using `"[]"` when there is nothing to record; deacon omits it entirely. `up::merged_config::build_container_metadata_label` returns `None` when the config contributes nothing and the image supplies no entries. The obvious fix (always emit) is wrong as stated: that `None` ALSO prevents clobbering a `devcontainer.metadata` label inherited from the image, so the correct behavior is "emit `[]` only when nothing is inherited", which needs its own test.
  - **(b) `cmd`** — deacon keeps the container alive with `sh -c 'export PATH=…; sleep infinity || tail -f /dev/null'`; the reference uses `echo Container started` + `trap "exit 0" 15` + `exec "$@"` + a `while sleep 1 & wait $!` loop. Both keep the container running; the reference's form also forwards SIGTERM and execs the configured command. Likely `intentional-divergence`, but SIGTERM handling is an observable difference that must be checked before deciding.
  - **Deliberately NOT tolerated meanwhile**: the case reports `diverge` on both paths. A scoped `allowedDifference` needs a real backing `wvr-`/`ext-` record, and authoring one before deciding what the difference MEANS is exactly the suppression the model forbids — `stricter` with no `characterizedAs` is the same defect the equivalence ledger blocks.
  - **Acceptance**: each path has a three-axis behavior record and either a fix (a) or a backed tolerance (b); `case-up-container-identity-labels` reports `agree`/`allowed-difference` on every path.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — **blocks all stories**
- **US1 (Phase 3)**: depends on Foundational. **Blocks US2, US5, US7** — nothing is measurable before the baseline is frozen
- **US2 (Phase 4)**: depends on US1
- **US3 (Phase 5)**: depends on US2 (needs cases to deduplicate); T039 and T049 are coupled
- **US4 (Phase 6)**: depends on Foundational only — **can run in parallel with US2/US3**; the one exception is T064 (characterizing surfaced differences), which needs the corpus cases from T038/T039 or a legacy-path run in the parity lane
- **US5 (Phase 7)**: depends on US1 + US2
- **US7 (Phase 8)**: depends on US5 (report) + US4 (T062/T063 produce the `stricter` relations to characterize)
- **US6 (Phase 9)**: depends on US7 — surfaces can only be cut over once carriers are deletable
- **Polish (Phase 10)**: depends on all above; T099 specifically depends on US7 completing

### Critical Path

```text
Setup → Foundational → US1 (baseline) → US2 (mapping) → US5 (report) → US7 (equivalence + deletion) → US6 (cut-over) → T099
                                   ↘ US3 (variants)
                     ↘ US4 (failure classes + prune retirement) ──────↗
```

### Parallel Opportunities

- T002–T004 (Setup) in parallel
- T006–T008 (Foundational types, distinct files) in parallel
- All test tasks within a story are `[P]` — distinct files
- **US4 runs in parallel with US2/US3** — different subsystem (normalization vs mapping), and its `prune` retirement is on the critical path for US7, so starting it early shortens the whole feature
- T036–T041 (per-carrier migrations) are `[P]` with each other except where they share `cases.json`; serialize `cases.json` edits or partition by carrier

---

## Parallel Example: User Story 1

```bash
# Tests first (all distinct files):
Task: "Determinism test in crates/conformance/tests/baseline_determinism.rs"
Task: "Drift-detection test in crates/conformance/tests/baseline_drift.rs"
Task: "Enumeration-source test in crates/conformance/tests/baseline_enumeration.rs"
```

---

## Implementation Strategy

### MVP (US1 only)

Setup → Foundational → US1. **Stop and validate**: the baseline is frozen, deterministic, drift-gated, and the stale 25-vs-24 counts are corrected. This alone converts "no coverage lost" from an unfalsifiable claim into a measurable one, and is worth shipping even if nothing else lands.

### Incremental Delivery

1. US1 → baseline frozen (MVP)
2. US2 → every unit has a destination; orphans structurally impossible
3. US4 (in parallel from step 2) → failure classes preserved, `prune` retired
4. US3 → denominator honest
5. US5 → conservation provable
6. US7 → carriers deleted with evidence
7. US6 → surfaces cut over

Each step is its own small, CI-gated PR with a Conventional-Commit title (`feat`/`fix`/`chore` — never `test`/`style`).

### Realistic End State

Per research D4, expect `parity_state_diff` and `parity_observable_state` to survive this feature carrying residual records (T103/T104). That is a planned outcome, not incomplete work — state it in the PR body so it does not read as a shortfall.

---

## Notes

- `[P]` = different files, no incomplete dependencies
- Verify each test fails before implementing (FR-047 requires the demonstration, not just the test)
- Every new test binary needs nextest overrides in **all** profiles plus a `registry.json` entry
- `validate.rs` and `conformance/RULES.md` change in lockstep — every new violation class touches both
- Never weaken `certify`, never delete a real gap, never reinstate a blanket normalization rule to go green
