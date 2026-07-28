# Implementation Plan: Continuous Conformance Operation & Release Certification

**Branch**: `026-continuous-conformance-certification` | **Date**: 2026-07-28 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/026-continuous-conformance-certification/spec.md`

## Summary

Turn the conformance record from an on-demand validator into a continuously operated system with three
capabilities: **explicit lanes** whose inclusion rules are data and whose unit denominator is machine-derived
(so a unit covered by nothing fails validation rather than passing silently); **source drift detection** that
observes upstream and produces review artifacts only, with no write path to any pin, disposition, or snapshot;
and a **release-grade certification report** that states exactly what was certified and refuses to certify on
any of nine enumerated failure conditions.

The technical approach follows the existing hermetic/live split. Everything that reasons about committed data
lives in `deacon-conformance` (no network, no Docker, no oracle) and runs on every PR. Everything that touches
the network, a container engine, or the reference implementation lives in `parity-harness` behind dedicated
nextest profiles. The two are joined by one new artifact — the **execution manifest** — which lets a hermetic
certifier assert that container-backed execution actually happened, resolving the otherwise-contradictory pair
"certification is hermetic" and "missing Docker execution must block the release".

Three findings from grounding the design in the current code shaped it:

- The `oracleType` distribution makes the oracle-free lanes viable without inventing anything: of 204 cases,
  81 are `spec-expectation`, 1 `snapshot`, 2 `invariant-metamorphic` — 84 that need no live reference. Of the
  81 Docker cases, 39 are oracle-free. So PR-Docker has real content (39 cases) and the nightly stable
  differential keeps the 42 live-differential Docker cases plus the 67 non-Docker ones.
- Expired waivers **already** block `certify` (the V6 disposition gate), so FR-041(e) needs tests, not code.
- Committed-snapshot handling is currently non-blocking *by deliberate 022 design*. FR-041(c) requires a
  precise refinement rather than a reversal — see research D5.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json`, `indexmap` (declaration order),
`sha2` (unit/manifest hashing), `tokio` (bounded async process exec), `thiserror`, `tracing`, `toml`
(nextest-profile drift check, already a `parity-harness` dep), `clap`, `chrono` (already in `parity-harness`),
`tempfile`. **No new crates** — network access reuses the 025 precedent of driving `git` (blob-filtered partial
clone) and `npm` as subprocesses, which needs no HTTP client, no API token, and hits no rate limit.
**Storage**: strict-JSON, version-controlled. New roots `conformance/lanes/` (hand-authored lane records) and
`conformance/drift/` (machine-owned upstream observations); new file `conformance/discovery/canary.json`.
Generated, git-ignored: `target/conformance/{execution-manifest,certification}.{json,md}`,
`target/drift/{scan,upgrade-proposal}.{json,md}`. All writes atomic (temp file + `fs::rename`).
**Testing**: `cargo-nextest`. New hermetic binaries join `default`/`dev-fast`; new live binaries get their own
profiles with explicit `binary(=…)` allow-lists and exclusions in **all** other profiles.
**Target Platform**: Linux x86_64 for the certified profile; hermetic lanes must also stay green on Windows
(`dev-fast` runs there, and nextest compiles all test binaries before filtering).
**Project Type**: Rust workspace — dev-only tooling crates plus CI workflow definitions. **No change to the
shipped `deacon` CLI surface.**
**Performance Goals**: PR-Hermetic under 3 minutes (pure data validation plus ~45 oracle-free non-Docker case
replays); PR-Docker within the existing 30-minute tier budget already asserted by the Docker driver.
**Constraints**: Certification must run with no network, no reference implementation installed, and no
container engine. Every generated artifact must be byte-stable (no timestamps, absolute paths, or hostnames).
**Scale/Scope**: 5 lanes, ~200 declarative cases, 11 observable channels, 5 upstream source kinds, 9
certification failure conditions, 4 new validation classes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design — see "Post-Design Re-Check".*

