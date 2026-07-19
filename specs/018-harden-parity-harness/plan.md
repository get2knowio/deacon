# Implementation Plan: Harden the Parity Test Harness

**Branch**: `018-harden-parity-harness` | **Date**: 2026-07-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/018-harden-parity-harness/spec.md`

## Summary

Make a passing parity result a trustworthy certification. Today every `parity_*`
test binary silently early-returns as PASS when the oracle (`@devcontainers/cli`),
Docker, or the `DEACON_PARITY=1` opt-in is absent; two of three Python corpus
runners always exit 0; the 0.87.0 oracle version is prose-only; normalization
exists in three divergent copies; and `make test-parity` bypasses nextest via
`cargo test`. The plan: introduce a dev-only `parity-harness` support crate that
owns oracle resolution + exact-version verification (pin read from a single
machine-readable file), one normalization module per comparison type, one waiver
schema/loader, per-binary run-report fragments with raw-output preservation, and
a report aggregator that validates completeness against a parity registry. All
parity checks move to a dedicated nextest `parity` profile (excluded by selection
from every other profile), the three Python corpus runners are ported to Rust
test binaries and retired, the two oracle-free "parity"-named binaries are
reclassified, and a new CI certification lane (nightly + dispatch + parity-path
PRs) provisions the pinned oracle and gates on the aggregated report.
Fault-injection acceptance tests (stub oracles, stub docker, malformed output,
injected diffs) prove every failure mode fails.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide); no Python after porting (the three corpus-runner scripts are retired)
**Primary Dependencies**: `serde`/`serde_json`, `tokio` (process + time for bounded oracle invocations), `thiserror`, `tracing`, `chrono` (already a workspace dep in `deacon-core` — RFC3339 report timestamps); **one new dev-scope dependency**: `toml` (registry check parses `.config/nextest.toml`); `cargo-nextest` as the sole test executor; Node 20+/npm in the certification lane to install the oracle
**Storage**: files — `fixtures/parity-corpus/oracle.json` (pin), `fixtures/parity-corpus/registry.json` (parity registry), waiver records adjacent to cases (`errors/*/expect.json`, `waivers/*.json`); run artifacts under `target/parity/` (report fragments + raw outputs), overridable via `DEACON_PARITY_REPORT_DIR`
**Testing**: `cargo-nextest` exclusively; new `[profile.parity]` in `.config/nextest.toml`; harness self-tests (fault injection, registry check) run hermetically in the fast lane; live parity runs only in the certification lane / `make test-parity` alias
**Target Platform**: Linux (certification lane: ubuntu-latest with Docker); harness self-tests compile everywhere (Windows dev-fast lane compiles all test binaries — stub-script fault-injection tests are `#[cfg(unix)]`-gated with reasons)
**Project Type**: CLI workspace — test-infrastructure feature (new internal support crate + test binaries + CI workflow); no production-code behavior change
**Performance Goals**: oracle invocation bounds: 2 min configuration-only, 15 min container-lifecycle (matches existing slow-timeout ceiling); certification lane wall-clock target ≤ 45 min
**Constraints**: no network in non-certification test lanes (hermetic fault-injection via stubs); nextest-only gating (Makefile target becomes a thin alias); all-profile nextest override coverage for every new/renamed binary; report/artifact writes are atomic (temp + rename, matching `cache/disk.rs::save_index` pattern)
**Scale/Scope**: 8 existing parity binaries (6 live, 2 reclassified), 3 corpus runners to port, ~30 valid-config corpus cases + 9 error cases, 7 nextest profiles to update, 1 new CI workflow

## Constitution Check

*GATE: evaluated against constitution v1.14.0 — PASS (pre-Phase-0 and re-checked post-Phase-1).*

| Principle | Assessment |
|---|---|
| I. Spec-Parity as Source of Truth | Strengthened, not touched: this feature hardens the mechanism that *verifies* spec parity against the pinned reference (0.87.0) and upstream spec commit `113500f4`. No production behavior changes. Divergences remain encoded as characterized waivers (Tier 1c corpus), now schema-validated. |
| II. Consumer-Only Scope | In scope: test infrastructure for consumer commands (`read-configuration`, `up`, `exec`, `build`). No authoring surface added. |
| III. Keep the Build Green | Improved: parity checks leave `default`/`full`/`ci`/`mvp-integration` selection (they currently "pass" there vacuously), so `make test-nextest` stays green without the oracle — by honest exclusion instead of silent skip. `make test-parity` becomes a nextest alias (constitution's "use make test-nextest-* / nextest exclusively" satisfied; the current `cargo test` side-channel is removed). No `#[ignore]` anywhere (FR-023 enforces). |
| IV. No Silent Fallbacks — Fail Fast | This feature is the principle applied to the harness itself: prerequisite absence → hard, cause-specific failure; empty corpus → failure; report-write failure → run failure. |
| V. Idiomatic, Safe Rust | New `crates/parity-harness` (publish = false) with `thiserror` domain errors, `tokio::process` + `tokio::time::timeout` for bounded oracle calls (no blocking IO in async), no `unwrap` in harness runtime paths, atomic file writes. |
| VI. Observability & Output Contracts | Report fragments and aggregated report are single JSON documents; diagnostics to stderr. Raw stdout/stderr of both CLIs preserved verbatim per case. |
| VII. Testing Completeness | All spec-mandated acceptance tests enumerated (fault injections, registry completeness, no-skip audit). Every new/renamed binary gets nextest group overrides in ALL profiles (the 3-spot dev-fast rule per binary). Fault-injection tests are hermetic (stub executables, no network). |
| VIII. Subcommand Consistency & Shared Abstractions | Consolidation is the point: one normalization module, one waiver loader, one oracle resolver — replacing 3 normalization copies and 2 waiver mechanisms. |
| IX. Executable & Self-Verifying Examples | No examples change (test-infra only). `fixtures/parity-corpus/README.md` updated in lockstep with retired scripts. |

**Violations requiring justification**: none. (New crate is justified under V "Modular
Boundaries": the harness is shared by 9+ test binaries and needs its own unit tests,
clippy/fmt coverage, and a report-aggregator `bin` target — impossible as a
`tests/parity_utils.rs` include-module.)

## Project Structure

### Documentation (this feature)

```text
specs/018-harden-parity-harness/
├── plan.md              # This file
├── research.md          # Phase 0 output — decisions D1–D12
├── data-model.md        # Phase 1 output — pin/registry/waiver/report entities
├── quickstart.md        # Phase 1 output — local + CI runbook
├── contracts/           # Phase 1 output
│   ├── execution-contract.md    # nextest profile, env vars, exit semantics
│   ├── report-schema.md         # fragment + aggregated report JSON schemas
│   └── registry-waiver-schema.md# registry.json + waiver record schemas
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/parity-harness/           # NEW dev-only support crate (publish = false)
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── oracle.rs                # pin loading, resolution, exact-version verification (D1, D3)
    ├── prereq.rs                # docker/fixture prerequisite checks → hard errors (D3)
    ├── exec.rs                  # bounded oracle/deacon invocation (2 min / 15 min), raw capture (D3, D7)
    ├── normalize.rs             # THE single normalization module per comparison type (D7)
    ├── waiver.rs                # unified waiver schema + loader + staleness validation (D6)
    ├── registry.rs              # registry.json loading + completeness checks (D5)
    ├── report.rs                # fragment writing (atomic), raw-output preservation (D8)
    └── bin/
        └── parity-report.rs     # aggregator: fragments → parity-report.json, completeness gate (D8)

crates/deacon/tests/             # test binaries (dev-dependency on parity-harness)
├── parity_read_configuration.rs # hardened: fail-fast prereqs, report fragments
├── parity_exec.rs               # hardened
├── parity_build.rs              # hardened
├── parity_up_exec.rs            # hardened (gains Docker prereq check)
├── parity_observable_state.rs   # hardened; known-divergence lists → waiver files
├── parity_state_diff.rs         # hardened
├── parity_corpus_tier1.rs       # NEW: port of run_tier1.py (D4)
├── parity_corpus_merged.rs      # NEW: port of run_tier1_merged.py (D4)
├── parity_corpus_errors.rs      # NEW: port of run_tier1_errors.py (D4)
├── parity_registry_check.rs     # NEW: registry completeness + no-skip audit (runs in fast lane) (D5, D10)
├── parity_harness_faults.rs     # NEW: fault-injection acceptance tests (stub oracle/docker) (D10)
├── consistency_remote_env_flags.rs  # RENAMED from parity_remote_env_flags.rs (D9)
├── consistency_env_probe_flag.rs    # RENAMED from parity_env_probe_flag.rs (D9)
└── parity_utils.rs              # DELETED (absorbed into parity-harness)

fixtures/parity-corpus/
├── oracle.json                  # NEW: single authoritative oracle pin (D1)
├── registry.json                # NEW: parity registry — binaries, corpora, min case counts (D5)
├── waivers/                     # NEW: observable-state waiver records (adjacent-to-case model) (D6)
├── errors/*/expect.json         # kept; now schema-validated by waiver loader (D6)
├── run_tier1.py                 # DELETED (ported)
├── run_tier1_merged.py          # DELETED (ported)
├── run_tier1_errors.py          # DELETED (ported)
└── fetch_realworld_corpus.py    # kept (corpus-fetch utility, not a runner)

.config/nextest.toml             # [profile.parity] NEW; parity binaries removed from
                                 # default/full/ci/mvp-integration selection; overrides
                                 # for new/renamed binaries in ALL profiles (D2)
.github/workflows/parity.yml     # NEW certification lane (nightly + dispatch + parity paths) (D11)
Makefile                         # test-parity → thin alias to nextest parity profile + aggregator (D12)
Cargo.toml                       # workspace member + dev-dep wiring
```

**Structure Decision**: single workspace, one new internal support crate
(`crates/parity-harness`), all comparison logic centralized there; test binaries in
`crates/deacon/tests/` stay thin (fixture selection + assertions). Registry, pin,
and waivers live under `fixtures/parity-corpus/` next to the corpus they govern.

## Complexity Tracking

No constitution violations to justify. The one structural addition (third workspace
crate) is required by Principle V modular-boundaries and by the aggregator `bin`
target; the alternative (growing the 981-line `parity_utils.rs` include-module,
which gets recompiled into every test binary and has no unit-test/clippy surface of
its own) is the status quo this feature exists to retire.
