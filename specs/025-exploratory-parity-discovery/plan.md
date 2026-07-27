# Implementation Plan: Exploratory Parity Discovery

**Branch**: `025-exploratory-parity-discovery` | **Date**: 2026-07-27 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/025-exploratory-parity-discovery/spec.md`

## Summary

Add a discovery pipeline that finds parity differences the curated record never anticipated:
generate valid and near-valid configurations from the **already-committed constraint inventory**,
mutate known-valid fixtures with an attributable operator catalogue, compare deacon against the
verified pinned oracle, reduce each difference structurally while preserving its normalized
signature, and emit a reviewable candidate. Findings accumulate in a persistent queue that lives
*outside* the conformance registry and is structurally unreachable from `certify`; nothing enters
the deterministic record except by human review.

The technical approach is deliberately additive rather than parallel. The grammar is the existing
`conformance/inventory/constraints.json` (609 units, fingerprint-verified against the vendored
pinned schemas), the signature derives from `normalize::diff`'s existing `ConfigDivergence`
output, and the pipeline proof reuses `inject.rs`'s sealed `EvidenceSource` boundary. Discovery
introduces no second normalization path, no second schema view, and no new workspace crate.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json` (strict-JSON records), `indexmap` (declaration order), `sha2` (`hash8` signature/fixture ids), `tokio` (bounded async exec), `thiserror` (domain errors), `tracing`, `tempfile` (dev-dep, isolated workspaces). **No new crates** — including no RNG crate (research D2).
**Storage**: strict-JSON, version-controlled. New root `conformance/discovery/` (queue, campaigns, corpus manifest) — a sibling of `registry/`, deliberately outside it. New registry file `conformance/registry/metamorphic.json` (`mrl-` relation records). Generated artifacts under `target/discovery/` (git-ignored, byte-stable). All writes atomic (temp file + `fs::rename`).
**Testing**: `cargo-nextest`. Hermetic tests (generator, mutation, shrinker, signature, queue validation, relation validation) run in `default`/`dev-fast`. Live campaigns run **only** under a new `[profile.discovery]` with an explicit `binary(=…)` allow-list.
**Target Platform**: Linux/macOS developer machines and CI. Hermetic differential tier needs Node 20 + the pinned oracle; container-backed tier additionally needs Docker; corpus canary additionally needs network. The metamorphic tier needs none of the three (research D12).
**Project Type**: development-only tooling inside an existing Rust workspace — libraries plus dev-only bins. Never part of the shipped `deacon` consumer surface (FR-059, asserted by `parity_registry_check`).
**Performance Goals**: scheduled campaign ≤ 30 min wall clock; ≤ 60 s per hermetic candidate, ≤ 5 min per container-backed candidate; ≥ 90% of candidates reach past document parsing (SC-002); median shrink ≥ 80% input-size reduction (SC-004).
**Constraints**: byte-stable outputs (no timestamps, no absolute paths); zero network and zero discovery-program selection in any PR lane (FR-055); no write path from any discovery program into the registry or snapshots (FR-036); exactly one normalization definition (FR-015); a seed must reproduce across dependency bumps (FR-001 → research D2).
**Scale/Scope**: grammar = 609 constraint units (469 non-annotation); mutation seeds = ~90 committed fixtures under `conformance/fixtures/`; corpus = 33 pinned entries; 11 observable channels; admission cap 25 new distinct signatures per campaign (research D10).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Verdict | Basis |
|---|---|---|
| **I. Spec-Parity as Source of Truth** | **PASS** | Strengthens it. The grammar is derived from the pinned spec surface, so generation explores what the spec permits rather than what a maintainer imagined. Findings are bound to the pin under which they were observed; a pin bump invalidates rather than carries them (Assumption 8). |
| **II. Consumer-Only Scope** | **PASS** | Dev-only tooling, identical posture to `deacon-conformance` and `parity-harness`. FR-059 forbids any consumer surface; `parity_registry_check` already asserts `deacon --help` gains nothing and is extended to cover the new bins (research D9). |
| **III. Keep the Build Green** | **PASS** | Hermetic tests run in `dev-fast`/`default`; every live discovery binary is excluded from all six existing profile `default-filter`s. A discovery failure cannot fail a PR (FR-058). |
| **IV. No Silent Fallbacks — Fail Fast** | **PASS** | Missing or mismatched pins fail loudly (FR-003); an injection that never landed is `InjectionInapplicable`, never "found nothing" (FR-042a); admission-cap suppression is always reported (FR-034b); a corpus digest mismatch fails that entry (FR-051). No `#[ignore]`, no silent skip. |
| **V. Idiomatic, Safe Rust** | **PASS with one tracked item** | Edition 2024, no `unsafe`, `thiserror` in the libraries, async only for bounded oracle exec. The in-repo PRNG (research D2) reimplements crate functionality — justified in Complexity Tracking; pure wrapping integer arithmetic, no `unsafe`. |
| **VI. Observability & Output Contracts** | **PASS** | Reports are deterministic and byte-stable; the new bins keep the single-JSON-document-on-stdout / diagnostics-on-stderr contract. Ordered structures (`Vec`/`IndexMap`) throughout — the `ordering-changed` signature class exists precisely because order defects are real here. |
| **VII. Testing Completeness** | **PASS** | Every mandated acceptance test in the spec maps to a test. New profile + all-profile exclusions wired per Principle VII's nextest rule; `parity_registry_check` extended so the wiring cannot drift. |
| **VIII. Subcommand Consistency & Shared Abstractions** | **PASS** | Reuses `normalize` (single definition, FR-015), `exec`, `oracle`, `prereq`, `observe`, `inject`, `hash8`, and the inventory loader. The plan explicitly forbids a second normalization or schema-extraction path (research D1, D3). |
| **IX. Executable & Self-Verifying Examples** | **N/A** | No `examples/` surface: this is dev tooling with no user-facing scenario. `quickstart.md` carries the runnable walkthrough instead, in line with 019–024. |