| Principle | Assessment |
|---|---|
| **I. Spec-Parity as Source of Truth** | PASS. This feature does not change deacon behavior; it operates the record that measures parity. Pins remain authoritative and human-only (FR-028). |
| **II. Consumer-Only Scope** | PASS. Every new command is dev-only (`deacon-conformance`, `parity-harness`). `deacon --help` gains nothing — extend `parity_registry_check`'s existing assertion to cover the new command groups. |
| **III. Keep the Build Green** | PASS. New hermetic binaries join the fast loop; new live binaries are excluded from it by explicit filter. |
| **IV. No Silent Fallbacks — Fail Fast** | PASS, and this principle *is* the feature. FR-004/FR-006 forbid skip-on-missing-precondition; FR-044 forbids downgrading a gate to a warning. |
| **V. Idiomatic, Safe Rust** | PASS. No new dependencies. Network/process IO is async and bounded; hermetic modules stay sync. Modular boundaries: `lane.rs`, `manifest.rs`, `certification.rs`, `drift/` rather than growing `certify.rs`. |
| **VI. Observability & Output Contracts** | PASS. JSON documents on stdout, logs to stderr; byte-stable artifacts; declaration order preserved via `IndexMap`. |
| **VII. Testing Completeness** | PASS. FR-049–FR-060 mandate the tests; nextest groups declared in **all** profiles per the known 3-spot rule. |
| **VIII. Subcommand Consistency** | PASS. Reuses `check_inventory`, `evaluate_obligations`, `snapshot::compare_staleness`, `oracle::verify_binary`, `corpus_fetch`'s git-subprocess pattern, and `waiver.rs`'s self-invalidating-tolerance pattern rather than re-implementing any of them. |
| **IX. Executable Examples** | N/A. No user-facing surface; `examples/` is untouched. |

**No violations requiring justification.** One complexity item is tracked below (new data roots) with its
rationale.

## Project Structure

### Documentation (this feature)

```text
specs/026-continuous-conformance-certification/
├── plan.md              # This file
├── spec.md              # Feature specification (with Clarifications session)
├── research.md          # Phase 0 output — 10 numbered decisions
├── data-model.md        # Phase 1 output — record shapes + validation classes
├── quickstart.md        # Phase 1 output — operator workflows
├── contracts/
│   ├── cli-lane.md              # `conformance lane <check|report|scaffold>`
│   ├── cli-drift.md             # `conformance drift <check|report|scaffold>` + `drift-scan` bin
│   ├── cli-certification.md     # `conformance certify --report-dir` contract
│   ├── execution-manifest.md    # manifest schema + verification rules
│   └── upgrade-proposal.md      # the seven-section bundle
├── checklists/
│   └── requirements.md  # Spec quality checklist (complete)
└── tasks.md             # Phase 2 output — NOT created by /speckit.plan
```

### Source Code (repository root)

```text
conformance/
├── lanes/
│   └── lanes.json                    # NEW hand-authored: 5 lane records + inclusion allow-lists
├── drift/
│   └── observations.json             # NEW machine-owned: last-observed upstream revisions
├── discovery/
│   └── canary.json                   # NEW hand-authored: canary pins (discovery root, clarification Q5)
└── registry/                         # UNCHANGED — no new registry record kinds

crates/conformance/src/               # hermetic: no network, no Docker, no oracle
├── lane.rs                           # NEW lane model, machine-derived unit denominator, V34
├── manifest.rs                       # NEW execution-manifest model + verification, V35
├── certification.rs                  # NEW release-grade report assembly
├── drift/
│   ├── mod.rs                        # NEW drift observation + proposal records
│   └── check.rs                      # NEW hermetic integrity checks, V36
├── certify.rs                        # EXTENDED: 4 new blocking kinds + --report-dir
├── validate.rs                       # EXTENDED: V34–V36 wired into validate_path_with_inventory
├── discovery/queue.rs                # EXTENDED: canary pin loading + D6
└── bin/conformance.rs                # EXTENDED: `lane`, `drift` command groups

crates/parity-harness/src/            # live: network / Docker / oracle
├── drift/
│   ├── mod.rs                        # NEW
│   ├── scan.rs                       # NEW upstream probes via git + npm subprocesses
│   └── proposal.rs                   # NEW seven-section bundle assembly
├── manifest_emit.rs                  # NEW: writes the execution manifest from driver results
└── bin/
    ├── drift-scan.rs                 # NEW (network; never gates on findings)
    └── oracle-upgrade-propose.rs     # NEW (network + Docker; produces the review bundle)

crates/deacon/tests/
├── conformance_replay.rs             # NEW hermetic lane: 45 oracle-free non-Docker cases
├── conformance_docker_pinned.rs      # NEW pr-docker lane: 39 oracle-free Docker cases + manifest
├── lane_integrity.rs                 # NEW hermetic guard: full unit assignment, profile agreement
├── certification_gates.rs            # NEW: 9 injected positive controls
├── drift_hermetic.rs                 # NEW: observation/proposal integrity
├── discovery_canary.rs               # NEW canary lane (live, non-blocking)
├── discovery_hermetic.rs             # EXTENDED: D6 canary-pin integrity + isolation source scan
└── parity_registry_check.rs          # EXTENDED: new commands absent from `deacon --help`

.config/nextest.toml                  # EXTENDED: [profile.pr-docker], [profile.canary] + exclusions everywhere
.github/workflows/
├── ci.yml                            # EXTENDED: hermetic lane gains the replay binary
├── conformance-docker.yml            # NEW pr-docker lane
├── parity.yml                        # EXTENDED: nightly stable differential identity assertion
├── canary.yml                        # NEW canary lane (non-blocking everywhere)
├── drift.yml                         # NEW drift detection (gates on nothing but its own ability to run)
└── release.yml                       # EXTENDED: docker-execution job → manifest artifact → certify
```

