# Implementation Plan: Declarative Conformance Runner

**Branch**: `022-conformance-runner` | **Date**: 2026-07-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/022-conformance-runner/spec.md`

## Summary

Turn conformance cases from *pointers to hand-written Rust test binaries* into *fully declarative data records* that a shared **runner** executes against one of three targets (deacon under test, the pinned reference CLI, or a stored snapshot), capturing six observable channels, normalizing them with named field-specific rules, and emitting a per-channel verdict attributable to a registry behavior. Today `conformance/registry/cases.json` records carry `executable: { binary: "parity_…" }` — a named Rust test. This feature adds a declarative `operations` + `expected` + `allowedDifferences` + `cleanup` shape so a new fixture/assertion needs **no new Rust function**, plus committed, provenance-stamped snapshots under `conformance/` with staleness gating and a reviewed-only refresh path.

The work reuses, not reinvents: `parity-harness` already owns oracle resolution/verification (`oracle.rs`), bounded exec with raw capture (`exec.rs`), the single normalization module (`normalize.rs`), and report fragments (`report.rs`); `deacon-conformance` (`crates/conformance`) already owns the registry data model, loaders, and the V1–V14 validation engine that gates every PR. This plan extends both crates along their existing seams — the hermetic data/validation/staleness logic lands in `deacon-conformance`; the live execution/observation/record logic lands in `parity-harness`.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)  
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json`, `indexmap` (declaration order), `sha2` (already used in `deacon-conformance` for `hash8`/fingerprints → case/fixture hashing), `tokio` (bounded async exec + streamed capture, already in `parity-harness`), `thiserror` (`HarnessError`/domain errors), `tracing`, `toml` (nextest-profile drift check, already a `parity-harness` dep), `tempfile` (isolated external workspaces, dev-dep); no new runtime crates  
**Storage**: strict-JSON, version-controlled — extend `conformance/registry/cases.json` (declarative case shape) + `channels.json` (new channels); new committed evidence tree `conformance/snapshots/<os>-<arch>/<case-id>/{provenance,raw,normalized}.json` (atomic temp-file + `fs::rename` writes)  
**Testing**: `cargo-nextest` only. Hermetic case-schema/validation/staleness/normalization/allowed-difference tests run in `default`/`dev-fast`; the live differential + Docker channel-capture binary runs **only** under `--profile parity` (and Docker channels under the `docker` profile's resource groups). One new live binary `crates/deacon/tests/parity_conformance_runner.rs`, registered in `fixtures/parity-corpus/registry.json` + overrides in ALL profiles.  
**Target Platform**: Linux + macOS (full, incl. Docker/live channels); Windows runs the hermetic slice only (`dev-fast`, no Docker) — Docker-backed and host-hook channels are `#[cfg(unix)]`-gated with a one-line reason, matching the repo's cross-platform convention  
**Project Type**: Rust workspace — dev-only test/conformance tooling across two existing crates (`crates/conformance`, `crates/parity-harness`) + registry data; NOT a shipped `deacon` subcommand  
**Performance Goals**: the hermetic slice (schema load, staleness compare, pure normalization rules) stays in `dev-fast` at unit-test speed; live/Docker cases inherit the parity/docker lanes' existing budgets (no new perf ceiling introduced)  
**Constraints**: fail-loud, never silent-skip (missing oracle/Docker/normalizer error → run fails with a cause-specific `HarnessError`); atomic evidence writes; snapshots keyed by `os-arch`; case hash covers only behavior-affecting inputs; no global ignore lists (scoped allowed differences only)  
**Scale/Scope**: dozens→low-hundreds of declarative cases; six observable channels; four oracle types; one new live test binary; two crates extended; one new committed snapshot tree

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Verdict |
|-----------|-----------|---------|
| **I. Spec-Parity as Source of Truth** | The runner *enforces* parity; snapshots pin `oracle version` + `source revision` in provenance and verify exactness (reusing `oracle.rs::VerifiedOracle`). Live-differential compares deacon vs the pinned oracle. | ✅ Reinforces |
| **II. Consumer-Only Scope** | Cases exercise only consumer commands (`up`/`down`/`exec`/`build`/`read-configuration`/`run-user-commands`/`templates apply`/`doctor`). The runner is dev tooling — **no new shipped `deacon` subcommand**; refresh is a dev-only bin. | ✅ Pass |
| **III. Keep the Build Green** | Hermetic slice in `dev-fast`; live/Docker isolated to `parity`/`docker` profiles; new binary added to ALL profiles + `registry.json` so `parity_registry_check` stays green. | ✅ Pass |
| **IV. No Silent Fallbacks — Fail Fast** | The feature's *purpose* is anti-silent-skip: no global ignore lists (FR-032), fail-loud on missing oracle/Docker/normalizer, stale snapshot → hard fail (FR-020), unknown behavior/malformed case → load error (FR-003). | ✅ Reinforces |
| **V. Idiomatic, Safe Rust** | `thiserror` domain errors (extend `HarnessError` + conformance errors); modular observers (one module per channel); async `tokio` exec (already present); no `unsafe`; imports std→ext→local. | ✅ Pass |
| **VI. Observability & Output Contracts** | Deterministic verdict report: single JSON doc on stdout, `tracing` logs on stderr; ordered records via `IndexMap`/`Vec`; distinct exit codes (pass / diverge / stale / harness-error). | ✅ Pass |
| **VII. Testing Completeness** | 12 mandatory acceptance areas (SC-012) map to hermetic + gated tests; new live binary gets nextest overrides in every profile + `registry.json` entry (V-series `parity_registry_check`). | ✅ Pass |
| **VIII. Subcommand Consistency & Shared Abstractions** | **Central gate.** MUST reuse `parity-harness` `exec`/`normalize`/`oracle`/`report` and the `deacon-conformance` loader/validator — extend the single `normalize.rs`, do NOT fork a second normalizer or waiver mechanism (FR-043). | ✅ Pass (enforced by design) |
| **IX. Executable & Self-Verifying Examples** | Not applicable — no `examples/` directory is touched (dev/test tooling). | ✅ N/A |

**Initial Constitution Check: PASS** — no violations; Complexity Tracking left empty. The one live risk is a Principle VIII violation (forking normalization/waiver logic); the design pins all shared logic to the existing modules to prevent it.

## Project Structure

### Documentation (this feature)

```text
specs/022-conformance-runner/
├── plan.md              # This file
├── research.md          # Phase 0 output — decisions D1–D10
├── data-model.md        # Phase 1 output — entities, fields, validation, states
├── quickstart.md        # Phase 1 output — author-a-case + record/refresh walkthrough
├── contracts/           # Phase 1 output
│   ├── case-schema.md            # Declarative case record (extends cases.json)
│   ├── snapshot-provenance.md    # Snapshot + provenance JSON + staleness rules
│   ├── runner-cli.md             # Runner + refresh CLI surface, exit codes, report JSON
│   └── observer-channel.md       # Per-channel observer contract + verdict shape
└── checklists/
    └── requirements.md  # (from /speckit.specify)
```

### Source Code (repository root)

Extends two existing crates + registry data. No new crate (Principle VIII — reuse the harness).

```text
crates/conformance/                        # deacon-conformance — HERMETIC (no Docker/Node)
├── src/
│   ├── model.rs                           # + declarative Case fields (operations, oracleType,
│   │                                      #   expected, allowedDifferences, cleanup, resourceGroup,
│   │                                      #   fsAllowlist); Channel/OracleType/AllowedDifference types
│   ├── case_hash.rs                       # NEW — case hash (behavior-affecting inputs only) + fixture hash (sha2)
│   ├── snapshot.rs                        # NEW — Provenance + Snapshot model, load, staleness compare
│   ├── load.rs                            # + load/parse the new case + snapshot records
│   ├── validate.rs                        # + new violation classes (continue V-series): case operations well-formed,
│   │                                      #   oracle-type arity, allowed-difference scoping + conflict, snapshot provenance
│   ├── coverage.rs / certify.rs / report.rs  # + surface snapshot coverage + "no reference for platform" in report/certify
│   └── bin/conformance.rs                 # + `snapshot check|diff` (hermetic staleness/diff); NOT record
└── tests/                                 # NEW hermetic acceptance tests (run in dev-fast/default)
    ├── case_schema_valid.rs               # FR-001..004, FR-003 fail-loud
    ├── snapshot_staleness.rs              # FR-020, SC-003 (each provenance field + case/fixture hash)
    ├── allowed_difference_scoping.rs      # FR-031..035, SC-008 (scope + conflict + no global ignore)
    └── normalization_semantics.rs         # FR-024..029 null/empty/default, path token, label, mount, PATH (pure)

crates/parity-harness/                     # DEV-ONLY — has oracle/Docker/Node access in the parity lane
├── src/
│   ├── normalize.rs                       # EXTEND (single normalizer): named rules — path-token rewrite,
│   │                                      #   label semantic parse, mount-source substitution, PATH segment/probe,
│   │                                      #   null/empty/default preservation; bump NORMALIZER_VERSION
│   ├── runner.rs                          # NEW — orchestrate: load case → run ops against target → observe → normalize → compare
│   ├── observe/                           # NEW — one module per observable channel (Principle V modularity)
│   │   ├── cli_process.rs                 # exit code, stdout, stderr, structured output, failure phase (closed phase set)
│   │   ├── filesystem.rs                  # per-case declared allowlist capture (NOT full-tree)
│   │   ├── image.rs                       # built-image config + metadata (labels parsed semantically)
│   │   ├── container_graph.rs             # container/network/volume/mount graph
│   │   ├── injected_process.rs            # env, user, cwd, PATH resolution, signals, TTY, exit propagation
│   │   └── temporal.rs                    # lifecycle ordering, first-create vs restart, resume, cleanup
│   ├── evidence.rs                        # NEW — raw + normalized evidence, persisted SEPARATELY (atomic)
│   ├── compare.rs                         # NEW — per-channel verdict; applies scoped allowed differences
│   ├── oracle_type.rs                     # NEW — dispatch: spec-expectation | snapshot | live-differential | invariant/metamorphic
│   ├── workspace.rs                       # NEW — isolated external tempdir + collision-resistant names + RAII cleanup guard
│   └── bin/conformance-snapshot.rs        # NEW — reviewed REFRESH (record mode; runs live oracle, writes committed snapshots)
└── tests/
    └── runner_record_replay.rs            # NEW hermetic-ish: record/replay equivalence on a fixture (SC-011)

crates/deacon/tests/
└── parity_conformance_runner.rs           # NEW live binary (thin shell): drive runner over declarative cases (parity profile only)

conformance/
├── registry/
│   ├── cases.json                         # EXTEND records with declarative shape (coexists with legacy binary-backed)
│   └── channels.json                      # ADD channels: chan-image, chan-process-graph, chan-injected-process, chan-temporal
└── snapshots/                             # NEW committed evidence tree, keyed by os-arch
    └── <os>-<arch>/<case-id>/{provenance.json,raw.json,normalized.json}

fixtures/parity-corpus/registry.json       # + register parity_conformance_runner (parity_registry_check gate)
.config/nextest.toml                       # + parity_conformance_runner overrides in ALL profiles; docker resource groups for channel tests
```

**Structure Decision**: Extend the two existing conformance/parity crates along their current module seams rather than adding a crate — mandated by Principle VIII (reuse shared abstractions) and by the crate dependency direction (`parity-harness` → `deacon-conformance`). Hermetic data/validation/staleness logic lands in `deacon-conformance` (runnable in `dev-fast`, gates every PR); live execution/observation/record logic lands in `parity-harness` (Docker/Node lane). The declarative case schema is an **extension** of the already-validated `cases.json`, so the existing V1–V14 engine and `certify` gate cover it for free once new classes are added.

## Complexity Tracking

> No constitution violations. Section intentionally empty.
