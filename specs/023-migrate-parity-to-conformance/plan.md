# Implementation Plan: Migrate Parity Assets into the Declarative Conformance System

**Branch**: `023-migrate-parity-to-conformance` | **Date**: 2026-07-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/023-migrate-parity-to-conformance/spec.md`

## Summary

Deacon proves spec fidelity through two overlapping systems: a hand-written parity harness (12 comparison programs, two corpora, an external manifest, and a normalization/diff vocabulary) and a declarative conformance record (cases as data, executed by one shared runner). The second was built to replace the first but only partially has: **25 of 31 registry cases are still pointers at hand-written programs**, and those 25 pointers stand in for **111 enumerated baseline units** — the record under-counts real coverage by roughly 3.6× (research D2).

This feature completes the migration under a conservation constraint. The approach is a frozen, mechanically enumerated baseline (111 units, established in research §1, not recalled); an explicit unit → case mapping that forbids orphans in both directions; a deterministic before-and-after coverage report that must account for every baseline item; and an equivalence ledger that permits deleting a superseded program only when its replacement is provably equivalent-or-stricter over the whole baseline.

Two findings shape the sequencing. First, the legacy config normalizer's `prune` blanket-drops every null and empty value plus `configFilePath`, and `DiffKind::DeaconOnly` is ranked lowest as "usually default noise" — this *is* the deacon-only-as-serialization-noise assumption the spec forbids, so migrating the 48 corpus units is expected to surface previously hidden differences (research D3). Second, residuals concentrate in `parity_state_diff` and `parity_observable_state`; the realistic outcome is that four programs are deleted and those two persist carrying residual records (research D4).

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json` (strict-JSON records), `indexmap` (declaration order), `sha2` (unit/case/fixture hashing), `tokio` (bounded async exec in the harness), `thiserror` (domain errors), `tracing`, `toml` (nextest-profile drift check), `tempfile` (dev-dep, isolated workspaces). **No new crates, no new dependencies** (research D6).
**Storage**: strict-JSON, version-controlled. New: `conformance/migration/baseline.json` (frozen inventory), `conformance/migration/mapping.json` (unit → case/residual), `conformance/registry/residuals.json` (residual records). Extended: `conformance/registry/cases.json`. Generated (git-ignored): `target/conformance/migration-report.{json,md}`, `target/parity/equivalence.json`. All writes atomic (temp file + `fs::rename`).
**Testing**: `cargo-nextest`. Hermetic checks (baseline drift, mapping validation, migration report, fault classes) run in **every** profile with no Docker and no network; the equivalence ledger runs only under `--profile parity`.
**Target Platform**: Linux/macOS/Windows for the hermetic path (the Windows `dev-fast` lane compiles and runs it); live comparison remains Linux + Docker + pinned oracle.
**Project Type**: Rust workspace; dev-only tooling in two existing crates (`deacon-conformance` hermetic, `parity-harness` live). Never a shipped `deacon` subcommand (Constitution II).
**Performance Goals**: baseline enumeration and coverage report each complete in < 5 s and are byte-stable across runs (SC-012).
**Constraints**: deterministic output — no timestamps, no absolute paths, no machine-specific values; fail-loud with cause-specific errors, never a silent skip; generation never writes hand-authored classification/disposition files.
**Scale/Scope**: 111 baseline units, 37 in-repo fixture dirs, 19 normalization rules, 6 result vocabularies, 16 characterized exceptions; registry grows from 31 cases toward ≈111 while behaviors stay at 25 or fall.

## Constitution Check

*GATE: evaluated before Phase 0 and re-evaluated after Phase 1.*

| Principle | Assessment | Verdict |
|---|---|---|
| **I. Spec-Parity as Source of Truth** | No deacon runtime behavior changes. The feature strengthens the record *of* spec parity. Deferrals are tracked in research §4 and must appear in `tasks.md` under `## Deferred Work`. | ✅ PASS |
| **II. Consumer-Only Scope** | All machinery is dev-only, invoked via `cargo run -p deacon-conformance` / `-p parity-harness`. A hermetic test must assert no new subcommand reaches the shipped `deacon` CLI. | ✅ PASS (with test obligation) |
| **III. Keep the Build Green** | Standard cadence. New hermetic tests run in `dev-fast`, so they gate every iteration rather than only the parity lane. | ✅ PASS |
| **IV. No Silent Fallbacks — Fail Fast** | Reinforced, not merely respected: FR-023 forbids reporting a pass when comparison did not occur; FR-019 requires each process failure to keep its own cause; residuals and `no-reference-for-platform` are explicit states rather than skips. | ✅ PASS (reinforcing) |
| **V. Idiomatic, Safe Rust** | New modules are focused (`baseline`, `mapping`, `conservation` in conformance; `equivalence` in the harness). No `unsafe`. Async only where the harness already spawns processes. | ✅ PASS |
| **VI. Observability & Output Contracts** | The dev CLI honors the stdout/stderr contract: report JSON on stdout in JSON mode, diagnostics on stderr. Ordered maps (`IndexMap`) preserve declaration order in emitted records. | ✅ PASS |
| **VII. Testing Completeness** | FR-046–FR-048 mandate seven acceptance areas, each demonstrated to fail (FR-047). Any new test binary needs nextest overrides in **all** profiles plus a `registry.json` entry, or `parity_registry_check` fails. | ✅ PASS (with config obligation) |
| **VIII. Subcommand Consistency & Shared Abstractions** | This principle *is* FR-029/FR-030. The migration's end state is one runner and one normalizer. **Transitionally violated** — see Complexity Tracking. | ⚠️ JUSTIFIED EXCEPTION |
| **IX. Executable & Self-Verifying Examples** | No `examples/` change. | N/A |

