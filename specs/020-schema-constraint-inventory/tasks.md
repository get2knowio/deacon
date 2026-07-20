# Tasks: Schema Constraint Inventory

**Input**: Design documents from `/specs/020-schema-constraint-inventory/`
**Prerequisites**: plan.md, spec.md, research.md (Decisions 1–10), data-model.md, contracts/, quickstart.md

**Tests**: INCLUDED — spec FR-023/FR-024 mandate the acceptance-test matrix (constitution VII: spec-mandated tests are acceptance criteria).

**Organization**: Grouped by user story. US1 (extraction) is the MVP; US2 (classification) makes it enforceable; US3 (drift/diff) and US4 (external sources) extend it. Certification wiring deliberately lands in the final phase (research Decision 10) so `certify` is never red mid-branch.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US4 from spec.md

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Vendored pinned inputs + crate plumbing every story needs

- [ ] T001 Vendor the two pinned schemas byte-exact: create `conformance/schemas/113500f4/devContainer.base.schema.json` and `conformance/schemas/113500f4/devContainerFeature.schema.json` from `https://raw.githubusercontent.com/devcontainers/spec/113500f4/schemas/…` (one-time network step, quickstart.md); verify SHA-256 = `a0883c04…5408dd` / `671fcd80…5af32eb` (research.md table)
- [ ] T002 [P] Create `conformance/schemas/113500f4/manifest.json` per data-model.md §1 (revision `rev-schema-113500f4`, document keys `base`/`feature`, upstream URLs, the two SHA-256 values)
- [ ] T003 [P] Add `sha2.workspace = true` to `crates/conformance/Cargo.toml`
- [ ] T004 [P] Create fixture directory skeleton `fixtures/conformance/schemas/` with a `README.md` stating these are extractor fixtures (composition/cycle/malformed cases), never vendored upstream artifacts

**Checkpoint**: pinned inputs committed and fingerprinted; crate builds.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: model + loader surface all stories build on. No user story starts until this phase completes.

- [ ] T005 Extend `crates/conformance/src/model.rs`: add `RecordType::Constraint` (`cst`) and `RecordType::Classification` (`cls`) to the enum, `prefix()`, `from_prefix()`; extend the module-doc ID regex comment; unit tests for `parse_id` on both prefixes
- [ ] T006 Add new model structs in `crates/conformance/src/model.rs` per data-model.md §1–§3: `SchemasManifest`, `ManifestDocument`, `ConstraintInventory`, `ConstraintUnit`, `ConstraintKind` (15-variant closed kebab-case enum), `UnitContext`, `Classification`, `Disposition` — all `deny_unknown_fields` + camelCase, with serde round-trip unit tests
- [ ] T007 [P] Add error variants per data-model.md §5 to the crate's error type (thiserror, cause-specific messages): `MalformedSchema`, `MalformedRef`, `UnresolvedRef`, `UnresolvedExternalRef`, `RefCycle { chain }`, `ManifestFingerprintMismatch`, `IdCollision`, `InventoryOutOfDate`
- [ ] T008 Extend `crates/conformance/src/load.rs`: load `conformance/schemas/<pin>/manifest.json` (+ SHA-256 verification of vendored files via `sha2`), `conformance/inventory/constraints.json`, and `conformance/registry/classifications/*.json` collections (envelope + ID-sort + prefix↔type agreement reusing existing V2 machinery); loader unit tests with minimal fixtures
- [ ] T009 [P] Add path helpers in `crates/conformance/src/lib.rs` (`default_schemas_dir()`, `default_inventory_file()`) mirroring `default_registry_dir()`, plus module declarations for the new modules as they land

**Checkpoint**: all new shapes load/reject correctly; nothing user-visible yet.

---

## Phase 3: User Story 1 — Generate the complete constraint inventory (P1) 🎯 MVP

**Goal**: deterministic, complete, provenance-carrying extraction of the two pinned schemas into a committed `conformance/inventory/constraints.json`, fail-loud on every malformed/cyclic/unresolved input.

**Independent Test** (spec US1): fixture schemas extract to exactly the expected units; two consecutive runs are byte-identical; error fixtures fail with cause-naming errors; no network anywhere.

