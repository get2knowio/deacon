---
description: "Task list for Normative Clause Inventory (feature 021)"
---

# Tasks: Normative Clause Inventory

**Input**: Design documents from `/workspaces/deacon/specs/021-normative-clause-inventory/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: INCLUDED — spec FR-027/FR-028 mandate acceptance tests and constitution VII
requires all spec-mandated tests before a feature is complete. Test tasks are written to
fail first, then made green by the implementation tasks in the same story.

**Organization**: Tasks are grouped by the four user stories (US1–US4) so each is an
independently testable increment. Per research Decision 10 the `certify` gate is wired in
the final story (US4), so the gate on `main` never goes red mid-rollout; the feature merges
as one CI-gated PR with all phases complete.

**Crate**: everything lands in the dev-only `deacon-conformance` crate
(`crates/conformance/`) and the version-controlled `conformance/` data tree. No consumer
CLI, Docker, network, or LLM in any test.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1–US4; Setup/Foundational/Polish tasks carry no story label

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Vendor the pinned prose and stand up test scaffolding.

- [X] T001 Vendor **all 18** ratified `docs/specs/` Markdown files at `113500f4` into `conformance/spec/113500f4/` (network, one-time) and write `conformance/spec/113500f4/manifest.json` with per-document `key`, `file`, `upstreamUrl`, `sha256`, and `scope` — 14 `consumer` (`reference`, `json-reference`, `supporting-tools`, `image-metadata`, `lockfile`, `devcontainer-id-variable`, `parallel-lifecycle`, `features-lifecycle-scripts`, `features-user-env`, `feature-dependencies`, `gpu-host-requirement`, `declarative-secrets`, `secrets-support`, `features-legacy-ids`) and 4 `authoring` (`features`, `features-distribution`, `templates`, `templates-distribution`) — per research Decision 6, referencing the existing `rev-spec-113500f4` record. Reject any CR bytes.
- [X] T002 [P] Create fixture prose under `fixtures/conformance/prose/` exercising: a multi-requirement paragraph, a moved-heading old/new pair, an ambiguous/hedged clause, an authoring-scope document, an I/O contract inside a code fence, and a whitespace-only (immaterial) reflow; plus `fixtures/conformance/clause-drift/{old,new}` clause-inventory files for diff tests.
- [X] T003 [P] Register the five new test binaries (`clause_extraction`, `clause_determinism`, `clause_baseline`, `clause_diff`, `clause_classification_join`) in `.config/nextest.toml` in ALL profiles (the `[profile.default]` override, the `[profile.dev-fast]` `default-filter` inclusion, and the `full`/`ci` profiles) as fast, non-docker binaries.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Records, loaders, and the deterministic prose primitives every story needs.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T004 Extend `RecordType`, `prefix()`, `from_prefix()`, and the `parse_id` grammar regex in `crates/conformance/src/model.rs` with the `clu` (ClauseUnit) and `clc` (ClauseClassification) prefixes.
- [X] T005 Add the serde model structs/enums in `crates/conformance/src/model.rs` (`deny_unknown_fields`, camelCase, kebab-case enum wire values): `SpecManifest`, `SpecDocument`, `DocumentScope`, `ClauseInventory`, `ClauseUnit`, `Strength`, `Testability`, `ClauseLocation`, and `ClauseClassification` (with the `clause`-XOR-`document` invariant).
- [X] T006 Add the new `LoadError` variants (`SpecFingerprintMismatch`, `ExcerptNotFoundAtAnchor`, `StrengthKeywordMismatch`, `AmbiguousClauseUnclassified`, `ClauseInventoryOutOfDate`, `DocumentScopeOnConsumerDoc`) and the loaders in `crates/conformance/src/load.rs`: `load_spec_manifest` (SHA-256 fingerprint verification), `load_clause_inventory` (absent → `Ok(None)`), and the `clause-classifications/` collection loader.
- [X] T007 [P] Implement `crates/conformance/src/prose/mod.rs`: a byte-deterministic ATX-heading tree with per-heading text spans and code-fence awareness (anchor = GitHub-style slug of the heading), exposing section lookup used for excerpt-present-at-anchor checks.
- [X] T008 [P] Implement `crates/conformance/src/prose/normalize.rs`: the pure `normalize_substance(excerpt) -> String` and its SHA-256 `fingerprint` (research Decision 3), with exhaustive unit tests for whitespace/markdown immateriality.
- [X] T009 [P] Implement `crates/conformance/src/prose/strength.rs`: the pure `detect_strength(excerpt) -> Option<Strength>` RFC-2119 keyword map (research Decision 4), with unit tests for MUST/SHOULD/MAY families and the `None` (ambiguous) case.
- [X] T010 Wire module declarations, the `CURRENT_SPEC_PIN` constant, and the `clause_paths_for(registry_dir)` sibling-path helper (`<registry>/../spec/<pin>` + `<registry>/../inventory/clauses.json`) in `crates/conformance/src/lib.rs`, mirroring `inventory_paths_for`.

**Checkpoint**: records, loaders, and prose primitives compile and unit-test green.

---

## Phase 3: User Story 1 - Build the complete clause inventory (Priority: P1) 🎯 MVP

**Goal**: A complete, byte-stable committed `conformance/inventory/clauses.json` produced by
deterministic canonicalization of authored (optionally LLM-proposed) clause records against
the vendored prose — no LLM/network in the path.

**Independent Test**: Run `clause generate` then `clause check` on the pinned inputs and on
fixtures; every fixture requirement appears once with correct provenance/strength, a
multi-requirement paragraph splits, and two runs are byte-identical.

### Tests for User Story 1 ⚠️ (write first, ensure they FAIL)

- [X] T011 [P] [US1] `crates/conformance/tests/clause_extraction.rs`: multi-requirement paragraph splits into distinct clauses, strength detection (MUST/SHOULD/MAY/algorithm/io-contract/descriptive), ambiguity surfaced (not auto-promoted to MUST), I/O contract in a code fence captured, descriptive text NOT recorded as a requirement — over `fixtures/conformance/prose/`.
- [X] T012 [P] [US1] `crates/conformance/tests/clause_determinism.rs`: regeneration byte-identical to the committed inventory, two in-memory regenerations identical, IDs stable across runs, no CR bytes (pinned + fixtures).
- [X] T013 [P] [US1] `crates/conformance/tests/clause_baseline.rs`: pinned-baseline assertions (FR-028) — specific well-known clauses of the pinned prose present with expected strength and provenance; per-document clause counts.

### Implementation for User Story 1

- [X] T014 [US1] Implement `crates/conformance/src/clause.rs`: `ClauseUnit` canonicalization, `derive_clause_id` (substance-anchored `hash8` over `document ‖ normalize_substance`, location excluded — research Decision 2), same-substance location merge, canonical `render`, and atomic `write_clauses` (temp-file + `fs::rename`, reusing `inventory.rs` discipline).
- [X] T015 [US1] Add the fail-loud integrity checks used by `generate` in `crates/conformance/src/clause.rs` (with `crates/conformance/src/prose/`): excerpt-present-at-anchor (`ExcerptNotFoundAtAnchor`) and strength-label ↔ excerpt-keyword cross-check (`StrengthKeywordMismatch`); descriptive clauses must not hide a mandatory keyword.
- [X] T016 [US1] Add the `Clause { command: ClauseCommand }` subcommand and the `clause generate` handler in `crates/conformance/src/bin/conformance.rs`: verify spec fingerprints, canonicalize committed records against vendored prose, write byte-stable `conformance/inventory/clauses.json` (exit 1 on any integrity error, never a partial file).
- [X] T017 [US1] Implement the `clause check` handler in `crates/conformance/src/bin/conformance.rs`: regenerate in memory and byte-compare against the committed inventory (`ClauseInventoryOutOfDate` with a compact new/removed/moved/changed summary).
- [X] T018 [US1] Author the clause records for the **14 consumer documents** (`reference`, `json-reference`, `supporting-tools`, `image-metadata`, `lockfile`, `devcontainer-id-variable`, `parallel-lifecycle`, `features-lifecycle-scripts`, `features-user-env`, `feature-dependencies`, `gpu-host-requirement`, `declarative-secrets`, `secrets-support`, `features-legacy-ids`) into `conformance/inventory/clauses.json` (LLM-proposed + human-reviewed; multi-requirement paragraphs split; ambiguous language labeled `ambiguous`, never `must`).
- [X] T019 [US1] Author the clause records for the **4 authoring documents** (`features`, `features-distribution`, `templates`, `templates-distribution`) into `conformance/inventory/clauses.json` (same file as T018 — sequential; full coverage per FR-015, including the consumer install/apply clauses inside `features`/`templates`).
- [X] T020 [US1] Run `clause generate` to canonicalize the committed inventory; make T011–T013 pass (green MVP: the inventory exists, is complete, and regenerates byte-identically).

**Checkpoint**: `clause generate`/`clause check` work; `clauses.json` is committed, complete, and byte-stable — independently demoable.

---

## Phase 4: User Story 2 - Classify every clause under consumer-only scope (Priority: P2)

**Goal**: Every clause carries exactly one effective disposition (per-clause for consumer
docs; document-scope not-applicable for authoring docs); ambiguous/unclassified clauses are
blocking review items; the join is machine-enforced (V11–V15).

**Independent Test**: Classify a subset, leave one clause ambiguous and one consumer clause
unclassified → `validate` reports exactly those two as blocking; an authoring document-scope
not-applicable does not block and stays listed with rationale.

### Tests for User Story 2 ⚠️ (write first, ensure they FAIL)

- [X] T021 [P] [US2] `crates/conformance/tests/clause_classification_join.rs`: V11 stale, V12 unclassified/duplicate AND unresolved-`ambiguous`-blocks, V13 malformed (id-tail mismatch, behaviors/rationale arity, `clause`-XOR-`document`, document-scope on a `consumer` doc), V15 (strength/keyword mismatch, excerpt-not-found-at-anchor, descriptive-hides-keyword), the `UNREVIEWED` sentinel load rejection, document-scope resolution order, and authoring-scope not-applicable non-blocking.

### Implementation for User Story 2

- [X] T022 [US2] Generalize V11–V14 in `crates/conformance/src/validate.rs` from "constraint unit" to "inventory unit": run `join_inventory`/`InventoryJoin` for the clause inventory + clause classifications, and extend `check_classification_shape` for the `clc` id-tail mirror, `clause`-XOR-`document`, and document-scope-only-on-authoring rules.
- [X] T023 [US2] Add V14 provenance for clauses in `crates/conformance/src/validate.rs`: spec-manifest fingerprint verification, `clauses.json` `revision` = the `rev-spec-*` pin, and committed-inventory byte-identity vs canonicalized regeneration.
- [X] T024 [US2] Add the new **V15** clause↔source-integrity class to `crates/conformance/src/validate.rs` (strength/keyword agreement, descriptive-hides-keyword, excerpt-present-at-anchor), reported in one pass with all other violations.
- [X] T025 [US2] Implement the `clause scaffold` handler in `crates/conformance/src/bin/conformance.rs`: emit `UNREVIEWED`-sentinel `clc` skeletons for unclassified clauses (per-clause for consumer/ambiguous; one per-document skeleton for authoring docs); never write the registry.
- [X] T026 [US2] Add the clause-inventory section to `crates/conformance/src/report.rs` (`report.{json,md}`): counts by strength/testability/document, disposition tallies, unclassified + ambiguous-pending lists; byte-stable (no timestamps/paths).
- [X] T027 [P] [US2] Author per-clause `clc` classifications for the **14 consumer documents** — one `conformance/registry/clause-classifications/<doc-key>.json` file per document (behavior-mapped with existing/extended `bhv-` ids per research Decision 11; `non-testable`/`not-applicable` with rationale). Several clauses MAY share one behavior (FR-010).
- [X] T028 [P] [US2] Author the document-scope `not-applicable` defaults for the four authoring documents in `conformance/registry/clause-classifications/authoring.json` (one `clc-doc-<key>` record each, with consumer-only-scope rationale), PLUS per-clause `behavior-mapped` overrides for the consumer install/apply clauses inside `features`/`templates` and per-clause records for any authoring clause labeled `ambiguous` (a blanket default never covers a consumer or ambiguous clause — research Decision 7).
- [X] T029 [US2] Update `conformance/RULES.md` in lockstep: generalize V11–V14 wording to "inventory unit (constraint or clause)", add V15, and describe the clause inventory + document-scope resolution rule (keep RULES.md ↔ `validate.rs` in sync).

**Checkpoint**: `validate` enforces V11–V15 over clauses; 100% of clauses classified; ambiguous/unclassified are blocking.

---

## Phase 5: User Story 3 - Detect and review upstream prose drift (Priority: P3)

**Goal**: A deterministic `clause diff` that reports new, removed, **moved**, and materially
changed clauses (moves first-class and non-blocking; immaterial reflow excluded).

**Independent Test**: Diff two fixture revisions differing by one added, one removed, one
moved-heading, and one reworded clause → exactly those with correct kinds; a formatting-only
edit is immaterial; added/changed surface as blocking drift, moved keeps its disposition.

### Tests for User Story 3 ⚠️ (write first, ensure they FAIL)

- [X] T030 [P] [US3] `crates/conformance/tests/clause_diff.rs`: new/removed/moved/changed/nonMaterial buckets over `fixtures/conformance/clause-drift/{old,new}`; a moved clause keeps its id (and disposition); a reworded clause yields new id + stale old id; immaterial reflow reported non-material; deterministic ordering; no disposition inherited by wording similarity.

### Implementation for User Story 3

- [X] T031 [US3] Implement `crates/conformance/src/clause_diff.rs`: fingerprint-keyed match (research Decision 9) producing `new_clauses`/`removed`/`moved`/`changed`/`nonMaterial`, deterministically sorted by `(document, heading, id)`, with `render_json` and `render_md` mirroring `diff.rs`.
- [X] T032 [US3] Implement the `clause diff <old> <new> [--format json|md] [--out <file>]` handler in `crates/conformance/src/bin/conformance.rs` (exit 1 on unreadable input; empty diff is exit 0).

**Checkpoint**: drift review is deterministic and move-aware; independently demoable on fixtures.

---

## Phase 6: User Story 4 - Trust the committed inventory offline, without an LLM (Priority: P4)

**Goal**: Wire the `certify` gate to block on clause V11–V15 (last, per Decision 10), prove
the whole CI-facing path is offline + LLM-free, and establish clause↔behavior traceability.

**Independent Test**: Run `validate`/`clause diff`/`certify` with no network and no model →
deterministic completion; any clause resolves from provenance to a real pinned location and
(for consumer clauses) to a behavior and back; an artificial unclassified clause blocks
`certify`.

### Tests for User Story 4 ⚠️ (write first, ensure they FAIL)

- [X] T033 [P] [US4] Extend the existing (already-registered) `crates/conformance/tests/gap_certification.rs`: an unclassified/stale clause blocks `certify`; `not-applicable`/`non-testable` clause dispositions do not; the real registry certifies clean once the gate is wired. (Extends an existing 020 binary — no new test binary, so no nextest change.)
- [X] T034 [P] [US4] Add a traceability test (`crates/conformance/tests/traceability.rs` extension): every consumer clause maps to an existing behavior and each mapped behavior is reachable back from its clauses (FR-026), checked deterministically and offline.

### Implementation for User Story 4

- [X] T035 [US4] Wire the clause blockers into `crates/conformance/src/certify.rs`: `certify` exits 1 iff any gap OR uncovered in-profile behavior OR any clause V11–V15 (in addition to 020's constraint blockers); waivers listed non-blocking; no flag bypass, no model/network.
- [X] T036 [US4] Supersede the hand-written prose source units in `conformance/registry/sources/spec.json` (FR-026 traceability / no dual bookkeeping): move their behavior links onto the corresponding `clc` classifications and retire the superseded records in the same change, so each behavior back-traces to clauses through exactly one path.
- [X] T037 [US4] Add the offline/LLM-free assertion coverage: confirm (in-test and in `contracts/cli-clause.md`) that `generate`/`check`/`validate`/`diff`/`certify` read only committed + vendored inputs — the hermetic harness already runs with no network/Docker/model, and this task makes the guarantee explicit (SC-004).

**Checkpoint**: `certify` blocks on clause gaps and is green on the real registry; the full path is provably offline and LLM-free.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T038 [P] Update the "Conformance Registry" section of `CLAUDE.md` and `AGENTS.md` to document the prose clause inventory (`conformance/spec/`, `conformance/inventory/clauses.json`, `clause` subcommands, V15, document-scope defaults) alongside 020's schema inventory.
- [X] T039 [P] Confirm `conformance/RULES.md`'s "Constraint inventory" section reflects the generalized V11–V14 + V15 and the clause/document-scope semantics (final consistency pass with `validate.rs`).
- [X] T040 Run `quickstart.md` end-to-end (`clause generate`/`check`/`validate`/`scaffold`/`diff`) and the full gate (`cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`), fixing any drift; confirm `registry_valid` and `certify` are green.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 (vendoring) blocks the loaders/baseline; T002/T003 [P] can start immediately.
- **Foundational (Phase 2)**: depends on Setup. T004→T005→T006 are sequential (`model.rs`/`load.rs`); T007/T008/T009 are [P] (separate files); T010 depends on T007–T009. **BLOCKS all user stories.**
- **US1 (Phase 3)**: depends on Foundational. MVP.
- **US2 (Phase 4)**: depends on US1 (needs `clauses.json` + `generate`).
- **US3 (Phase 5)**: depends on US1 (diffs clause inventories); independent of US2.
- **US4 (Phase 6)**: depends on US2 (classifications) and US1; wires the gate last (Decision 10).
- **Polish (Phase 7)**: depends on all desired stories.

### User Story Dependencies

- **US1 (P1)**: after Foundational. No dependency on other stories.
- **US2 (P2)**: after US1 (classifies the generated clauses).
- **US3 (P3)**: after US1 (operates on clause inventories); parallelizable with US2.
- **US4 (P4)**: after US2 (gate consumes classifications) + US1.

### Parallel Opportunities

- Setup: T002, T003 in parallel.
- Foundational: T007, T008, T009 in parallel (prose primitives).
- US1 tests T011, T012, T013 in parallel; then implementation.
- US2 classification authoring T027, T028 in parallel (different files); US3 can proceed in parallel with US2 once US1 is done.

---

## Parallel Example: User Story 1

```bash
# Write the three US1 test binaries together (they must fail first):
Task: "clause_extraction.rs in crates/conformance/tests/"
Task: "clause_determinism.rs in crates/conformance/tests/"
Task: "clause_baseline.rs in crates/conformance/tests/"
```

```bash
# Foundational prose primitives in parallel:
Task: "prose/mod.rs heading tree in crates/conformance/src/prose/"
Task: "prose/normalize.rs normalize_substance in crates/conformance/src/prose/"
Task: "prose/strength.rs detect_strength in crates/conformance/src/prose/"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Setup (Phase 1) + Foundational (Phase 2).
2. US1 (Phase 3) → committed `clauses.json` that regenerates byte-identically.
3. **STOP and VALIDATE**: `clause generate` + `clause check` + the three US1 test binaries green.

### Incremental Delivery (single CI-gated PR)

1. Foundation → US1 (inventory exists) → US2 (fully classified, V11–V15 enforced) → US3 (drift review) → US4 (certify gate wired last, offline/LLM-free proven).
2. Because the `certify` gate is wired only in US4 after 100% classification (research Decision 10), every intermediate commit keeps `validate`/`certify` green for bisectability, and the branch merges as one PR (SC-008) with no red gate on `main`.

---

## Notes

- [P] = different files, no dependency on an incomplete task. Tasks editing the same file
  (`model.rs` T004/T005; `clauses.json` T018/T019; `validate.rs` T022–T024;
  `bin/conformance.rs` T016/T017/T025/T032) are sequential.
- Tests are written first and must fail before the implementing task closes them.
- No `## Deferred Work`: research resolves every unknown; no deferrals are carried into this
  feature (a specification is not complete while deferrals remain — none exist here).
- Never delete a clause or weaken `certify` to go green; units are machine-canonicalized and
  ambiguity/unclassified/stale are blocking by design.
