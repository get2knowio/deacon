# Implementation Plan: Deterministic Conformance Coverage

**Branch**: `024-deterministic-conformance-coverage` | **Date**: 2026-07-26 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/024-deterministic-conformance-coverage/spec.md`

## Summary

The migrated conformance record is truthful about what it claims and silent about what it
omits: 82 of 82 declarative operations are `read-configuration`/`up`/`exec`, zero cases
declare a context, one observable channel has never been observed, and the behavior
denominator is 27 records that `certify` measures against — so an unrecorded behavior is
invisible rather than uncovered.

This feature adds a **constrained context model** (six scenario dimensions plus the existing
environment dimensions), **applicability rules** that shrink the space to valid combinations,
a **generated obligation set** (per-operation pairwise plus hand-selected high-risk triples),
and a **mandatory disposition** on every applicable obligation — case, non-testable rationale,
scoped expiring waiver, or visible gap — with gaps and expired waivers blocking `certify`. It
then fills the space with deterministic cases across the whole consumer workflow, adds a
Docker-backed error-path tier so parity stops relying on the reference's lenient
configuration read, de-suppresses the fields broad normalization used to hide, and proves each
observable channel can fail via injected regressions.

The technical shape follows one finding from Phase 0: scenario dimensions **cannot** join the
existing `dimensions.json`, because `applies_in_profile` treats a condition on an unassigned
dimension as unsatisfied, which would silently shrink the very denominator this feature exists
to expose (research Decision 1). Scenario context therefore gets its own namespace and its own
evaluator, and the obligation set reuses the machine-owned/hand-authored split that 020 and 021
have already validated twice.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json` (strict-JSON
records), `indexmap` (declaration order), `sha2` (obligation/case/fixture hashing), `tokio`
(bounded async execution, per-case timeouts, `JoinSet` concurrency), `thiserror`
(`HarnessError`/domain errors), `tracing`, `toml` (nextest-profile drift check), `tempfile`
(isolated workspaces, dev-dep). **No new crates** (research Decision 2, mirroring 023 D6).
**Storage**: strict-JSON, version-controlled. New hand-authored:
`conformance/registry/scenario.json`, `conformance/registry/applicability.json`,
`conformance/registry/obligation-dispositions/<area>.json`,
`conformance/registry/regressions.json`. New machine-owned:
`conformance/obligations/obligations.json`. Migrated: `conformance/registry/cases.json` →
`conformance/registry/cases/<area>.json` (research Decision 7). Generated, git-ignored:
`target/conformance/coverage-{pairwise,triples,operations,observables}.{json,md}`,
`target/conformance/regressions.json`. All writes atomic (temp file + `fs::rename`).
**Testing**: `cargo-nextest` exclusively. Hermetic work runs in the default/CI profiles via
the existing `registry_valid` gate plus new hermetic guards; live work runs **only** under
`--profile parity` across two driver binaries (research Decision 4).
**Target Platform**: Linux x86_64 for the live lane; hermetic crates build and test on
Linux/macOS/Windows (the `dev-fast` Windows lane compiles every test binary, so new test
files must compile there even when excluded from selection).
**Project Type**: Dev-only tooling inside an existing Rust workspace — `deacon-conformance`
(hermetic data, validation, obligation generation, reporting) and `parity-harness` (live
execution, observation, regression injection). Neither is part of the shipped `deacon`
consumer CLI (Principle II; enforced by `parity_registry_check`).
**Performance Goals**: Docker-backed tier completes within **30 minutes** per run on the
certification lane, asserted from measured wall clock rather than delegated to a timeout
(research Decision 10); per-case timeout **5 minutes**, fail-loud.
**Constraints**: Byte-stable deterministic outputs (no timestamps, absolute paths, or
run-dependent ordering); no network in any hermetic command; no `#[ignore]`, no env-var opt-in,
no silent skip; report generation is read-only with respect to the record.
**Scale/Scope**: Grows the record from 27 behaviors / 88 cases toward coverage of 10
operations × 11 channels. Expected order of magnitude: a few hundred combination obligations
after applicability pruning, ~12–20 high-risk triples, and a case set large enough that the
30-minute budget is a real constraint on the Docker tier rather than a formality.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment | Status |
|---|---|---|
| **I. Spec-Parity as Source of Truth** | The feature adds no product behavior. It measures conformance against the pinned upstream spec (`113500f4`) and the pinned oracle (0.87.0), both unchanged. New behaviors originate from named source units, never invention (research Decision 9). Phased-implementation rules apply: deferrals go to `research.md` numbered decisions **and** a `tasks.md` "Deferred Work" section. | ✅ PASS |
| **II. Consumer-Only Scope** | Every command added is dev-only (`deacon-conformance`, `parity-harness` bins). `deacon --help` gains nothing; `parity_registry_check` asserts this on every PR. Obligations that would require feature-authoring commands are permanently excluded with a stated ground (spec Assumption 7). | ✅ PASS |
| **III. Keep the Build Green** | Hermetic additions run in `make test-nextest-fast`; the full gate runs before PR. **Fix, Don't Skip** is load-bearing here: FR-075 forbids `#[ignore]` and env-var opt-in, and FR-044 requires a missing prerequisite to fail loudly rather than skip. | ✅ PASS |
| **IV. No Silent Fallbacks — Fail Fast** | The central design rule. Unavailable oracle/Docker fails with a cause-specific error (FR-044); an undispositioned obligation fails validation (FR-020); a stale tolerance is reported rather than retained (FR-024); a dead observer is reported inert rather than passing (FR-065b, research Decision 5). | ✅ PASS |
| **V. Idiomatic, Safe Rust** | No `unsafe`. `thiserror` in both crates, `anyhow` only at bin boundaries. Async discipline matters: `docker inspect` is blocking and MUST go through `spawn_blocking` in the concurrent driver — the exact defect 023 fixed (D-2/D-3). Modular boundaries: obligation generation, coverage reporting, and regression injection are separate modules, not additions to `validate.rs` (already 3,344 lines). | ✅ PASS |
| **VI. Observability & Output Contracts** | New reports are byte-stable and ordered (`IndexMap`/sorted keys). `report.json`'s existing versioned contract is left intact; new artifacts are siblings (research Decision 6). | ✅ PASS |
| **VII. Testing Completeness** | FR-071 makes every acceptance scenario an automated test. Nextest configuration is mandatory: the new Docker driver binary needs overrides in **all** profiles plus a `registry.json` entry, or `parity_registry_check` fails. Determinism/hermeticity: hermetic commands never touch the network. | ✅ PASS |
| **VIII. Subcommand Consistency & Shared Abstractions** | Reuses `normalize.rs` (the single normalization module), `waiver.rs`'s self-invalidating pattern for stale tolerances, `snapshot.rs`'s provenance/staleness model, and the 020/021 machine-owned ↔ hand-authored split rather than forking any of them. The obligation loader reuses `Condition` verbatim. | ✅ PASS |
| **IX. Executable & Self-Verifying Examples** | No `examples/` change: this feature adds no user-facing surface. `quickstart.md` carries the executable walkthrough instead. | ✅ PASS (N/A) |

