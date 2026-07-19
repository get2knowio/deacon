# Implementation Plan: Repository-Owned Conformance Registry

**Branch**: `019-conformance-registry` | **Date**: 2026-07-19 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/019-conformance-registry/spec.md`

## Summary

Build a validated, repository-owned conformance registry — JSON data under
`conformance/registry/` plus a new dev-only crate `crates/conformance/`
(package `deacon-conformance`) providing `validate` / `report` / `certify` subcommands.
The registry models four source inventories (schema constraints, spec clauses, CLI
surface, observed reference behavior) mapped many-to-many onto deduplicated behavior
units carrying a three-axis disposition (spec status / reference status / project
decision), with applicability contexts, observable channels, executable-test case links,
gaps, waivers, and Deacon extensions. Validation enforces ten violation classes and runs
hermetically on every PR; strict certification (blocks on gaps/uncovered behaviors)
gates the release workflow; reports are byte-deterministic. Existing parity waivers and
error-corpus expectations migrate into the registry, which `parity-harness` then
consumes — retiring the duplicate exception lists.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only (`serde`/`serde_json`, `indexmap`,
`thiserror` core / `anyhow` at bin boundary, `clap`, `tracing`) plus **new dev-only**
`jiff` (current UTC civil date for waiver expiry; confined to `deacon-conformance`)
**Storage**: strict-JSON files under `conformance/registry/` (version-controlled, hand-edited,
PR-reviewed); reports to `target/conformance/`; fixtures under `fixtures/conformance/`
**Testing**: `cargo-nextest` — all new tests hermetic (no network, no Docker); no new
nextest groups required; the `registry_valid` test validates the real registry per PR
**Target Platform**: cross-platform dev tooling — must compile and pass on the Windows
`dev-fast` lane (no Unix-only APIs; separator-agnostic path assertions)
**Project Type**: workspace dev-only crate (`publish = false`, like `parity-harness`) + data
**Performance Goals**: validate + report < 30 s (SC-008); expected < 1 s at seed scale
**Constraints**: deterministic byte-identical `report.json` (no timestamps/abs-paths/env
data; ID-sorted iteration); injectable `--today`; all violations reported per run
**Scale/Scope**: seed ≈ 10s of behaviors / ~15 divergence records / 9 error-corpus cases,
growing to O(10²–10³) records as inventories populate incrementally

## Constitution Check

*GATE: evaluated pre-Phase-0 and re-evaluated post-design — PASS, no violations.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-parity as source of truth | PASS | The feature *strengthens* this principle: pins (`rev-spec-113500f4`, `rev-oracle-0-87-0`) become first-class validated records; divergences get explicit three-axis characterization instead of prose. No spec-defined runtime behavior changes. |
| II. Consumer-only scope | PASS | No new `deacon` CLI subcommand; `conformance` is contributor tooling in a `publish = false` crate, invoked via `cargo run -p deacon-conformance`. |
| III. Keep the build green | PASS | New tests are hermetic and fast; `registry_valid` gates PRs; full-gate cadence unchanged. Parity-harness migration keeps the nine live binaries' logic untouched (query API preserved). |
| IV. No silent fallbacks | PASS | Validation fails fast with class codes + record IDs + locations; registry files use `deny_unknown_fields` (registry is deacon-modeled data — "strict on mistakes" side applies; the "preserve unmodeled" side governs devcontainer.json, not deacon's own formats). All violations reported per run, none swallowed. |
| V. Idiomatic, safe Rust | PASS | `thiserror` domain errors in the lib, `anyhow` at the bin; no async needed (pure file IO in a CLI tool — sync is correct here); modular: `model` / `load` / `validate` / `coverage` / `report` / `certify` modules. One new lean dep (`jiff`), dev-only. |
| VI. Observability & output contracts | PASS | `--json` mode: single JSON doc on stdout, logs on stderr; text mode mirrors. Exit codes 0/1/2 defined in contracts/cli.md. ID-sorted serialization satisfies the ordering rule. |
| VII. Testing completeness | PASS | FR-029's mandated suites map to concrete hermetic tests (one fixture per violation class, traceability, applicability, expiry incl. boundary, gap/certification, determinism byte-equality). No `#[ignore]`. No new nextest groups needed (no Docker/fs-heavy). |
| VIII. Shared abstractions | PASS | Waiver record types/loaders move to `deacon-conformance`; `parity-harness` consumes them (single normalization of the waiver concept, no duplicate schema). No overlap with consumer-CLI helpers. |
| IX. Executable examples | PASS (N/A) | Dev tooling — no `examples/` surface. quickstart.md documents contributor flows. |

