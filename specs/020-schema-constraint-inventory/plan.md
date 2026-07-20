# Implementation Plan: Schema Constraint Inventory

**Branch**: `020-schema-constraint-inventory` | **Date**: 2026-07-20 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/020-schema-constraint-inventory/spec.md`

## Summary

Extend the dev-only `deacon-conformance` crate to reproducibly extract a
constraint-level inventory from the containers.dev JSON schemas pinned by the
conformance registry (`rev-schema-113500f4`). The two mandatory schemas
(`devContainer.base.schema.json`, `devContainerFeature.schema.json`) are **vendored**
under `conformance/schemas/113500f4/` with recorded SHA-256 fingerprints; a new
`inventory` module walks each schema document deterministically (definition-site
attribution, no ref inlining), emits atomic constraint units with stable
substance-and-location-derived IDs into a **committed** artifact
(`conformance/inventory/constraints.json`), and a hermetic test regenerates it and
requires byte-identical output. Hand-authored **classification records** in the
registry (`conformance/registry/classifications/`) join to constraint units by stable
ID; new validation classes make "every unit classified exactly once, nothing stale"
machine-enforced, and `certify` blocks on any unclassified or stale unit — wired only
in the final phase, after the full initial classification lands. A deterministic
`inventory diff` compares two inventory files and reports added / removed / materially
changed constraints, which is how upstream revision drift surfaces as unclassified
(blocking) work.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing workspace deps only — `serde`/`serde_json` (schema
parsing + inventory serialization), `indexmap` (declaration-ordered walks), `clap`
(new `inventory` subcommands on the existing `conformance` bin), `thiserror` (domain
errors), `tracing`; plus `sha2` (already a workspace dep, used by `deacon`/`deacon-core`)
for content fingerprints and stable-ID hashes. No new external crates.
**Storage**: strict-JSON files — vendored schemas under `conformance/schemas/<rev>/`
(with `manifest.json` fingerprints), generated committed inventory at
`conformance/inventory/constraints.json`, hand-authored classification records under
`conformance/registry/classifications/`. All version-controlled; no network at
generation, validation, or certification time.
**Testing**: `cargo nextest` via the existing hermetic conformance test binaries
pattern (`crates/conformance/tests/`); fixture schemas under `fixtures/conformance/`
plus assertions against the vendored pinned baseline. No Docker, no network.
**Target Platform**: contributor tooling on Linux/macOS/Windows (must stay green in
the `dev-fast` Windows CI lane; byte-identical output across platforms — LF endings,
no path-dependent content)
**Project Type**: extension of the existing dev-only `deacon-conformance` library +
CLI crate (`publish = false`; NOT part of the `deacon` consumer CLI)
**Performance Goals**: not a driver — inventory generation over the two pinned schemas
(≈37 KB total) must simply feel instant (< 1 s); determinism matters, speed does not.
**Constraints**: byte-identical regeneration across runs and platforms; offline-only
in CI; fail-loud on cycles/unresolved refs/malformed schemas (no partial inventories);
regeneration must never mutate hand-authored records; `certify` on main never goes red
during rollout (gate wired last).
**Scale/Scope**: 2 pinned schema documents (~24 KB + ~13 KB, 7 + 3 definitions,
~120 + ~72 `type` keywords) → an estimated 400–800 constraint units to extract and
classify; single new module family in one crate; ~4 new violation classes.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Assessment |
|---|-----------|------------|
| I | Spec-Parity as Source of Truth | **Pass.** The feature *strengthens* spec-parity: it turns the pinned schemas (part of the upstream spec repo at `113500f4`) into an enumerated, evidence-tracked surface. No deacon runtime behavior changes. |
| II | Consumer-Only Scope | **Pass.** Everything lands in the dev-only `deacon-conformance` crate; the consumer CLI is untouched. Feature-authoring/template-authoring/editor-only schema constraints are *inventoried* but explicitly classified `not-applicable` — inventorying them is bookkeeping, not authoring tooling. |
| III | Keep the Build Green | **Pass.** All new tests are hermetic and register in `.config/nextest.toml` following the existing conformance test binaries. The byte-identical regeneration check is itself a green-build gate. Gate wiring is sequenced last so `certify` never goes red on main (SC-008). |
| IV | No Silent Fallbacks — Fail Fast | **Pass — this feature is an application of the principle.** Cycles, unresolved refs, malformed schemas, fingerprint mismatches → cause-specific hard errors; no partial inventories. Unmodeled schema keywords are captured as `unmodeled-keyword` units rather than dropped (faithful-on-the-unmodeled). |
| V | Idiomatic, Safe Rust | **Pass.** `thiserror` domain errors in the library, `anyhow` only at the bin boundary; no `unwrap` in runtime paths; pure synchronous file IO in a dev tool (no async needed); new focused modules (`schema`, `inventory`, `diff`) rather than growing `validate.rs` monolithically. |
| VI | Observability & Output Contracts | **Pass.** `inventory` subcommands follow the existing `conformance` bin contract: results to stdout/files, diagnostics via `tracing` to stderr; deterministic byte-stable artifacts (no timestamps/absolute paths), mirroring `report`. |
| VII | Testing Completeness | **Pass.** Spec FR-023/FR-024 enumerate the mandatory acceptance tests (composition, unions, null, required, additionalProperties, cycles, malformed, stable IDs, determinism, drift) — all planned as fixture-driven hermetic tests plus pinned-baseline assertions. |
| VIII | Subcommand Consistency & Shared Abstractions | **Pass.** Reuses the crate's existing loader/validation/report/certify plumbing and the registry ID grammar (`parse_id`) extended with two new prefixes; no duplicated bespoke loaders. |
| IX | Executable & Self-Verifying Examples | **N/A.** Dev-only tooling; `examples/` is consumer-facing and untouched. |

**Post-Phase-1 re-check**: design artifacts below introduce no new violations; the two
new record prefixes (`cst`, `cls`) extend the existing closed ID grammar rather than
inventing a parallel identity scheme. No Complexity Tracking entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/020-schema-constraint-inventory/
├── plan.md              # This file
├── research.md          # Phase 0 output (decisions 1–10)
├── data-model.md        # Phase 1 output (entities, ID scheme, violation classes)
├── quickstart.md        # Phase 1 output (developer walkthrough)
├── contracts/
│   ├── inventory-schema.md       # constraints.json + schemas manifest contract
│   ├── classification-schema.md  # classification record contract + violation classes
│   └── cli-inventory.md          # `conformance inventory …` CLI contract
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
conformance/
├── schemas/
│   └── 113500f4/
│       ├── manifest.json                     # NEW: file list + sha256 + upstream URLs + revision id
│       ├── devContainer.base.schema.json     # NEW: vendored, byte-exact from upstream @113500f4
│       └── devContainerFeature.schema.json   # NEW: vendored, byte-exact from upstream @113500f4
├── inventory/
│   └── constraints.json                      # NEW: generated, committed, byte-stable
└── registry/
    ├── classifications/                      # NEW: hand-authored disposition records
    │   ├── base.json                         #   one file per pinned schema document
    │   └── feature.json
    └── sources/schema.json                   # MODIFIED: two hand-written units retired (FR-022)

crates/conformance/
├── Cargo.toml                                # MODIFIED: + sha2 (workspace)
├── src/
│   ├── lib.rs                                # MODIFIED: new module decls + inventory paths
│   ├── model.rs                              # MODIFIED: RecordType + Cst/Cls prefixes; Classification record
│   ├── load.rs                               # MODIFIED: load classifications + inventory + manifest
│   ├── validate.rs                           # MODIFIED: V11–V14 join checks
│   ├── certify.rs                            # MODIFIED (last phase): block on unclassified/stale
│   ├── report.rs                             # MODIFIED: inventory coverage section in report.{json,md}
│   ├── schema/
│   │   ├── mod.rs                            # NEW: schema document model + JSON Pointer helpers
│   │   ├── resolve.rs                        # NEW: internal $ref resolution, cycle detection
│   │   └── extract.rs                        # NEW: keyword→constraint-unit extraction walk
│   ├── inventory.rs                          # NEW: unit model, stable IDs, canonical serialization
│   └── diff.rs                               # NEW: revision diff (added/removed/changed)
├── src/bin/conformance.rs                    # MODIFIED: `inventory generate|check|diff|scaffold`
└── tests/
    ├── inventory_extraction.rs               # NEW: fixture-driven extraction acceptance tests
    ├── inventory_determinism.rs              # NEW: byte-identical regeneration (pinned + fixtures)
    ├── inventory_baseline.rs                 # NEW: pinned-baseline assertions (FR-024)
    ├── inventory_diff.rs                     # NEW: drift/diff acceptance tests
    └── classification_join.rs                # NEW: V11–V14 join validation tests

fixtures/conformance/
├── schemas/                                  # NEW: fixture schemas (composition, cycles, malformed, …)
└── inventory-drift/                          # NEW: old/new fixture inventories for diff tests

.config/nextest.toml                          # MODIFIED: group overrides for the new test binaries
```

**Structure Decision**: extend the existing `deacon-conformance` crate in place — the
registry loader, ID grammar, violation-class engine, and deterministic report writer
it already owns are exactly the substrate FR-019–FR-022 require. New logic is split
into focused modules (`schema/{mod,resolve,extract}`, `inventory`, `diff`) per
constitution V rather than growing `validate.rs`. Generated data (`conformance/inventory/`)
is deliberately a **sibling of, not inside,** `conformance/registry/` so the
hand-edited registry and the machine-generated artifact never share a directory.

## Complexity Tracking

No constitution violations to justify — table intentionally empty.