**Gate result: PASS — no violations, Complexity Tracking left empty.**

Two risks are called out rather than waived, because both are places where a green result
could be wrong:

1. **The `cases.json` → `cases/<area>.json` migration can silently lose records.** It must
   land as its own commit with a before/after count assertion (research Decision 7).
2. **Growing the behavior denominator is itself a way to dilute coverage.** SC-014 and V28
   make a new behavior arrive with dispositions or fail; the guard must land *before* the
   behaviors it guards.

## Project Structure

### Documentation (this feature)

```text
specs/024-deterministic-conformance-coverage/
├── plan.md                       # This file
├── research.md                   # Phase 0: 10 decisions + measured baseline
├── data-model.md                 # Phase 1: entities, invariants, violation classes
├── quickstart.md                 # Phase 1: authoring + drift workflows
├── contracts/
│   ├── scenario-model.md         # sdim-/rule- record shapes, applicability semantics
│   ├── obligation.md             # obl- generation, identity, disposition resolution
│   ├── coverage-cli.md           # `coverage generate|check|report` contract
│   ├── coverage-report.md        # the four report schemas
│   └── regression-harness.md     # injection point, inert detection, reporting
├── checklists/
│   └── requirements.md           # Spec quality checklist (complete)
└── tasks.md                      # Phase 2 output — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
crates/conformance/src/            # deacon-conformance — hermetic data + validation
├── scenario.rs                    # NEW: sdim-/rule- model, applicability evaluator
├── obligation.rs                  # NEW: obligation identity, generation, disposition join
├── coverage_report.rs             # NEW: the four report renderers (byte-stable)
├── regression.rs                  # NEW: regression record model + V30 checks
├── model.rs                       # EXTEND: scenario context on TestCase; re-export ids
├── load.rs                        # EXTEND: cases/<area>.json dir loading; new files
├── validate.rs                    # EXTEND: V26–V30 (new fns, not inline growth)
├── certify.rs                     # EXTEND: BlockingKind::Obligation; new report buckets
├── coverage.rs                    # EXTEND: obligation-aware coverage evaluation
└── bin/conformance.rs             # EXTEND: `coverage` command group

crates/parity-harness/src/         # live execution + observation
├── inject.rs                      # NEW: evidence-source-boundary regression injection
├── runner.rs                      # EXTEND: per-case timeout, bounded concurrency
├── observe/mod.rs                 # EXTEND: derived fields for de-normalized US5 comparisons
├── normalize.rs                   # EXTEND: named scoped rules only (V24 still applies)
└── bin/coverage-regressions.rs    # NEW: the injected-regression acceptance run

crates/deacon/tests/
├── parity_conformance_runner.rs   # RESHAPE: config-only driver, per-resource-group fns
├── parity_conformance_docker.rs   # NEW: Docker-backed driver incl. error-path tier
└── parity_registry_check.rs       # EXTEND: new binary registered in all profiles

conformance/
├── registry/
│   ├── scenario.json              # NEW (hand-authored): sdim- dimensions + values
│   ├── applicability.json         # NEW (hand-authored): rule- exclusions with grounds
│   ├── obligation-dispositions/   # NEW (hand-authored): one file per area
│   ├── regressions.json           # NEW (hand-authored): one record per channel
│   ├── cases/<area>.json          # MIGRATED from cases.json (own commit)
│   ├── behaviors/<area>.json      # EXTEND: new behaviors from named sources
│   └── waivers/wvr-*.json         # EXTEND: scoped, expiring
├── obligations/obligations.json   # NEW (machine-owned): sole output of `coverage generate`
├── fixtures/                      # EXTEND: fixtures for the new cases
└── RULES.md                       # EXTEND: V26–V30 rows + obligation sections

.config/nextest.toml               # EXTEND: parity_conformance_docker in ALL profiles
fixtures/parity-corpus/registry.json # EXTEND: new binary; fix stale docker_required
```