- [ ] T010 [US1] Create `crates/conformance/src/schema/mod.rs`: schema document model (parsed `serde_json::Value` + document key), RFC 6901 JSON Pointer building/escaping helpers (`~0`/`~1`), and pointer-lookup; unit tests including escaping edge cases
- [ ] T011 [US1] Create `crates/conformance/src/schema/resolve.rs` per research Decision 5: fragment-ref resolution within a pinned document, relative-path refs resolving ONLY to other manifest entries, typed failures (`UnresolvedRef`/`UnresolvedExternalRef`/`MalformedRef`), pure-`$ref`-chain cycle detection with full-chain `RefCycle` reporting; unit tests for each failure class
- [ ] T012 [US1] Create `crates/conformance/src/schema/extract.rs` per research Decisions 3–4: single-visit definition-site walk emitting facet units for all 15 `ConstraintKind`s (property-existence, required, type+nullable, enum, const, default, union-alternative with arm index, all-of, conditional with condition context, additional-properties tri-state incl. `patternProperties`/`unevaluatedProperties`, array-shape, value-shape, reference edges, annotation, unmodeled-keyword catch-all); deterministic emission order
- [ ] T013 [US1] Create `crates/conformance/src/inventory.rs` per research Decisions 6–7: stable ID derivation (`cst-<doc>-<slug>-<kind>-<hash8>`, hash8 = first 8 chars of lowercase-hex sha256 over document‖pointer‖kind‖canonical-substance, slug deterministically truncated at 48 chars, collision → `IdCollision`), unit sort by ID, canonical serialization (sorted keys, 2-space indent, LF, trailing newline, no timestamps/abs paths), atomic temp-file+rename write
- [ ] T014 [US1] Wire `inventory generate` and `inventory check` subcommands into `crates/conformance/src/bin/conformance.rs` per contracts/cli-inventory.md (`--schemas`, `--out`, `--inventory` overrides; exit codes; `check` prints compact added/removed/changed ID summary on mismatch)
- [ ] T015 [P] [US1] Author fixture schemas in `fixtures/conformance/schemas/`: `composition.json` (nested allOf/anyOf/oneOf + union alternatives + null-in-type-union + required + additionalProperties tri-state cases), `recursive-ok.json` (productive self-reference), `cycle.json` (pure $ref loop), `unresolved-ref.json`, `external-ref.json` (live-URL ref), `malformed.json` (invalid JSON), `empty.json` (trivial valid schema), each with a sibling manifest fixture
- [ ] T016 [P] [US1] Write `crates/conformance/tests/inventory_extraction.rs` against the T015 fixtures (FR-023): nested composition resolved, each union arm its own unit with arm index, nullable flag set, required captured, additionalProperties tri-state distinguished, conditional context preserved, unmodeled keyword captured verbatim, empty schema OK; error fixtures produce the exact typed error (cycle message lists the chain); fingerprint mismatch fails (tamper a fixture copy in a temp dir)
- [ ] T017 [P] [US1] Write `crates/conformance/tests/inventory_determinism.rs` (FR-023, SC-002): regenerate from vendored pinned schemas into a temp dir, assert byte-equality with `conformance/inventory/constraints.json`; regenerate twice in-memory, assert identical; assert stable IDs unchanged across runs
- [ ] T018 [US1] Run `cargo run -p deacon-conformance -- inventory generate` and COMMIT the real `conformance/inventory/constraints.json`; perform and document (in the PR description) the SC-001 spot-audit: sample ≥25 constraints across both documents from the vendored schemas and verify each appears exactly once with correct kind, substance, and provenance pointer — zero missing or mis-attributed
- [ ] T019 [P] [US1] Write `crates/conformance/tests/inventory_baseline.rs` per contracts/inventory-schema.md "Baseline assertions" (FR-024): forwardPorts array-type unit, features object/additional-properties units, base top-level `oneOf` container-variant `union-alternative` units, one known `nullable: true` union, exact per-document unit counts
- [ ] T020 [US1] Register the three new test binaries in `.config/nextest.toml` following existing conformance-test entries (hermetic, no docker groups; ensure they run in default/dev-fast/full/ci profiles and are NOT matched by the parity profile's allow-list)

**Checkpoint**: MVP delivered — complete inventory committed, regeneration byte-identical, all error paths fail loud. US2–US4 can start.

---

## Phase 4: User Story 2 — Classify every constraint under the consumer-only scope (P2)

**Goal**: every unit carries exactly one hand-authored disposition; validation joins inventory ↔ classifications ↔ behaviors (V11–V14); the two hand-written schema source units are superseded.

**Independent Test** (spec US2): validate reports precisely the unclassified/stale/malformed-mapping records; not-applicable units remain visible in report output.

- [ ] T021 [US2] Implement `inventory scaffold` in `crates/conformance/src/bin/conformance.rs` per contracts/cli-inventory.md: emit skeleton `cls-` records (sentinel `"UNREVIEWED"` disposition the loader rejects) for every unclassified unit, to stdout only
- [ ] T022 [US2] Extend `crates/conformance/src/validate.rs` with V11–V14 per contracts/classification-schema.md: V11 stale classification, V12 unclassified/duplicated unit, V13 shape/linkage (id-tail mirror, behaviors arity + existence, rationale arity), V14 provenance (manifest fingerprint, inventory revision ≠ registry schema pin, committed ≠ regenerated); all reported in one run alongside V1–V10
- [ ] T023 [P] [US2] Write `crates/conformance/tests/classification_join.rs`: fixture registry + inventory pairs proving each of V11/V12(zero)/V12(duplicate)/V13(each arity rule)/V14(each provenance rule) fires with the offending ID named, and a fully-classified fixture passes clean; sentinel `UNREVIEWED` rejected at load as SCHEMA
- [ ] T024 [US2] Classify ALL `annotation`-kind and `unmodeled-keyword`-kind units: author `non-testable` (+ rationale) records via scaffold assist into `conformance/registry/classifications/base.json` and `conformance/registry/classifications/feature.json` (JSONC directives `allowComments`/`allowTrailingCommas` and `$schema` are `non-testable`; any unmodeled keyword gets a conscious per-keyword decision)
- [ ] T025 [US2] Classify all remaining `base` document units in `conformance/registry/classifications/base.json` applying research Decision 11's evidence rule in order: (1) map to existing covered `bhv-` records; (2) where none exists but an existing deacon test evidences the constraint, create the behavior + a `case-` record naming that test binary (`executable.binary`), then map; (3) where no evidence exists, write the bounded hermetic test in-feature or accept the honest blocking gap — NEVER an uncovered decorative behavior; editor-only surface → `not-applicable` + rationale (100% coverage of base units — SC-003)
- [ ] T026 [US2] Classify all `feature` document units in `conformance/registry/classifications/feature.json`: feature-AUTHORING constraints → `not-applicable` under consumer-only scope (constitution II) with rationale; the consumer-side install surface (option value validation etc.) → `behavior-mapped` per research Decision 11's evidence rule (same (1)/(2)/(3) order as T025) (100% coverage of feature units)
- [ ] T027 [US2] Migration per contracts/classification-schema.md (FR-022): remove `src-schema-features-type` and `src-schema-forwardports-type` from `conformance/registry/sources/schema.json` (keep empty collection), confirm the replacement `cls-` records map the same behaviors (`bhv-readconfig-wrong-type-{features,forwardports}-rejected`); `cargo run -p deacon-conformance -- validate` passes with zero violations
- [ ] T028 [US2] Extend `crates/conformance/src/report.rs`: inventory section in `report.json` + `report.md` (unit counts by document/kind, disposition tallies, unclassified + stale listings — normally empty); keep byte-stable determinism; update `crates/conformance/tests/report_determinism.rs` expectations
- [ ] T029 [US2] Register `classification_join` in `.config/nextest.toml` (same pattern as T020)

**Checkpoint**: 100% of units classified; `validate` enforces the join; registry has a single schema-evidence bookkeeping system.

---

## Phase 5: User Story 3 — Detect and review upstream schema drift (P3)

**Goal**: deterministic added/removed/materially-changed diff between two inventory files; drift review workflow proven end-to-end on fixtures.

**Independent Test** (spec US3): two fixture revisions differing by one added + one removed + one changed constraint produce exactly those three, correctly kinded; description-only change is non-material.

- [ ] T030 [US3] Create `crates/conformance/src/diff.rs` per research Decision 9 + data-model.md §4: match on `(document, pointer, kind)`, substance decides `changed`, annotation-kind differences segregated to `nonMaterial`, deterministic sort, JSON + Markdown renderers
- [ ] T031 [US3] Wire `inventory diff <old> <new> [--format json|md] [--out]` into `crates/conformance/src/bin/conformance.rs` per contracts/cli-inventory.md
- [ ] T032 [P] [US3] Author drift fixtures in `fixtures/conformance/inventory-drift/`: `old/` and `new/` schema+manifest pairs where `new` adds one constraint, removes one, materially changes one (type widened), moves one unchanged (→ removed+added per spec Assumption), and reworders/rewords one description (→ nonMaterial only)
- [ ] T033 [P] [US3] Write `crates/conformance/tests/inventory_diff.rs` (FR-023 drift): generate inventories from both fixture revisions, diff, assert exact added/removed/changed/nonMaterial sets and the changed entry carries oldId≠newId with old/new substance; assert diff output itself is byte-deterministic across two runs; end-to-end drift workflow test: classifications authored against `old` go V11-stale / new units go V12-unclassified against `new` (SC-007: no inheritance)
- [ ] T034 [US3] Register `inventory_diff` in `.config/nextest.toml` (same pattern as T020)

**Checkpoint**: pin-bump workflow (quickstart.md "Re-vendoring") fully rehearsed on fixtures.

---

## Phase 6: User Story 4 — Add an explicitly pinned external schema source (P4)

**Goal**: additional explicitly-pinned schemas join the inventory as separate sources; unpinned external refs stay hard errors.

**Independent Test** (spec US4): a pinned external fixture schema's constraints appear under its own document key; an unpinned URL ref errors without any fetch.

- [ ] T035 [P] [US4] Add fixture: three-document manifest under `fixtures/conformance/schemas/multi-source/` where doc `extra` relative-refs doc `base-fixture` (resolves per research Decision 5) and a variant manifest omitting the target (fails `UnresolvedExternalRef`)
- [ ] T036 [US4] Extend `crates/conformance/tests/inventory_extraction.rs`: pinned-external-source cases from T035 — cross-document reference units carry the target document key; separate-source attribution in unit `document` fields; unpinned variant errors naming the offending ref; confirm report (T028) lists sources separately

**Checkpoint**: P4 machinery proven on fixtures; vendoring real editor schemas stays a conscious future decision (research Decision 2).

---

## Phase 7: Polish, Certification Wiring & Docs (final — research Decision 10)

**Purpose**: flip on the release-gate semantics only now that classification is 100% complete; documentation lockstep; full gates.

- [ ] T037 Extend `crates/conformance/src/certify.rs` per contracts/cli-inventory.md: exit 1 iff gap OR uncovered in-profile behavior OR any V11/V12/V13/V14; `not-applicable`/`non-testable` never block; update `crates/conformance/tests/gap_certification.rs` with fixture cases for each new blocking condition AND a real-registry run asserting exit 0 (SC-008)
- [ ] T038 [P] Update `conformance/RULES.md` per contracts/classification-schema.md: "Constraint inventory" section (V11–V14 table, disposition arity rules, drift workflow, machine-owned vs hand-authored file boundary) — validate.rs/RULES.md lockstep rule
- [ ] T039 [P] Update `CLAUDE.md`: extend the Conformance Registry section with the inventory (paths, `inventory generate|check|diff|scaffold` commands, V11–V14 one-liners, the "committed inventory is machine-owned / classifications are hand-authored" boundary, re-vendoring workflow pointer to quickstart)
- [ ] T040 Verify quickstart.md commands all work verbatim (run each; fix doc or code on any mismatch) and re-check `.config/nextest.toml`: all five new binaries in default/dev-fast/full/ci, excluded from the parity profile allow-list, `make test-nextest-fast` includes them
- [ ] T041 Full gates: `cargo fmt --all && cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `make test-nextest`, `cargo run -p deacon-conformance -- validate` (0 violations) and `certify` (exit 0), `inventory check` (identical); SC-004 verification: run `inventory generate` + `check` + the five new test binaries once in a network-isolated environment (`unshare -rn` if available, else document the isolation used) and record zero network access in the PR description; confirm Windows-safe (no `#[cfg(unix)]` needs, LF-stable output)

---

## Dependencies & Execution Order

- **Phase 1 → Phase 2 → Phase 3**: strictly sequential (pinned inputs → shapes → extraction).
- **Phase 3 (US1) blocks everything downstream** — it produces the committed inventory.
- **Phase 4 (US2)** depends on US1 only. **Phase 5 (US3)** depends on US1; independent of US2 except T033's stale-workflow case (needs T022's V11/V12). **Phase 6 (US4)** depends on US1; T036's report check needs T028.
- **Phase 7** depends on ALL stories (certify may only be wired once classification is complete — Decision 10, SC-008).
- Within phases, [P] tasks touch disjoint files and can run concurrently once their listed predecessors exist.

```text
Setup (T001–T004) → Foundational (T005–T009) → US1 (T010–T020) ─┬→ US2 (T021–T029) ─┬→ Polish (T037–T041)
                                                                ├→ US3 (T030–T034) ─┤
                                                                └→ US4 (T035–T036) ─┘
```

## Parallel Opportunities

- Phase 1: T002/T003/T004 after T001.
- Phase 3: T015/T016/T017/T019 are parallel test/fixture work alongside T014; T010–T013 are a mostly-serial core chain (mod → resolve → extract → inventory).
- Phases 4/5/6 can proceed concurrently after US1 (different files), converging at Phase 7.
- Classification authoring T024/T025/T026 can be parallelized per document file once T021/T022 exist.

## Implementation Strategy

**MVP = Phase 1–3 (US1)**: a complete, committed, byte-reproducible inventory with fail-loud extraction — already delivers the "true size of the pinned surface" value on its own. Then US2 makes it enforceable (the certification story), US3 makes pin bumps safe, US4 proves the extension seam. All phases merge as ONE CI-gated PR with certify green (Decision 10); intermediate commits stay individually green for bisectability.

**Estimated totals**: 41 tasks — Setup 4, Foundational 5, US1 11, US2 9, US3 5, US4 2, Polish 5.