**Structure Decision**: The hermetic/live split established by 019–025 is preserved exactly.
`deacon-conformance` remains incapable of network, Docker, or oracle access, so everything that gates a PR or
a release stays reproducible; `parity-harness` owns every capability that could make a result depend on the
machine it ran on. The one new coupling between them — the execution manifest — deliberately flows *live →
hermetic* as committed-shape data, never the reverse, so the gate never grows a live dependency.

## Phase 0: Research

See [research.md](./research.md) for the ten numbered decisions. The load-bearing ones:

- **D1** — Lane records get their own root, `conformance/lanes/`, not a home inside `registry/`. A lane is
  operational configuration, not a conformance claim; inside the registry it would be reachable by `certify`,
  letting a CI-config edit change a release verdict.
- **D3** — The execution manifest is the hermetic/live seam. Produced by the Docker driver, consumed by
  `certify`, carrying the revision plus per-case case/fixture hashes so a manifest from another revision or a
  stale one cannot be presented as evidence.
- **D5** — Snapshot *staleness* becomes blocking; snapshot *coverage* stays non-blocking except for the
  profile under certification. This threads FR-041(c) through 022's deliberate "a snapshot is a reviewed
  artifact, not a release gate" position without reversing it.
- **D7** — Canary pins live in `conformance/discovery/`, and their integrity is a **D-class (D6)**, not a
  V-class, because D-classes are what police the discovery root. A V-class would put canary state on a path
  that reaches `certify`, contradicting FR-017a.
- **D9** — Split case execution by oracle requirement rather than by area. `oracleType` already encodes
  whether a live reference is needed, so the lane filter is a property of existing data, not a new annotation.

## Phase 1: Design & Contracts

- [data-model.md](./data-model.md) — Lane, Execution Unit, Execution Manifest, Drift Observation, Upgrade
  Proposal, Canary Pin, Certification Report; validation classes V34–V36 and D6.
- [contracts/](./contracts/) — five contract documents covering the two new hermetic command groups, the
  extended `certify` surface, the execution-manifest schema, and the seven-section upgrade bundle.
- [quickstart.md](./quickstart.md) — operator workflows: run a lane locally, read a certification refusal,
  triage a drift signal, prepare an oracle upgrade, and the "add a case, keep the lanes honest" loop.

### Post-Design Re-Check (Constitution)

Re-evaluated after the design above. Still PASS on all nine principles. Two points worth recording:

1. **Principle IV interaction with FR-044.** `certify` must have no escape hatch, but tests need to construct
   failing registries. Resolved the way 019–024 did: tests point `--registry` at fixture trees rather than
   adding a bypass flag. The nine positive controls in `certification_gates.rs` all work this way, so no
   downgrade path exists in the binary.
2. **Principle VII's nextest 3-spot rule** applies to two new live binaries (`conformance_docker_pinned`,
   `discovery_canary`) and four new hermetic ones (`conformance_replay`, `lane_integrity`,
   `certification_gates`, `drift_hermetic`). Each needs its **test-group declaration in every profile** — not
   only a `default-filter` entry — because a binary excluded from a filter still needs its group declared
   wherever it could run. The hermetic ones are *added* to `default`/`dev-fast`; the live ones are *excluded*
   from all six other profiles and allow-listed in exactly one. This is the highest-risk mechanical step in
   the feature, so it gets its own lane-integrity test (FR-059) rather than relying on review.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| Two new data roots (`conformance/lanes/`, `conformance/drift/`) alongside `registry/` and `discovery/` | Lane config and upstream observations must be unreachable from `certify`'s loader, exactly as discovery findings are. Co-location is what makes the isolation structural rather than conventional. | Putting them in `registry/` would let a CI-configuration edit or an upstream observation change a release verdict — the specific failure FR-017a and FR-026 exist to prevent. |
| A fourth artifact type (execution manifest) crossing the hermetic/live boundary | The only way a hermetic certifier can assert that container-backed execution occurred (FR-041(h)). | Making `certify` run Docker would put a container engine in the release path and destroy reproducibility (FR-036, SC-013). Trusting CI job ordering would make the gate unenforceable outside CI. |
| Three new validation classes (V34–V36) plus one discovery class (D6) | Each polices a distinct new record kind; folding them together would produce a violation that cannot name its own cause. | A single "operations integrity" class would report lane, manifest, and drift failures under one code, defeating FR-042's requirement that a failure name its specific offending record. |