**Structure Decision**: The existing two-crate split is preserved exactly and is the reason
the feature fits: hermetic data/validation/generation logic lives in `deacon-conformance`
(runs on every PR, no Docker, no network), and live execution/observation/injection lives in
`parity-harness` (runs only under `--profile parity`). New logic goes into **new modules**
rather than growing `validate.rs` (3,344 lines) or `model.rs` (1,909 lines), per Principle V's
modular-boundaries rule. The one structural change is splitting the declarative driver by
resource group (research Decision 4), which makes the already-declared `resourceGroup` data
meaningful instead of inert.

## Delivery Stages

Ordered so that each guard lands before the thing it guards — the failure mode called out in
the Constitution Check.

These are **stages**, deliberately not called "phases": `tasks.md` uses "Phase N" for its own
numbering, and the two do not correspond one-to-one (stage S3's reports live inside tasks
Phase 3, because generating a report is part of the US1 story). The right-hand column is the
mapping.

| Stage | Delivers | Gate it establishes | tasks.md |
|---|---|---|---|
| **S1** | Scenario model + applicability rules + `coverage generate/check` | The denominator exists and is deterministic (V26, V27) | Phase 3 (US1) |
| **S2** | Obligation dispositions + `certify` integration + fixture-driven gate tests | Undispositioned/expired blocks release (V28, V29); SC-009, SC-014 | Phase 4 (US2) |
| **S3** | The four coverage reports | Progress is measurable (SC-001, SC-002, SC-010) | Phase 3 (US1) |
| **S4** | Runner split, per-case timeout, budget assertion, `cases/<area>.json` migration | Live work can scale within budget (FR-077a/b) | Phase 2 Blocks A + B |
| **S5** | Case build-out across the workflow + Docker error-path tier | SC-003, SC-004, SC-007 | Phases 5–6 (US3, US4) |
| **S6** | De-normalization of the US5 fields | SC-008 | Phase 7 (US5) |
| **S7** | Injected regressions | SC-005, SC-006 — proves the rest can fail | Phase 8 (US6) |

S1–S3 are fully hermetic and deliver the P1 user story with zero new conformance cases, which
is the spec's stated highest-value increment. S7 is last because it validates everything
before it.

## Complexity Tracking

> No Constitution Check violations. Section intentionally empty.