**Gate result: PASS.** One item tracked in Complexity Tracking; no unjustified violation.

### Post-Phase-1 re-check

Re-evaluated after `data-model.md` and `contracts/` were written. No verdict changed. Two
design outcomes strengthen earlier PASSes rather than qualifying them:

- **Principle IV**: `contracts/discovery-cli.md` gives every discovery command an exit-status
  contract where the status reflects *whether the command ran*, never *what it found* —
  extending the `coverage report` discipline to the whole surface, so no discovery output can
  become a gate by accident.
- **Principle VIII**: the shrinker takes its reproduction predicate as a parameter (research
  D4/D5), so reduction strategy stays hermetic and unit-testable while the live campaign
  supplies the real predicate. Oracle access remains confined to the crate that already owns it.

## Project Structure

### Documentation (this feature)

```text
specs/025-exploratory-parity-discovery/
├── plan.md              # This file
├── spec.md              # Feature specification (clarified 2026-07-27)
├── research.md          # Phase 0 output — 12 decisions
├── data-model.md        # Phase 1 output — record shapes, validation, state
├── quickstart.md        # Phase 1 output — runnable walkthroughs
├── contracts/           # Phase 1 output
│   ├── discovery-cli.md         # dev-only command surface + exit-status contracts
│   ├── findings-queue.md        # queue record contract + D1–D5 violations
│   └── metamorphic-catalogue.md # mrl- record contract + V31/V32
├── checklists/
│   └── requirements.md  # Spec quality checklist (passed)
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created here)
```

### Source Code (repository root)