**Post-Phase-1 re-evaluation**: the design in `data-model.md` and `contracts/` introduces no new crate, no new dependency, and no new shipped command surface; the single justified exception (VIII, transitional dual comparison paths) is unchanged in scope and remains bounded by the US7 equivalence gate. **Gate passes.**

## Project Structure

### Documentation (this feature)

```text
specs/023-migrate-parity-to-conformance/
├── plan.md              # This file
├── research.md          # Phase 0: the enumerated 111-unit baseline + decisions D1–D9
├── data-model.md        # Phase 1: entities, schemas, validation classes V21–V25
├── quickstart.md        # Phase 1: how to run the migration loop
├── contracts/
│   ├── baseline-inventory.md   # baseline.json schema + enumeration contract
│   ├── migration-report.md     # before/after conservation accounting + failure modes
│   ├── equivalence-ledger.md   # equivalent-or-stricter comparison contract
│   └── cli-commands.md         # dev-only command surface
├── checklists/
│   └── requirements.md  # spec quality checklist (passing)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/core/                              # unchanged
crates/deacon/
├── src/                                  # unchanged — no shipped surface added
└── tests/
    ├── parity_corpus_tier1.rs            # DELETE when its 24 units migrate
    ├── parity_corpus_merged.rs           # DELETE when its 24 units migrate
    ├── parity_corpus_errors.rs           # DELETE when its 9 units migrate
    ├── parity_read_configuration.rs      # DELETE when its 2 units migrate
    ├── parity_exec.rs                    # migrate 4 units; delete if residual-free
    ├── parity_build.rs                   # migrate 6 units; delete if residual-free
    ├── parity_up_exec.rs                 # migrate 1 unit; delete if residual-free
    ├── parity_observable_state.rs        # 7 units — expected to persist w/ residuals
    ├── parity_state_diff.rs              # 8 units — expected to persist w/ residuals
    ├── parity_conformance_runner.rs      # the surviving live driver (grows)
    ├── parity_harness_faults.rs          # EXTEND with declarative failure classes (D9)
    ├── parity_registry_check.rs          # EXTEND: baseline/mapping/no-shipped-surface guards
    └── corpus_runner/mod.rs              # DELETE with the corpus binaries

crates/conformance/src/                   # hermetic (deacon-conformance)
├── baseline.rs                           # NEW: enumerate + verify the frozen inventory
├── mapping.rs                            # NEW: unit → case/residual, orphan detection
├── conservation.rs                       # NEW: before/after accounting report
├── residual.rs                           # NEW: residual records (non-blocking, queued)
├── validate.rs                           # EXTEND: V21–V25
├── report.rs / certify.rs                # EXTEND: residual queue as non-blocking info
└── bin/                                  # EXTEND: baseline / migration subcommands

crates/parity-harness/src/                # live
├── equivalence.rs                        # NEW: equivalent-or-stricter ledger
├── normalize.rs                          # SHRINK: retire prune/sanitize as units migrate
├── runner.rs / observe/                  # EXTEND only as existing coverage requires
└── bin/equivalence-report.rs             # NEW: live ledger producer (parity profile)

conformance/
├── migration/baseline.json               # NEW: frozen 111-unit inventory
├── migration/mapping.json                # NEW: unit → case/residual mapping
├── registry/residuals.json               # NEW: residual records
├── registry/cases.json                   # GROWS: 31 → ≈111
└── fixtures/                             # GROWS: 2 → ≈37+ migrated fixture dirs

fixtures/parity-corpus/                   # DELETE per-corpus as units migrate;
                                          # fetch_realworld_corpus.py survives (D8)
.config/nextest.toml                      # UPDATE in lockstep as binaries are deleted
```

**Structure Decision**: No new crate. The work splits along the seam 022 already established (research D6) — hermetic data, validation, and accounting in `deacon-conformance` so they gate every PR; live execution, observation, and the equivalence ledger in `parity-harness` so they stay in the parity lane. Fixtures migrate from `fixtures/parity-corpus/` into `conformance/fixtures/` one-to-one (FR-012), and each parity binary is deleted only when the equivalence gate clears every unit it carries.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| **Two comparison paths coexist transitionally** (Constitution VIII; FR-030 end-state) — the legacy `prune`/`diff_states` normalizers run alongside the declarative channel normalizer | FR-033 requires running *both* paths over the full baseline to prove the replacement is never more permissive. Deleting the old path first makes that proof impossible. | Cutting over without proof is the exact failure this feature exists to prevent. Bounded by an explicit invariant: no case may be *added* to a legacy carrier once migration starts (hermetic test: legacy carrier case counts may only decrease), and each carrier is deleted the moment its last unit clears the equivalence gate. |
| **Three new committed data files** (`baseline.json`, `mapping.json`, `residuals.json`) | The baseline must be frozen and diff-reviewable to be falsifiable (FR-004/FR-045); the mapping must be explicit because equal counts do not prove no item was lost (research D7); residuals need a home that is queryable but non-blocking (FR-054). | Folding all three into `cases.json` was rejected: it mixes machine-derived data (baseline) with hand-authored data (cases), which V14-style provenance rules exist to keep apart, and it would make `cases.json` churn on every enumeration. |
| **`baseline.json`'s drift gate is removed at feature completion** (FR-053) | The baseline describes the pre-migration world. Once migration completes, a permanent drift gate would forbid ever changing the machinery the migration retires. | Keeping the gate forever was rejected as self-defeating; deleting the artifact was rejected because it is the evidence for the conservation claim. Retain the artifact, retire the gate. |
