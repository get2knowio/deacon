---

description: "Task list for 025-exploratory-parity-discovery"
---

# Tasks: Exploratory Parity Discovery

**Input**: Design documents from `/specs/025-exploratory-parity-discovery/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: **MANDATORY.** The feature specification explicitly requires acceptance tests for nine
areas (seed reproduction, semantic generation, shrinking, metamorphic failures, finding
classification, deduplication, review-only promotion, pinned real-world provenance, and lane
isolation), and constitution Principle VII treats spec-mandated tests as acceptance criteria.
Test tasks precede implementation within each story.

**Organization**: Grouped by user story so each can be implemented, tested, and landed
independently. Per the repository default, **each story is its own small CI-gated PR** with a
Conventional-Commit title (`feat`/`fix`/`chore` — never `test`/`style`).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Maps to a user story from spec.md (US1–US7)

## Path Conventions

Rust workspace. Hermetic logic in `crates/conformance/`, live execution in
`crates/parity-harness/`, test binaries in `crates/deacon/tests/`, version-controlled data under
`conformance/`. See plan.md § Project Structure.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module skeletons, data roots, and lane wiring so every later task has a home.

- [X] T001 Create hermetic module skeleton `crates/conformance/src/discovery/mod.rs` with empty submodules (`grammar`, `rng`, `generate`, `mutate`, `shrink`, `signature`, `queue`, `metamorphic`, `corpus`, `report`) and register `pub mod discovery;` in `crates/conformance/src/lib.rs`
- [X] T002 [P] Create live module skeleton `crates/parity-harness/src/discovery/mod.rs` with empty submodules (`campaign`, `differential`, `metamorphic_run`, `minimize`, `candidate`, `corpus_fetch`, `pipeline_proof`) and register `pub mod discovery;` in `crates/parity-harness/src/lib.rs`
- [X] T003 [P] Create the discovery data root: `conformance/discovery/findings.json`, `conformance/discovery/campaigns.json`, `conformance/discovery/corpus.json`, each `{"schemaVersion": 1, "records": []}`
- [X] T004 [P] Add `DiscoveryError` variants (malformed record, unresolvable reference, unknown channel, stale pin) to the domain error enum in `crates/conformance/src/lib.rs`
- [X] T005 [P] Add discovery variants (`OracleUnverified`, `CandidateTimeout`, `ShrinkBudgetExhausted`, `CorpusDigestMismatch`, `CorpusUnreachable`) to `HarnessError` in `crates/parity-harness/src/lib.rs`
- [X] T006 Add `[profile.discovery]` to `.config/nextest.toml` with an explicit `default-filter = 'binary(=discovery_campaign) | binary(=discovery_metamorphic)'` allow-list — NOT a `discovery_*` glob (research D9)
- [X] T007 Add `binary(=discovery_campaign)` and `binary(=discovery_metamorphic)` exclusions to the `default-filter` of all six existing profiles in `.config/nextest.toml` (`default`, `dev-fast`, `full`, `ci`, `mvp-integration`, `parity`)
- [X] T008 [P] Add `test-discovery`, `test-discovery-proof`, and `test-discovery-check` targets to `Makefile` per contracts/discovery-cli.md § Make targets
- [X] T009 [P] Add `target/discovery/` to `.gitignore`
- [X] T121 Assign nextest **test groups** to every new test binary in `.config/nextest.toml` across ALL profiles (constitution Principle VII): `discovery_campaign` → `docker-shared` for the container tier, `discovery_metamorphic` → `fs-heavy`, and the hermetic guards ungrouped. Add the override rules to `default`, `dev-fast`, `full`, `ci`, `mvp-integration`, `parity`, and `discovery` — a binary excluded from a `default-filter` still needs its group declared where it *does* run

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The hermetic spine every story depends on — deterministic randomness, the grammar,
the signature, and the queue as a persistence substrate.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T010 Implement the in-repo deterministic PRNG (SplitMix64 seed expansion → xoshiro256\*\* stream) in `crates/conformance/src/discovery/rng.rs`, with the algorithm identity documented as a `generatorVersion` component (research D2)
- [X] T011 [P] Unit-test the PRNG against published xoshiro256\*\* reference vectors in `crates/conformance/src/discovery/rng.rs` — this is what makes the stream a reviewable pin rather than an accident
- [X] T012 Implement grammar loading in `crates/conformance/src/discovery/grammar.rs`: read `conformance/inventory/constraints.json`, index the 469 non-annotation units by pointer and kind, expose lookup by schema pointer (research D1)
- [X] T013 [P] Unit-test grammar loading in `crates/conformance/src/discovery/grammar.rs` asserting per-kind unit counts (187 `type`, 117 `property-existence`, 41 `array-shape`, 20 `required`, 18 `union-alternative`, 14 `enum`+`const`) so a re-vendored inventory surfaces as a test change
- [X] T014 Implement `Signature`, the value-shape classifier, and the `sig-` id derivation in `crates/conformance/src/discovery/signature.rs`, consuming `parity_harness::normalize::ConfigDivergence` — derive only, never re-diff (research D3)
- [X] T015 [P] Unit-test value-shape classification in `crates/conformance/src/discovery/signature.rs` covering `present-absent`, `type-changed`, `ordering-changed` (array permutation detection), and `value-changed`
- [X] T016 Implement `Finding`, `Witness`, `FindingState`, and `Classification` record models in `crates/conformance/src/discovery/queue.rs` per data-model.md § 3
- [X] T017 [P] Implement `Campaign`, `PinnedInputSet`, `Budget`, and `CampaignOutcome` record models in `crates/conformance/src/discovery/queue.rs` per data-model.md § 4
- [X] T018 Implement the strict-JSON loader for `conformance/discovery/` in `crates/conformance/src/discovery/queue.rs`, rejecting unknown fields at load
- [X] T019 Implement the atomic writer (unique temp file + `fs::rename`) for the discovery data root in `crates/conformance/src/discovery/queue.rs`
- [X] T020 Implement signature-keyed upsert (insert a new finding, or append a witness to the existing one) in `crates/conformance/src/discovery/queue.rs` — the derived `fnd-` id makes duplicate findings unrepresentable
- [X] T021 Implement violation classes **D1** (malformed record, empty witnesses, undeclared channel, unresolvable campaign reference) and **D5** (pin absent from `revisions.json`) in `crates/conformance/src/discovery/queue.rs`
- [X] T022 Wire the `discovery` command group (`check`, `report`, `triage`, `split`, `scaffold`) into `crates/conformance/src/bin/conformance.rs` with the exit-status contract from contracts/discovery-cli.md
- [X] T023 [P] Create hermetic guard binary `crates/deacon/tests/discovery_hermetic.rs` asserting the discovery data root loads and validates clean
- [X] T024 [P] Register `discovery_hermetic` in the `default` and `dev-fast` nextest profiles in `.config/nextest.toml` — it MUST run in the fast lane (it is a guard, not a campaign)

**Checkpoint**: The hermetic spine exists. Stories can now proceed.

---

## Phase 3: User Story 1 - Find a difference nobody curated (Priority: P1) 🎯 MVP

**Goal**: Generate valid and near-valid configurations from the pinned grammar, mutate known-valid
fixtures, run both implementations, and report every normalized difference.

**Independent Test**: Run a campaign with a fixed seed on a machine with the verified oracle;
confirm it produces findings, that under 10% of candidates fail at document parsing, and that
re-running the seed reproduces both the candidates and the findings.

### Tests for User Story 1

- [X] T025 [P] [US1] Seed-reproduction acceptance test in `crates/deacon/tests/discovery_campaign.rs`: the same seed and pinned input set produce an identical ordered candidate sequence and an identical finding set across two runs (SC-001)
- [X] T026 [P] [US1] Trivial-failure-ceiling test in `crates/deacon/tests/discovery_campaign.rs`: `parseStageFailures / candidatesGenerated` stays below 10% (SC-002)
- [X] T027 [P] [US1] Mutation-category coverage test in `crates/deacon/tests/discovery_campaign.rs`: all eleven categories applied at least once, all eleven keys present in `mutationApplications` including zeroes (SC-003)
- [X] T028 [P] [US1] Oracle fail-loud test in `crates/deacon/tests/discovery_campaign.rs`: a missing or wrong-version oracle fails naming the cause and reports no findings — never a silent skip
- [X] T029 [P] [US1] Budget-exhaustion test in `crates/deacon/tests/discovery_campaign.rs`: an exhausted budget stops the campaign, sets `budgetExhausted`, and reports `spaceCoveredFraction`

### Implementation for User Story 1

- [X] T030 [US1] Implement constrained generation in `crates/conformance/src/discovery/generate.rs`: draw from the grammar so `required` keys are satisfied for valid candidates and violated deliberately for near-valid ones
- [X] T031 [US1] Implement the eleven-category mutation catalogue in `crates/conformance/src/discovery/mutate.rs` per data-model.md § 5, each application recording its `mop-` name
- [X] T032 [P] [US1] Unit tests per mutation operator in `crates/conformance/src/discovery/mutate.rs`, asserting each produces a schema-adjacent (not byte-corrupted) result
- [X] T033 [US1] Implement the campaign driver in `crates/parity-harness/src/discovery/campaign.rs`: seed, tier, budget, per-candidate timeout, and outcome accumulation
- [X] T034 [US1] Implement the differential comparison in `crates/parity-harness/src/discovery/differential.rs`, reusing `exec`, `oracle`, and `prereq` — no new process-execution path
- [X] T035 [US1] Wire `normalize::diff` output into `signature.rs` from `crates/parity-harness/src/discovery/differential.rs`, reusing the single normalization definition (FR-015 — a second path is a defect, not a feature)
- [X] T036 [US1] Implement already-characterized suppression in `crates/parity-harness/src/discovery/differential.rs`: a difference covered by an existing case, waiver, or tolerated difference reports as characterized and never enters the queue as new (FR-017)
- [X] T037 [US1] Implement unsafe-candidate discard-and-count in `crates/parity-harness/src/discovery/campaign.rs` (FR-011) and the unpinned-image guard for container-bound candidates (FR-012)
- [X] T038 [US1] Implement the `discovery-campaign` bin in `crates/parity-harness/src/bin/discovery-campaign.rs` with `--seed` **required** (never defaulted) and `--tier`/`--budget-seconds`/`--lane` per contracts/discovery-cli.md
- [X] T039 [US1] Implement campaign-outcome reporting in `crates/conformance/src/discovery/report.rs`, emitting all eleven `mutationApplications` keys always — an absent key is indistinguishable from a never-applied category (FR-010)
- [X] T040 [US1] Create the live test binary `crates/deacon/tests/discovery_campaign.rs` and verify it is selected by `[profile.discovery]`'s allow-list and excluded from all six other profiles (the allow-list entry itself lands in T006)
- [X] T122 [US1] Implement the outcome-only comparison guard in `crates/parity-harness/src/discovery/differential.rs` — relate exit status and structured content, never diagnostic message wording — with a test asserting two rejections differing only in wording produce no finding (FR-016)
- [X] T123 [US1] Implement zero-finding volume reporting in `crates/conformance/src/discovery/report.rs` and test in `crates/deacon/tests/discovery_campaign.rs` that a campaign finding nothing still reports `candidatesGenerated`/`candidatesExecuted`, so "nothing found" is distinguishable from "nothing ran" (FR-062)

**Checkpoint**: US1 answers the question the project cannot answer today — *does anything differ
outside what we curated?*

---

## Phase 4: User Story 2 - Minimize the difference and hand over a reviewable candidate (Priority: P1)

**Goal**: Reduce each finding's input while preserving its signature, then package a complete,
self-contained reviewable candidate.

**Independent Test**: Take a known difference on a large generated input, minimize it, and confirm
the result is smaller, reproduces the same signature, is minimal against the declared catalogue,
and is packaged with all six parts.

### Tests for User Story 2

- [ ] T041 [P] [US2] Signature-preservation test in `crates/conformance/src/discovery/shrink.rs` using a synthetic predicate: the reduced input yields the same signature as the original
- [ ] T042 [P] [US2] Minimality test in `crates/conformance/src/discovery/shrink.rs`: applying any single further catalogue step to a minimal result no longer preserves the signature (FR-021)
- [ ] T043 [P] [US2] Determinism test in `crates/conformance/src/discovery/shrink.rs`: the same finding and seed yield the identical minimal input (SC-004)
- [ ] T044 [P] [US2] Budget-exhaustion test in `crates/conformance/src/discovery/shrink.rs`: the best reduction is emitted with `isMinimal: false` and a reason — never silently presented as minimal
- [ ] T045 [P] [US2] Signature-drift test in `crates/conformance/src/discovery/shrink.rs`: a step that changes the signature is rejected for the finding under reduction and the new signature is captured as a separate candidate finding (FR-023)
- [ ] T046 [P] [US2] Candidate-completeness test in `crates/deacon/tests/discovery_campaign.rs`: every emitted candidate contains all six parts and is reproducible from the candidate plus its named pins (SC-005)

### Implementation for User Story 2

- [ ] T047 [US2] Implement the ordered seven-step structural reduction catalogue in `crates/conformance/src/discovery/shrink.rs` per data-model.md § 6, taking the reproduction predicate as a **parameter** so the strategy stays hermetic (research D4/D5)
- [ ] T048 [US2] Implement the live reproduction predicate in `crates/parity-harness/src/discovery/minimize.rs`, supplying it to `shrink.rs`
- [ ] T049 [US2] Implement candidate assembly in `crates/parity-harness/src/discovery/candidate.rs`, writing `target/discovery/candidates/<fnd-id>/` per data-model.md § 9
- [ ] T050 [US2] Write `raw.json` and `normalized.json` as **separate** files in `crates/parity-harness/src/discovery/candidate.rs` — raw and normalized evidence must never be conflated (FR-014)
- [ ] T051 [US2] Implement the suggested behavior mapping in `crates/parity-harness/src/discovery/candidate.rs`, emitting either a resolvable `bhv-` id or an explicit `{"match": "none"}` — never an invented id (FR-025)
- [ ] T052 [US2] Wire minimization into the campaign driver in `crates/parity-harness/src/discovery/campaign.rs` with a per-finding shrink budget
- [ ] T124 [US2] Test in `crates/deacon/tests/discovery_campaign.rs` that the container-backed tier is selectable independently of the configuration-resolution tier, so a campaign runs where Docker is unavailable (FR-060)

**Checkpoint**: Findings are cheap enough to triage and stable enough to deduplicate.

---

## Phase 5: User Story 3 - Keep the deterministic lane hermetic (Priority: P1)

**Goal**: Guarantee structurally that no discovery activity can reach a pull-request lane.

**Independent Test**: Run the deterministic lane with the network unavailable; confirm it passes,
selects no discovery program, and performs no fetch.

### Tests for User Story 3

- [X] T053 [P] [US3] No-network test in `crates/deacon/tests/discovery_hermetic.rs`: the hermetic discovery surface completes with zero network requests (SC-013)
- [X] T054 [P] [US3] Profile-selection structural test in `crates/deacon/tests/parity_registry_check.rs`: every discovery binary is selected by `[profile.discovery]` and by no pull-request profile; a mismatch fails (FR-057)
- [X] T055 [P] [US3] Never-gates test in `crates/deacon/tests/discovery_hermetic.rs`: a campaign reporting findings and a campaign reporting none both exit `0` (SC-014)

### Implementation for User Story 3

- [X] T056 [US3] Extend `crates/deacon/tests/parity_registry_check.rs` with discovery-lane wiring assertions: registry ↔ `crates/deacon/tests/*.rs` ↔ `.config/nextest.toml` agreement
- [X] T057 [US3] Extend `crates/deacon/tests/parity_registry_check.rs` asserting `deacon --help` gains no discovery surface (FR-059)
- [X] T058 [US3] Create `.github/workflows/discovery.yml` with a nightly `schedule` lane and a `workflow_dispatch` lane accepting `seed` and `budget` inputs, provisioning the pinned oracle
- [ ] T059 [US3] Enforce the exit-status contract in `crates/parity-harness/src/bin/discovery-campaign.rs`: status reflects whether the campaign ran, never what it found (FR-058)
  - **Blocked on US1 (T038)**, which creates that bin. The `.github/workflows/discovery.yml`
    invocation it needs is already in place, so this is a one-file follow-up once US1 lands.
- [X] T060 [US3] Register the discovery binaries in `fixtures/parity-corpus/registry.json` so `parity_registry_check` can enforce their wiring
  - Registered: `discovery_campaign`, `discovery_metamorphic` (role `live`),
    `discovery_hermetic`, `discovery_cli` (role `guard`). The `parity-harness` **bins**
    `discovery-campaign` (T038) and `discovery-proof` (T082) are not test binaries and,
    like `parity-report` / `conformance-snapshot` / `equivalence-report`, take no registry
    entry and no nextest override.

**Checkpoint**: A green pull-request run means exactly what it meant before this feature existed.

---

## Phase 6: User Story 4 - Classify and deduplicate what was found (Priority: P2)

**Goal**: Place every finding in one of six causes and collapse repeats, so the queue reflects
distinct problems rather than campaign volume.

**Independent Test**: Run two campaigns with different seeds over inputs known to trigger the same
underlying difference; confirm the queue holds one finding with two witnesses and one
classification.

### Tests for User Story 4

- [X] T061 [P] [US4] Exactly-one-classification test in `crates/deacon/tests/discovery_hermetic.rs` (SC-007) — `every_finding_carries_exactly_one_classification_or_is_visibly_unclassified`. The partition has **two** unclassified buckets, not one: `untriaged` and `split` (a split ancestor surrenders its classification to its children, Q10). Both are counted, so "no finding is in neither state" holds with both visible.
- [X] T062 [P] [US4] Signature-merge test in `crates/deacon/tests/discovery_hermetic.rs`: equal signatures from two campaigns collapse to one finding with two witnesses (SC-006)
- [X] T063 [P] [US4] Distinct-signature test in `crates/deacon/tests/discovery_hermetic.rs`: different signatures mapping to the same behavior stay distinct findings, grouped in the report but not merged (FR-031)
- [X] T064 [P] [US4] Non-promotable test in `crates/deacon/tests/discovery_hermetic.rs`: `normalizer-defect` and `fixture-defect` are rejected at promotion (FR-035)
- [X] T065 [P] [US4] No-longer-reproducing test in `crates/deacon/tests/discovery_hermetic.rs`: a finding that stops reproducing is reported with the campaign that last observed it, not deleted (FR-033)
- [X] T066 [P] [US4] Untriaged-bucket test in `crates/deacon/tests/discovery_hermetic.rs`: the count is visible, so "not yet looked at" never reads as "nothing found" (FR-029)
- [X] T067 [P] [US4] Admission-cap test in `crates/deacon/tests/discovery_campaign.rs`: exceeding the cap admits at most the cap, reports a non-zero `signaturesSuppressed`, and still exits `0` (SC-019) — `exceeding_the_admission_cap_suppresses_visibly_and_still_succeeds`, verified live against the pinned oracle. Uses `admission_cap = 1` so the boundary is genuinely reached, and compares against an effectively uncapped run of the same seed: at the default of 25 that comparison run itself suppressed 4 signatures, so the comparison had to be lifted above reviewer throughput to stay sound.

### Implementation for User Story 4

- [X] T068 [US4] Implement the `Classification` closed set and violation class **D2** in `crates/conformance/src/discovery/queue.rs` — the closed set already existed; D2 is new (`check_classifications` + `DiscoveryError::ClassificationArity`). "More than one classification" is unrepresentable rather than unchecked: `Finding` carries `Option<Classification>` and the strict loader rejects an array, so the arity rule reduces to its only reachable violation — zero where exactly one is required.
- [X] T069 [US4] Implement the finding state machine transitions in `crates/conformance/src/discovery/queue.rs` per data-model.md § 3 — `FindingState::may_transition_to` encodes exactly the diagram's five arrows, plus `Finding::{triage,promote,mark_no_longer_reproducing}` and `TransitionError`. Three plausible-looking arrows are absent on purpose and documented: `untriaged → no-longer-reproducing` (the state requires a classification, so the arrow would manufacture a D2 out of a campaign merely not re-observing something nobody had looked at), `no-longer-reproducing → promoted` (promotion needs current evidence), and `untriaged → split` (a split separates a judgement, so there must be one).
- [X] T070 [US4] Implement split with `splitFrom` lineage in `crates/conformance/src/discovery/queue.rs`, and make the deduplication rule skip split lineages so a reviewer's judgement is never silently reverted (FR-032) — `split_finding` produces one child per witness, each keyed by `Finding::derive_child_id` (`parent ‖ its witness ids`, since a child shares its parent's signature and so cannot use the signature derivation). `upsert_finding` refuses the whole **lineage**, including when the ancestor record is gone and only children remain — without that clause the id lookup misses and a fresh merged record resurrects under the ancestor's id. `check` gained the child-id rule (replacing the blanket exemption) and the Q10 ≥2-children rule.
- [X] T071 [US4] Implement the per-campaign admission cap (default 25, research D10) in `crates/parity-harness/src/discovery/campaign.rs`, always reporting `signaturesSuppressed` — never a silent truncation — the cap and its suppression accounting were already implemented by US1. **One gap fixed**: the cap was measured against `admitted`, which also counts findings the standing queue already carried and this run merely re-witnessed, so a queue grown past the cap would reach it on re-witnesses alone and freeze out every genuinely new signature forever. It now counts only *newly distinct* signatures, per FR-034b's wording.
- [X] T072 [US4] Implement `discovery triage` and `discovery split` in `crates/conformance/src/bin/conformance.rs`, the only writers of `classification` — both now delegate to the queue's state machine rather than re-deciding the lifecycle, and `split` authors the children (previously deferred to this task). Covered end to end by `split_writes_a_lineage_the_checker_accepts_and_the_deduplication_never_re_merges`.
- [X] T073 [US4] Implement `discovery report` in `crates/conformance/src/discovery/report.rs` emitting byte-stable `target/discovery/queue.{json,md}` with the five buckets from quickstart.md § 2 — `untriaged` (counted), `triaged`, `no-longer-reproducing`, `promoted`, `pin-stale` — the buckets existed; this adds the FR-031 grouping view (`BehaviorIndex`/`FindingGroup`, keyed by a *reviewed* behavior where a promoted finding's case resolves, else by observable path) and the queue-level suppression total, which is what tells a reviewer the queue is a **sample** rather than everything.
- [X] T125 [US4] Add a guard test in `crates/deacon/tests/discovery_hermetic.rs` asserting no discovery source file writes to or extends the `allowedDifferences` mechanism — a discovery program authoring a tolerance would let a difference disappear by being observed (FR-018) — `no_discovery_source_writes_to_the_allowed_difference_mechanism`. Scans the hermetic **and** live halves (the live half is the one holding the registry it would have to write to). *Reading* stays permitted — FR-017 requires it — and the guard asserts the read still happens, so a scan that matched nothing cannot pass by checking nothing.

**Checkpoint**: The nightly report is a triage queue, not noise.

---

## Phase 7: User Story 5 - Promote a finding only through review (Priority: P2)

**Goal**: Make promotion a human act with a stable behavior identity and a disposition, and make
automatic promotion structurally impossible.

**Independent Test**: Attempt to have discovery write into the deterministic record and confirm it
cannot; then promote a finding by hand and confirm it validates as an ordinary case.

### Tests for User Story 5

- [ ] T074 [P] [US5] No-write-path test in `crates/deacon/tests/discovery_hermetic.rs`: no discovery program can write a behavior, case, waiver, tolerated difference, disposition, or snapshot — modelled on `only_the_refresh_bin_writes_committed_snapshots` (SC-008)
- [ ] T075 [P] [US5] Promotion-validates test in `crates/deacon/tests/discovery_hermetic.rs`: a promoted finding's case passes full record validation including `scenarioContext` and obligation updates (SC-009)
- [ ] T076 [P] [US5] Missing-identity test in `crates/deacon/tests/discovery_hermetic.rs`: a promotion lacking a behavior identity or a disposition fails validation naming what is missing (FR-038)
- [ ] T077 [P] [US5] Certify-isolation test in `crates/deacon/tests/discovery_hermetic.rs`: `certify`'s result with a queue holding unreviewed findings is identical to its result with an empty queue (SC-018)
- [ ] T078 [P] [US5] Injected-difference traversal test in `crates/deacon/tests/discovery_hermetic.rs`: an injected difference traverses generation → comparison → minimization → candidate → classification → promotable, and an injection that never lands fails loudly rather than reading as "found nothing" (SC-016)

### Implementation for User Story 5

- [ ] T079 [US5] Implement `discovery scaffold` in `crates/conformance/src/bin/conformance.rs` emitting skeleton behavior/case/fixture records to **stdout only**, with `UNREVIEWED` sentinels the loader rejects — generation never writes a hand-authored file
- [ ] T080 [US5] Implement violation class **D3** (`promotedTo` must resolve to a real case) in `crates/conformance/src/discovery/queue.rs`
- [ ] T081 [US5] Implement the pipeline proof in `crates/parity-harness/src/discovery/pipeline_proof.rs` using `parity_harness::inject::perturb_source`'s sealed `EvidenceSource` boundary, so injecting into an observer's return value does not compile (research D7)
- [ ] T082 [US5] Implement the `discovery-proof` bin in `crates/parity-harness/src/bin/discovery-proof.rs`, exiting non-zero on a failed traversal **or** an inapplicable injection
- [ ] T083 [US5] Add the no-write-path guard to `crates/deacon/tests/discovery_hermetic.rs` asserting no discovery source file references a registry or snapshot write helper
- [ ] T126 [US5] Implement the tolerate path in `crates/conformance/src/bin/conformance.rs`: `discovery scaffold --tolerate` emits a scoped `wvr-` waiver skeleton (rationale + `expires`) plus the scoped `allowedDifferences` entry that references it, to **stdout only**; add a test in `crates/deacon/tests/discovery_hermetic.rs` rejecting a blanket or unscoped scope (FR-041)

**Checkpoint**: A stochastic process cannot author the record it is tested against.

---

## Phase 8: User Story 6 - Assert what cannot be written down (Priority: P2)

**Goal**: Metamorphic relations, each grounded in the specification, evaluable against deacon alone.

**Independent Test**: Apply each declared transformation to known-valid configurations and confirm
the relation holds; deliberately break one and confirm the failure names the relation and the
transformation.

**Note**: This tier needs no oracle, no Docker, and no network (research D12) — a contributor
without the devcontainer CLI installed can develop and test it locally.

### Tests for User Story 6

- [X] T084 [P] [US6] Invariance tests in `crates/deacon/tests/discovery_metamorphic.rs` for formatting, JSONC comments/trailing commas, and key order within unordered maps (FR-044)
- [X] T085 [P] [US6] Path-relocation test in `crates/deacon/tests/discovery_metamorphic.rs`: results equal modulo the declared tokenization, with any residual reported (FR-046)
- [X] T086 [P] [US6] Lifecycle-equivalence test in `crates/deacon/tests/discovery_metamorphic.rs` across the permitted string/array/object forms
- [X] T087 [P] [US6] Sensitivity test in `crates/deacon/tests/discovery_metamorphic.rs`: permuting a declaration-ordered collection MUST change the result, and a failure to change is reported as a finding (FR-043)
- [X] T088 [P] [US6] Ground-required test in `crates/conformance/src/validate.rs`: a relation with a missing or unresolvable `ground` is **V31** (SC-010)
- [X] T089 [P] [US6] Mandated-family test in `crates/conformance/src/validate.rs`: a family from data-model.md § 7 with no record is **V32**
- [X] T090 [P] [US6] Inert-relation test in `crates/deacon/tests/discovery_metamorphic.rs`: deliberately breaking each relation causes exactly that relation to fail and be named — zero relations are inert (SC-011)

### Implementation for User Story 6

- [X] T091 [US6] Implement the `MetamorphicRelation` model in `crates/conformance/src/discovery/metamorphic.rs` per contracts/metamorphic-catalogue.md
- [X] T092 [US6] Author `conformance/registry/metamorphic.json` with the seven mandated relations, each naming a resolvable `clu-` or `bhv-` ground and a rationale
- [X] T093 [US6] Extend the registry loader in `crates/conformance/src/load.rs` to read `metamorphic.json`
- [X] T094 [US6] Implement violation classes **V31** and **V32** in `crates/conformance/src/validate.rs`
- [X] T095 [US6] Implement deacon-only relation evaluation in `crates/parity-harness/src/discovery/metamorphic_run.rs`
- [ ] T096 [US6] Add the `metamorphic` tier to the campaign driver in `crates/parity-harness/src/discovery/campaign.rs`, requiring no external prerequisite
- [X] T097 [US6] Create the live test binary `crates/deacon/tests/discovery_metamorphic.rs` and verify it is selected by `[profile.discovery]` and excluded from all six other profiles (the allow-list entry itself lands in T006)
- [X] T127 [US6] Implement metamorphic failure-candidate emission in `crates/parity-harness/src/discovery/metamorphic_run.rs` and test in `crates/deacon/tests/discovery_metamorphic.rs` that the candidate names the relation, the transformation applied, both inputs, and both normalized outputs (FR-047)
- [X] T098 [US6] Add the **V31** and **V32** rows to the violation-class index in `conformance/RULES.md`, keeping `validate.rs`/`RULES.md` lockstep

**Checkpoint**: The project can now catch deacon and the reference being *consistently* wrong —
which the differential cannot see, because both sides agree.

---

## Phase 9: User Story 7 - Watch the real ecosystem (Priority: P3)

**Goal**: A pinned real-world workspace corpus as an ecological canary, with provenance recorded
and mutable references rejected.

**Independent Test**: Run the corpus canary in the network-backed lane; confirm every entry
resolves to an immutable commit, provenance is recorded, and a mutable reference is rejected.

### Tests for User Story 7

- [ ] T099 [P] [US7] Immutable-reference test in `crates/deacon/tests/discovery_hermetic.rs`: a branch, tag, `HEAD`, or `latest` is **D4** and rejected hermetically, with no network (SC-012)
- [ ] T100 [P] [US7] Provenance test in `crates/deacon/tests/discovery_hermetic.rs`: every entry records repository, commit, path, and content digest
- [ ] T101 [P] [US7] Digest-mismatch test in `crates/deacon/tests/discovery_campaign.rs`: a fetched entry whose digest disagrees fails loudly for that entry (FR-051)
- [ ] T102 [P] [US7] Unreachable-entry test in `crates/deacon/tests/discovery_campaign.rs`: an unreachable entry is distinguished from one that ran and found nothing (FR-052)
- [ ] T103 [P] [US7] Pipeline-parity test in `crates/deacon/tests/discovery_campaign.rs`: a corpus finding enters the same minimization, classification, deduplication, and promotion pipeline, naming its upstream provenance
- [ ] T104 [P] [US7] Not-a-seed test in `crates/deacon/tests/discovery_hermetic.rs`: no corpus entry appears among the generator's mutation seeds (FR-008a/FR-054a)

### Implementation for User Story 7

- [ ] T105 [US7] Implement the `CorpusEntry` model and violation class **D4** in `crates/conformance/src/discovery/corpus.rs`, requiring a 40-hex commit
- [ ] T106 [US7] Port the 33 pinned entries from `fixtures/parity-corpus/fetch_realworld_corpus.py` into `conformance/discovery/corpus.json` with `contentDigest: null`
- [ ] T107 [US7] Implement fetch with digest verification in `crates/parity-harness/src/discovery/corpus_fetch.rs`: record the digest on first materialization, verify it thereafter
- [ ] T108 [US7] Add the `corpus` tier to the campaign driver in `crates/parity-harness/src/discovery/campaign.rs`, gated to the network-backed lane
- [ ] T128 [US7] Add a **weekly** `schedule` trigger for the corpus tier to `.github/workflows/discovery.yml`, separate from the nightly hermetic campaign — an ecological canary that runs only on request cannot warn anyone (FR-056, research D10)
- [ ] T109 [US7] Resolve the fate of `fixtures/parity-corpus/fetch_realworld_corpus.py` — retire it now that the manifest is Rust-owned, or keep it as an exploratory aid — and update `fixtures/parity-corpus/README.md` and the `res-realworld-corpus-not-vendored` residual accordingly (research D8 left this open deliberately)

**Checkpoint**: All seven stories are independently functional.

---

## Phase 10: Polish & Cross-Cutting Concerns

- [ ] T110 [P] Add an "Exploratory Parity Discovery" section to `CLAUDE.md` covering the two data roots, the hermetic/live split, the D-class vs V-class distinction, and the never-gates rule
- [ ] T111 [P] Add the discovery-lane row to the gate table in `CLAUDE.md` § "Parity & Conformance", making explicit that discovery is a third lane that gates nothing
- [ ] T112 [P] Document the gap-vs-finding distinction in `conformance/RULES.md`: a finding is a *candidate* for an assertion and never blocks; a gap is missing coverage and always blocks
- [ ] T113 Run the full quickstart walkthrough in `specs/025-exploratory-parity-discovery/quickstart.md` end to end and correct any drift
- [ ] T114 [P] Verify SC-002 and SC-004 thresholds against three real campaigns and record the observed values in `specs/025-exploratory-parity-discovery/research.md` (a threshold nobody measured is a guess)
- [ ] T115 [P] Verify SC-017 against a real candidate under `target/discovery/candidates/`: a reviewer on a different machine reproduces it from the candidate plus its named pins alone in under 10 minutes; record the walkthrough in `specs/025-exploratory-parity-discovery/quickstart.md` § Troubleshooting if any step proves unclear
- [ ] T116 Confirm the three-covering-case floor holds for every channel the discovery machinery relies on: cross-check `conformance/registry/channels.json` against `conformance/registry/regressions.json` and add any missing `reg-` record (V30)
- [ ] T117 Run `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` across the workspace
- [ ] T118 Run `make test-nextest` (full gate) and confirm no discovery binary is selected by it
- [ ] T119 Run `make test-discovery-check` and `make test-discovery-proof` and confirm both pass
- [ ] T120 Confirm `cargo run -p deacon-conformance -- certify` produces an unchanged verdict with a populated findings queue
- [ ] T129 Verify SC-015 against the nightly lane: the scheduled campaign completes within 30 minutes, the per-candidate timeout is 60 s hermetic and 5 min container-backed, and a candidate exceeding its timeout is discarded and counted rather than hanging the run

---

## Deferred Work

**None.** Every research decision resolved to an in-scope task. The one item research D8 left
open — the fate of the Python corpus fetcher — is tracked as **T109** rather than deferred, per
the constitution's requirement that a specification is not complete while deferred tasks remain.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies
- **Foundational (Phase 2)**: depends on Setup — **blocks all user stories**
- **US1, US2, US3 (Phases 3–5, all P1)**: depend on Foundational
- **US4, US5, US6 (Phases 6–8, P2)**: depend on Foundational
- **US7 (Phase 9, P3)**: depends on Foundational
- **Polish (Phase 10)**: depends on all desired stories

### User Story Dependencies

| Story | Depends on | Notes |
|---|---|---|
| US1 | Foundational | independent |
| US2 | Foundational | shrink strategy is hermetic and testable with a synthetic predicate, so US2's core lands without US1; T046 and T052 integrate with US1 |
| US3 | Foundational | fully independent — smallest story, highest severity if skipped |
| US4 | Foundational | T067 exercises the cap through a campaign, so it integrates with US1 |
| US5 | Foundational | T078's traversal proof exercises US1+US2+US4; the guards (T074, T083) are independent |
| US6 | Foundational | fully independent, and needs **no oracle, Docker, or network** |
| US7 | Foundational | T101–T103 integrate with US1's pipeline |

### Within Each User Story

- Tests are written first and MUST fail before implementation
- Models before services; hermetic logic before its live driver
- Story complete before moving to the next priority

### Parallel Opportunities

- Setup: T002–T005, T008, T009 in parallel (T006/T007 both edit `.config/nextest.toml` — serialize)
- Foundational: T011, T013, T015, T017 in parallel; T023/T024 in parallel after T022
- Every story's test tasks are `[P]` — different assertions in the same or different files, written before implementation
- Across stories: US3 and US6 are fully independent of the others and of each other, so they are the two best candidates for parallel staffing

---

## Parallel Example: User Story 1

```bash
# Write all US1 acceptance tests together (they must fail first):
Task: "Seed-reproduction test in crates/deacon/tests/discovery_campaign.rs"
Task: "Trivial-failure-ceiling test in crates/deacon/tests/discovery_campaign.rs"
Task: "Mutation-category coverage test in crates/deacon/tests/discovery_campaign.rs"
Task: "Oracle fail-loud test in crates/deacon/tests/discovery_campaign.rs"
Task: "Budget-exhaustion test in crates/deacon/tests/discovery_campaign.rs"
```

## Parallel Example: Foundational

```bash
# The four hermetic unit-test tasks touch different modules:
Task: "PRNG reference-vector tests in crates/conformance/src/discovery/rng.rs"
Task: "Grammar per-kind count tests in crates/conformance/src/discovery/grammar.rs"
Task: "Value-shape classification tests in crates/conformance/src/discovery/signature.rs"
Task: "Campaign record model in crates/conformance/src/discovery/queue.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1)

1. Phase 1: Setup
2. Phase 2: Foundational — **critical, blocks everything**
3. Phase 3: User Story 1
4. **STOP and VALIDATE**: run a seeded campaign; confirm reproducibility and the parse-failure ceiling
5. US1 alone answers the question the project cannot answer today

### The no-oracle path

A contributor without the pinned devcontainer CLI can still deliver a complete vertical slice:
**Phase 1 → Phase 2 → Phase 8 (US6)**. The metamorphic tier exercises generation → comparison →
signature → candidate with no oracle, no Docker, and no network (research D12), so it proves the
entire hermetic spine before any live provisioning exists. If oracle provisioning is a bottleneck,
build this first and treat US1 as the second increment.

### Recommended landing order

`Setup + Foundational` → **US3** (smallest, protects the PR lane before anything stochastic
exists) → **US1** (MVP) → **US2** → **US4** → **US5** → **US6** → **US7**.

US3 is deliberately pulled ahead of US1 despite both being P1: it costs little, and landing the
lane guards *before* the first campaign binary exists means there is never a window in which a
discovery program could be selected by a pull-request lane.

### Parallel team strategy

After Foundational, three tracks run cleanly without contention:

- **Track A**: US1 → US2 (the differential spine)
- **Track B**: US3 → US5 (lane isolation and the promotion guards)
- **Track C**: US6 → US7 (metamorphic, then corpus)

Track C needs no oracle until US7.

---

## Notes

- `[P]` = different files, no dependencies on incomplete tasks
- **`[P]` on test tasks sharing one binary** (T025–T029, T061–T066, T074–T078, T099/T100/T104)
  means independent test **functions** with no shared fixture or state: they can be authored
  concurrently, but they land in one file, so expect a merge conflict and resolve it as the
  **union** of the functions — the same convention already used for `.config/nextest.toml`
  `binary(=…)` clauses
- Task IDs T121–T129 were added by `/speckit.analyze` remediation; they sit in their correct
  phase, and phase placement (not numeric order) determines execution order
- Each story is its own CI-gated PR with a Conventional-Commit title (`feat`/`fix`/`chore` —
  `test` and `style` fail the PR-title check)
- T006 and T007 both edit `.config/nextest.toml`; expect a conflict if two stories add binaries
  concurrently, and resolve to the **union** of the `binary(=…)` clauses
- Adding any new observable channel requires a `reg-` regression record (V30) and the
  three-covering-case floor — T116 checks this
- Never make `certify` non-blocking or delete a real gap to go green; discovery exists to *find*
  differences, not to make them disappear