```text
crates/conformance/src/                  # HERMETIC half (no oracle, no Docker, no network)
├── discovery/
│   ├── mod.rs                           # public surface for the hermetic half
│   ├── grammar.rs                       # constraint inventory -> generation grammar (D1)
│   ├── rng.rs                           # in-repo deterministic PRNG (D2)
│   ├── generate.rs                      # constrained candidate generation
│   ├── mutate.rs                        # the 11-category mutation operator catalogue
│   ├── shrink.rs                        # structural reduction; predicate is a parameter (D5)
│   ├── signature.rs                     # normalized signature + value-shape class (D3)
│   ├── queue.rs                         # findings-queue model, loader, D1-D5 validation (D6)
│   ├── metamorphic.rs                   # mrl- relation model + V31/V32 (D11)
│   ├── corpus.rs                        # corpus manifest model + immutable-ref validation (D8)
│   └── report.rs                        # byte-stable campaign + queue reports
├── validate.rs                          # EXTEND: V31, V32
├── load.rs                              # EXTEND: load metamorphic.json (registry side only)
└── bin/conformance.rs                   # EXTEND: `discovery` command group

crates/parity-harness/src/               # LIVE half (oracle / Docker / network)
├── discovery/
│   ├── mod.rs
│   ├── campaign.rs                      # driver: budget, seed, admission cap, tiers
│   ├── differential.rs                  # deacon vs oracle over a candidate
│   ├── metamorphic_run.rs               # deacon-only relation evaluation (D12)
│   ├── minimize.rs                      # supplies the live reproduction predicate to shrink.rs
│   ├── candidate.rs                     # assemble the reviewable candidate
│   ├── corpus_fetch.rs                  # network-lane fetch + digest verification (D8)
│   └── pipeline_proof.rs                # injected-difference proof via sealed EvidenceSource (D7)
└── bin/
    ├── discovery-campaign.rs            # run a campaign (seed + budget required)
    └── discovery-proof.rs               # the FR-042a pipeline proof

crates/deacon/tests/
├── discovery_hermetic.rs                # hermetic guards; runs in default/dev-fast
├── discovery_campaign.rs                # LIVE; [profile.discovery] allow-list only
├── discovery_metamorphic.rs             # deacon-only, no external prereq; discovery profile
│                                        #   (excluded from PR lanes for stochasticity, not cost)
└── parity_registry_check.rs             # EXTEND: discovery lane wiring + no consumer surface

conformance/
├── discovery/                           # NEW ROOT — outside registry/, unreachable from certify
│   ├── findings.json                    # the persistent findings queue
│   ├── campaigns.json                   # campaign provenance (seed + pinned input set)
│   └── corpus.json                      # 33 pinned entries + content digests
└── registry/
    └── metamorphic.json                 # NEW — hand-authored mrl- relation records

.config/nextest.toml                     # NEW [profile.discovery]; exclusions in all 6 profiles
.github/workflows/discovery.yml          # NEW — scheduled + workflow_dispatch lanes
Makefile                                 # NEW targets: test-discovery, test-discovery-proof
```

**Structure Decision**: No new workspace crate. The feature splits along the **hermetic/live**
line established by 022 — pure data-to-data logic (grammar, generation, mutation, reduction
strategy, signature, queue, relations, reports) in `deacon-conformance`; everything touching the
oracle, Docker, or the network in `parity-harness`. This keeps the generator, shrinker, and
signature unit-testable in the fast lane with no external dependency, which is what makes the
hermeticity claim of FR-055 cheap to hold rather than a thing to be careful about. Two data roots
are introduced rather than one: `conformance/discovery/` (machine-produced candidates, outside
the registry) and one registry file `metamorphic.json` (hand-authored assertions, inside it) —
the split is the subject of research D11.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| In-repo PRNG instead of the `rand` crate (research D2) | FR-001 requires a recorded seed to reproduce an identical candidate sequence, and FR-034 requires findings to persist indefinitely — so a seed recorded today must still reproduce after arbitrary dependency updates. | `rand` does not offer value-stream stability across versions; its stream is documented as an implementation detail. Pinning it to freeze behavior fights Principle V's dependency-hygiene rule and still breaks under a forced advisory bump. Making the stream a property of committed code turns a silent corpus-wide invalidation into a reviewable pin change, exactly as `NORMALIZER_VERSION` does. ~40 lines, no `unsafe`, tested against published vectors. |
| A second data root under `conformance/` (research D6) | The clarified requirement is that no finding — reviewed or not — can influence `certify`. `load.rs` enumerates named subdirectories under `conformance/registry/`, so a sibling of `registry/` is structurally unreachable rather than merely conventionally separate. | Placing the queue inside `conformance/registry/` means either the loader rejects it, or someone wires it in and unreviewed, machine-produced findings join the certification denominator — the silent failure mode 024 D1 documented. A `target/`-only queue cannot satisfy FR-030's cross-campaign deduplication or FR-034's persistence. |

## Phase 2 note

`/speckit.plan` stops here. `tasks.md` is produced by `/speckit.tasks`. Sequencing guidance for
that step, from research D12: the **metamorphic tier is the cheapest complete vertical slice** —
it exercises generation → comparison → signature → candidate with no oracle, no Docker, and no
network, so building it first proves the hermetic spine before any live provisioning exists.
