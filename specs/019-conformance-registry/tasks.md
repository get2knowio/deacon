# Tasks: Repository-Owned Conformance Registry

**Input**: Design documents from `/specs/019-conformance-registry/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: Acceptance tests are MANDATORY (spec FR-029) — schema validation, traceability,
applicability, waiver expiry, gap handling, deterministic report generation. All tests are
hermetic (no network, no Docker) and must compile on the Windows `dev-fast` lane.

**Organization**: Grouped by user story. After EVERY code change:
`cargo fmt --all && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: New dev-only crate skeleton, wired into the workspace.

- [X] T001 Create `crates/conformance/Cargo.toml` (package `deacon-conformance`, `publish = false`, deps: `serde`, `serde_json`, `indexmap`, `thiserror`, `clap`, `anyhow`, new dev-only `jiff`) and add the member to the workspace in root `Cargo.toml`; create compiling stubs `crates/conformance/src/lib.rs` and `crates/conformance/src/bin/conformance.rs` (clap skeleton with `validate`/`report`/`certify` subcommands returning "unimplemented" errors, global `--registry <dir>` and `--today <YYYY-MM-DD>` flags per contracts/cli.md)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Record model, loader, and a minimal valid registry — every story consumes these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T002 Implement record model in `crates/conformance/src/model.rs`: all record types and closed enums from data-model.md (SourceRevision, SourceUnit, ContextDimension, ObservableChannel, CertificationProfile, BehaviorUnit + three disposition enums, TestCase + ExpectedOutcome, Gap, Waiver with preserved parity `scope`/`expect` shapes, DeaconExtension) with `#[serde(deny_unknown_fields)]`, plus the ID regex `^(rev|src|dim|chan|prof|bhv|case|gap|wvr|ext)-[a-z0-9]+(-[a-z0-9]+)*$` and prefix↔type agreement helper; unit tests for enum round-trips and ID parsing in the same file
- [X] T003 Implement registry loader in `crates/conformance/src/load.rs`: read the full `conformance/registry/` layout (collection files with `{schemaVersion, records}`, per-waiver files, `behaviors/*.json`, `sources/*.json`), producing a `Registry` aggregate; report SCHEMA-class errors with file path + location and collect ALL file errors in one pass (constitution IV precise messages, FR-019); `thiserror` domain errors
- [X] T004 [P] Seed the minimal authoritative registry skeleton: `conformance/registry/revisions.json` (`rev-spec-113500f4`, `rev-schema-113500f4`, `rev-oracle-0-87-0` with `verifiedAgainst: fixtures/parity-corpus/oracle.json`, `rev-cli-surface-0-87-0`), `dimensions.json` (`dim-os`, `dim-arch`, `dim-runtime`, `dim-oracle` + values), `channels.json` (six `chan-*` records), `profiles.json` (`prof-linux-amd64-docker-0870`, `active: true`) — all ID-sorted per contracts/registry-schema.md
- [X] T005 [P] Create the valid test fixture registry `fixtures/conformance/valid/` — a small self-consistent registry exercising every record type (≥2 behaviors incl. one out-of-profile, 1 case, 1 waiver, 1 gap, 1 extension, all four source inventories)

**Checkpoint**: `cargo run -p deacon-conformance -- validate --registry fixtures/conformance/valid` can load (validation logic lands in US1).

---

## Phase 3: User Story 1 — Maintainer validates registry integrity (Priority: P1) 🎯 MVP

**Goal**: `conformance validate` fails precisely on all ten violation classes + SCHEMA, reporting every violation in one run.

**Independent Test**: run `validate` against `fixtures/conformance/valid` (passes, zero violations) and against each `fixtures/conformance/invalid-v*/` fixture (fails naming that class and record) — no reports or seeding needed.

### Implementation for User Story 1