**Post-design re-check**: PASS — design introduces no violations; Complexity Tracking
left empty.

## Project Structure

### Documentation (this feature)

```text
specs/019-conformance-registry/
├── plan.md              # This file
├── research.md          # Phase 0 — 9 numbered decisions
├── data-model.md        # Phase 1 — entities, file layout, violation classes V1–V10
├── quickstart.md        # Phase 1 — contributor flows
├── contracts/
│   ├── cli.md           # conformance validate/report/certify contract
│   ├── registry-schema.md
│   └── report-schema.md
└── tasks.md             # Phase 2 (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
conformance/                        # NEW — authoritative registry data
├── RULES.md                        # contradiction rules R1–R8 + scope notes (FR-014)
└── registry/
    ├── revisions.json  ├── dimensions.json  ├── channels.json  ├── profiles.json
    ├── sources/{schema,spec,cli,observed}.json
    ├── behaviors/<area>.json
    ├── cases.json  ├── gaps.json  ├── extensions.json
    └── waivers/<wvr-id>.json       # migrated from fixtures/parity-corpus/{waivers,errors/*/expect.json}

crates/conformance/                 # NEW — dev-only crate `deacon-conformance` (publish = false)
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── model.rs                    # record types, enums, ID rules (serde, deny_unknown_fields)
│   ├── load.rs                     # registry loading with located SCHEMA errors
│   ├── validate.rs                 # V1–V10 + contradiction rules R1–R8
│   ├── coverage.rs                 # derived coverage states, denominators, profile filtering
│   ├── report.rs                   # deterministic report.json + report.md rendering
│   ├── certify.rs                  # strict certification evaluation
│   └── bin/conformance.rs          # clap CLI: validate | report | certify
└── tests/
    ├── registry_valid.rs           # validates the REAL conformance/registry/ (PR gate)
    ├── validation_classes.rs       # one fixture per violation class (SC-002)
    ├── traceability.rs             # source→behavior→context→case→outcome chain
    ├── applicability.rs            # in-profile vs out-of-profile
    ├── waiver_expiry.rs            # valid / expired / boundary via --today injection
    ├── gap_certification.rs        # gap visibility + strict-cert blocking
    ├── report_determinism.rs       # repeat-run byte equality
    ├── disposition_rules.rs        # three-axis combos accepted/rejected per R1–R8
    └── seed_completeness.rs        # legacy divergence inventory fully migrated (SC-001)

fixtures/conformance/               # NEW — test registries: valid/ + invalid-v1/ … invalid-v10/ + schema-error/

crates/parity-harness/              # MODIFIED
├── Cargo.toml                      # + dep on deacon-conformance
└── src/waiver.rs                   # WaiverSet becomes thin wrapper over registry loader (query API unchanged)

fixtures/parity-corpus/             # MODIFIED — waivers/ and errors/*/expect.json removed (migrated);
                                    #   errors/<case>/ fixture dirs stay (they are test inputs, not exceptions)

.github/workflows/release.yml      # MODIFIED — verify job gains blocking `conformance certify` step
CLAUDE.md / docs/DIFFERENTIATORS.md # MODIFIED — divergence prose reduced to registry pointers (FR-027)
```

**Structure Decision**: New top-level `conformance/` for authoritative data (not
`fixtures/` — wrong connotation), new dev-only workspace crate mirroring the
`parity-harness` precedent, harness consumes the registry (dependency direction:
`parity-harness` → `deacon-conformance`). Rationale in research.md Decisions 1–3.

## Complexity Tracking

*No constitution violations — table intentionally empty.*