- [X] T006 [US1] Implement the validation engine scaffold in `crates/conformance/src/validate.rs`: `Violation { code, record, message }`, run-all-checks-collect-all-violations driver (FR-019), sorted output (code, then record ID); wire V1 (dangling refs — record IDs, dimension values, profile context) and V2 (duplicate IDs across the whole registry, ID format / prefix↔type mismatches)
- [X] T007 [US1] Add V3 (case with no behaviors), V4 (source unit with empty `behaviors` and no `outOfScope`), V9 (outcome referencing undeclared channel), V10 (case context with empty intersection against a linked behavior's applicability) to `crates/conformance/src/validate.rs`, including the applicability/context intersection evaluator shared with coverage
- [X] T008 [US1] Add V5 (in-active-profile behavior with no case, no waiver, AND no gap — clarification Q1 semantics) and V8 (contradiction rules R1–R8 from research.md Decision 5, incl. extension↔decision consistency) to `crates/conformance/src/validate.rs`
- [X] T009 [US1] Add V6 (waiver expiry: ISO-date lexicographic compare, expired iff `expires < today`, boundary `expires == today` passes; `today` injected as a parameter, defaulted from `jiff` UTC) and V7 (SourceRevision.pin vs `verifiedAgainst` repo file — parse `fixtures/parity-corpus/oracle.json` and compare) to `crates/conformance/src/validate.rs`
- [X] T010 [US1] Add V1 executable-reference checking: `TestCase.executable.binary` must exist as `crates/*/tests/<binary>.rs` (structural technique mirroring `parity-harness::registry::check_test_files`), separator-agnostic for Windows, in `crates/conformance/src/validate.rs`
- [X] T011 [US1] Implement the `validate` subcommand in `crates/conformance/src/bin/conformance.rs` per contracts/cli.md: text mode (violations to stdout one-per-line, nothing on success) and `--json` mode (single JSON doc on stdout, logs to stderr), exit codes 0/1/2
- [X] T012 [P] [US1] Create one invalid fixture registry per violation class under `fixtures/conformance/` — `invalid-v1/` … `invalid-v10/` (each exhibiting ONLY that violation, derived from the valid fixture) plus `schema-error/` (malformed record → SCHEMA class)
- [X] T013 [US1] Acceptance tests `crates/conformance/tests/validation_classes.rs`: valid fixture passes with zero violations; each `invalid-v*` fixture fails naming exactly its class and the offending record ID; `schema-error` reports SCHEMA with file+location; multi-violation fixture reports all violations in one run (SC-002, FR-019)
- [X] T014 [P] [US1] Acceptance tests `crates/conformance/tests/waiver_expiry.rs`: valid / expired / boundary (`expires == today`) via injected `--today`, exercising both the library API and the CLI (FR-029 waiver-expiry mandate)
- [X] T015 [US1] PR-gate test `crates/conformance/tests/registry_valid.rs`: validates the REAL `conformance/registry/` (skeleton from T004 must pass); confirm it is selected by `make test-nextest-fast` (hermetic, no nextest group overrides needed — verify with `cargo nextest list -E 'binary(=registry_valid)'`)

**Checkpoint**: US1 fully functional — the registry cannot silently rot.

---

## Phase 4: User Story 2 — Coverage reports & strict certification (Priority: P2)

**Goal**: Deterministic `report.json` + `report.md` with full source→behavior→context→case→outcome traceability; `certify` blocks on gaps/uncovered.

**Independent Test**: run `report` and `certify` against fixture registries with a known mix of conformant/divergent/waived/gap/out-of-profile behaviors; assert counts, trace chains, byte-identical repeat runs, and certification pass/fail on both sides of the boundary.

### Implementation for User Story 2

- [X] T016 [US2] Implement derived coverage evaluation in `crates/conformance/src/coverage.rs`: per-behavior coverage state (`conformant`/`divergent`/`waived`/`gap`) per data-model.md rules, active-profile filtering, out-of-profile bucket, deduplicated-behavior denominator (FR-003, FR-017, FR-023) with unit tests including the several-sources→one-behavior denominator case (SC-006)
- [X] T017 [US2] Implement `report.json` generation in `crates/conformance/src/report.rs` per contracts/report-schema.md: schemaVersion, profile, revisions, summary, behaviors with `sources`/`applicability`/`cases`/`outcomes` trace fields, outOfProfile, extensions, gaps, waivers — all ID-sorted, zero timestamps/absolute-paths/env data (SC-004)
- [X] T018 [US2] Implement `report.md` rendering in `crates/conformance/src/report.rs` with the seven required sections in contract order (summary table keeps `waived` distinct from `conformant`; Gaps section always present; extensions separate from divergences; behavior traceability index) — FR-020, FR-022, FR-023
- [X] T019 [US2] Implement strict certification in `crates/conformance/src/certify.rs` (fails iff any gap record exists OR any in-profile behavior uncovered; waivers listed, non-blocking) and wire the `report` (`--out-dir`, default `target/conformance/`) and `certify` subcommands with contract exit codes into `crates/conformance/src/bin/conformance.rs`
- [X] T020 [P] [US2] Acceptance tests `crates/conformance/tests/traceability.rs`: from `report.json`, walk source unit → behavior → applicability → case → outcome for the valid fixture; assert every chain link resolves and summary counts sum consistently with the behavior inventory (FR-022, SC-003 machine side)
- [X] T021 [P] [US2] Acceptance tests `crates/conformance/tests/applicability.rs`: out-of-profile behavior excluded from denominator and listed in `outOfProfile` (never "uncovered"); in-profile behaviors counted once each (FR-017, SC-007 — assert the report claims nothing beyond the profile's context)
- [X] T022 [P] [US2] Acceptance tests `crates/conformance/tests/gap_certification.rs`: registry with one gap → `certify` exit 1 listing the gap while `report` succeeds and shows it; same registry with the gap resolved (case added, dispositions updated, gap removed) → `certify` exit 0; waiver-covered behavior does not block but appears in `waived` (FR-020, FR-025, SC-005)
- [X] T023 [P] [US2] Acceptance tests `crates/conformance/tests/report_determinism.rs`: generate `report.json` twice into different out-dirs (and with different injected `--today`) → byte-identical output; `report.md` identical too (SC-004, FR-024)

**Checkpoint**: US1 + US2 work independently — validated data now drives release decisions.

---

## Phase 5: User Story 3 — Three-axis disposition recording (Priority: P3)

**Goal**: Contributors can record precise spec/reference/decision combinations; contradictions and single-state records are rejected; rules are documented in the registry.

**Independent Test**: fixture behaviors covering each meaningful axis combination — valid ones accepted, each contradiction rule rejected, missing-axis records rejected as SCHEMA, reports render the axes separately.

### Implementation for User Story 3

- [X] T024 [P] [US3] Write `conformance/RULES.md`: the full contradiction rule table R1–R8 with plain-language rationale (evidence-backed statuses; the R8→R4→R7 incremental-population chain), the gap-vs-waiver distinction, and the out-of-scope note for non-behavioral differentiators (FR-014 "documented in the registry itself"; research.md Decisions 5–6)
- [X] T025 [US3] Acceptance tests `crates/conformance/tests/disposition_rules.rs`: (a) accepted combos — conformant/divergent/follow-spec (US3 scenario 1), unspecified/aligned/align-with-reference (scenario 2), not-applicable/not-applicable/deacon-extension (scenario 3), nonconformant/divergent/intentional-divergence; (b) one failing fixture per rule R1–R8 asserting V8 with the rule named in the message; (c) a record missing any axis → SCHEMA (FR-012, US3 scenarios 4–5)
- [X] T026 [US3] Report-rendering assertions for dispositions: extend `crates/conformance/tests/traceability.rs` (or add to `disposition_rules.rs`) to assert `report.json`/`report.md` show the three axes as separate fields/columns, extensions listed under Extensions and never under Divergences (US3 scenario 3, FR-012)

**Checkpoint**: Defensible, auditable conformance claims — no "different but acceptable" state exists anywhere.

---

## Phase 6: User Story 4 — Seed from documented divergences & retire legacy lists (Priority: P4)

**Goal**: Registry authoritative from day one: every documented divergence migrated (research.md Decision 6 inventory), parity harness consumes the registry, prose reduced to pointers.

**Independent Test**: enumerate the legacy divergence inventory; assert each appears once in the seeded registry with full three-axis disposition; `registry_valid` passes; parity hermetic guards (`parity_harness_faults`, `parity_registry_check`) and `parity_corpus_errors` still pass.

### Implementation for User Story 4

- [X] T027 [US4] Seed source units and behaviors for the error corpus in `conformance/registry/sources/observed.json` and `conformance/registry/behaviors/read-configuration.json`: one `src-obs-*` + `bhv-*` per errors case (9: bad-config-path, duplicate-keys, extends-cycle, extends-missing, malformed-json, missing-config, unknown-field-preserved, wrong-type-features, wrong-type-forwardports) with dispositions per research.md Decision 6 (strictness `deacon-stricter` cases → reference `divergent` + decision `intentional-divergence`; the extends family → spec `unspecified` + reference `divergent` + decision `deacon-extension`, linked from `ext-extends-resolution`; both-accept/both-reject → `aligned`), and matching `case-*` records in `conformance/registry/cases.json` referencing binary `parity_corpus_errors` with corpus/case fields
- [X] T028 [P] [US4] Seed behaviors + cases for the tier1 corpus and live parity binaries: `case-*` records for the nine live binaries (from `fixtures/parity-corpus/registry.json`) and tier1 corpus cases, linked to normalized `bhv-*` records in `conformance/registry/behaviors/` (areas: up, exec, build, read-configuration, observable-state), with `src-obs-*`/`src-cli-*` provenance in `conformance/registry/sources/`
- [X] T029 [US4] Migrate waivers into the registry: convert `fixtures/parity-corpus/waivers/extends-child-merged.json` and all nine `fixtures/parity-corpus/errors/*/expect.json` into `conformance/registry/waivers/wvr-*.json` (preserved `scope`/`expect`/`rationale`/`added`, new `behaviors` links + `expires: 2027-01-19`), deduplicating the extends family into single behavior records referenced from all legacy provenance (FR-028); DELETE the legacy files. **NOTE: the legacy-file DELETION is deliberately deferred to the T031/T032 session.** `parity-harness` still reads `fixtures/parity-corpus/waivers/` and `errors/*/expect.json` from their legacy location; deleting them now would break the currently-passing parity suite. The registry `wvr-*` records are equivalent-content ADDITIONS; the legacy files are deleted once T031 repoints `parity-harness` at the registry.
- [X] T030 [P] [US4] Seed extensions and remaining divergences: `conformance/registry/extensions.json` (`ext-workspace-trust-gate`, `ext-secrets-file-env-format`, `ext-user-profiles`, `ext-host-ca-injection`, `ext-auto-forward-ports`, `ext-extends-resolution`) with linked `bhv-*` (decision `deacon-extension`, spec `unspecified`/`not-applicable`); behaviors for the DIFFERENTIATORS.md compose-project-name divergence; `gap-*` record for the CLAUDE.md compose marker-cleanup parity note (kind `coverage`); per research.md Decision 6 items 3–5
- [X] T031 [US4] Repoint `parity-harness` at the registry: add `deacon-conformance` dependency to `crates/parity-harness/Cargo.toml`; rewrite `crates/parity-harness/src/waiver.rs` internals to load waiver records via the conformance loader from `conformance/registry/waivers/` while preserving the `WaiverSet` public query API (`load`, `records`, `get`, `corpus_case`, `corpus_cases`, `state_field_waivers`, `stale_among`) and the stale-waiver semantics; update `WaiverSet::load` callers if the signature must change
- [X] T032 [US4] Update the hermetic parity guards for the new layout: `parity_registry_check` (and `crates/parity-harness/src/registry.rs` if it references waiver paths) must enforce the NEW waiver location and the absence of legacy `expect.json`/`waivers/` files; run `cargo nextest run -E 'binary(=parity_registry_check) or binary(=parity_harness_faults) or binary(=parity_corpus_errors)'` and fix fallout
- [X] T033 [US4] Seeding-completeness acceptance test `crates/conformance/tests/seed_completeness.rs`: hard-coded enumeration of the legacy divergence inventory (1 tier1 waiver + 9 errors cases + extensions list) asserting each maps to exactly one registry record with all three axes present (SC-001, FR-026, FR-028), and that `fixtures/parity-corpus/waivers/` and `errors/*/expect.json` no longer exist
- [X] T034 [US4] Retire prose duplicates (FR-027): update `docs/DIFFERENTIATORS.md`, `fixtures/parity-corpus/errors/README.md`, `fixtures/parity-corpus/README.md`, and the CLAUDE.md "Verified Non-Bugs" read-configuration-strictness entry to POINT at registry record IDs instead of restating divergence details; verify `registry_valid` passes on the fully seeded registry

**Checkpoint**: Single source of truth achieved; parity suite green against the migrated data.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T035 Wire strict certification into the release gate: add a blocking `cargo run -p deacon-conformance -- certify` step to the verify job in `.github/workflows/release.yml`, uploading `target/conformance/report.json` + `report.md` as release artifacts (clarification Q5, FR-025)
- [X] T036 [P] Documentation: add a "Conformance Registry" section to `CLAUDE.md` (data layout, `validate`/`report`/`certify` commands, record-a-divergence recipe pointer to `conformance/RULES.md` and quickstart.md; note the registry is now the authoritative pin/waiver location); fix the stale `docs/subcommand-specs/` reference noted in research.md Decision 6
- [X] T037 Run the quickstart.md flows end-to-end (`validate`, `report`, `certify`, fixture + `--today` knobs) and fix any drift between quickstart and actual CLI behavior; verify SC-008 (validate+report well under 30 s)
- [X] T038 Full gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings` (whole workspace — deacon-core included), `make test-nextest` (all profiles green, no nextest group regressions), and confirm the conformance crate compiles with no Unix-only APIs (Windows `dev-fast` lane)

---

## Deferred Work

**All 38 feature tasks (T001–T038) are complete; no feature task is deferred.**

One caveat is recorded here for transparency — it is registry *data* the feature is
designed to hold, **not** an unfinished task:

- **Open registry gap `gap-compose-marker-cleanup` (seeded in T030, research.md Decision 6
  item 4).** The active profile is therefore **not certified** — `certify` exits 1 and, as
  wired in T035, the release workflow's `verify` job will block until this gap is resolved
  or explicitly waived. This is the feature working **as designed** (FR-025 / clarification
  Q5: strict release-gating on genuine, tracked gaps), not a defect in the implementation.
  - **Why not resolved in this feature**: The gap is the CLAUDE.md "Verified Non-Bugs"
    compose marker-cleanup parity note (issue #117) — the compose `up` path does not call
    `clear_markers()` on `--remove-existing-container`, unlike the single-container path.
    Confirmed empirically that `commands/up/compose.rs` never consults lifecycle phase
    markers (only `ComposeState`/`StateManager`), so the difference is non-surfacing,
    parity-only cleanup with **no observable behavior to test**. Adding the call would be a
    behavioral no-op with no evidence for a conformance case, so the gap cannot be
    legitimately closed under the registry's evidence-backed-status rule; and modifying
    compose lifecycle behavior is outside 019-conformance-registry's scope.
  - **Acceptance to close (maintainer decision, not this feature)**: either (a) add the
    `clear_markers()` call to the compose `--remove-existing-container` path AND an
    executable case proving the behavior, then update the linked behavior's dispositions
    and delete `gaps.json`'s `gap-compose-marker-cleanup`; or (b) characterize it as an
    intentional divergence with a waiver (rationale + `expires`). Either path makes
    `certify` exit 0 and unblocks the release gate.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 → Phase 2**: crate must exist before model/loader.
- **Phase 2 blocks all stories** (model + loader + skeleton data are universal inputs).
- **US1 (Phase 3)**: only needs Phase 2. **US2 (Phase 4)**: needs US1's validation engine (report/certify run validate first). **US3 (Phase 5)**: needs US1's V8 implementation (T008); T024 (RULES.md) only needs research.md. **US4 (Phase 6)**: needs US1 (registry must validate) and benefits from US2/US3 tests; T031–T032 (harness repoint) need T029 (migrated waivers).
- **Phase 7**: T035 needs US2; T036–T038 need everything.

### Within-story ordering

- US1: T006 → T007/T008/T009/T010 (same file, sequential) → T011; T012 [P] alongside engine work; T013/T014 after T011+T012; T015 after T011+T004.
- US2: T016 → T017 → T018 → T019 → T020–T023 [P].
- US4: T027/T028/T030 [P with each other] → T029 → T031 → T032 → T033/T034.

### Parallel opportunities

- Phase 2: T004 ∥ T005 (different directories, both after T002/T003 only for schema shape — can draft concurrently with T003).
- US1: fixture authoring T012 ∥ engine tasks T006–T010; T014 ∥ T013.
- US2: all four test tasks T020–T023 in parallel after T019.
- US4: seed tasks T027 ∥ T028 ∥ T030 (different registry files).
- Cross-story: after Phase 3, US2 (T016+) and US3 (T024, T025) can proceed in parallel.

## Parallel Example: User Story 2

```bash
# After T019 lands, launch the four acceptance-test tasks together:
Task: "traceability.rs — walk source→behavior→context→case→outcome from report.json"
Task: "applicability.rs — out-of-profile exclusion and denominator checks"
Task: "gap_certification.rs — certify blocks on gaps, passes when resolved"
Task: "report_determinism.rs — repeat-run byte equality"
```

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + US1** (T001–T015): a validated registry that cannot rot,
gating every PR via `registry_valid`. Stop, validate independently, then deliver
incrementally: US2 (reports + release gate value), US3 (disposition rigor), US4 (seeding
+ legacy retirement — the single-source-of-truth payoff), Phase 7 polish. Each checkpoint
leaves `main`-mergeable state: earlier stories never depend on later ones.

**Totals**: 38 tasks — Setup 1, Foundational 4, US1 10, US2 8, US3 3, US4 8, Polish 4.
