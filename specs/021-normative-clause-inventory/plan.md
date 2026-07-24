# Implementation Plan: Normative Clause Inventory

**Branch**: `021-normative-clause-inventory` | **Date**: 2026-07-24 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/021-normative-clause-inventory/spec.md`

## Summary

Extend the dev-only `deacon-conformance` crate with a **prose-clause inventory** —
the companion to feature 020's schema-constraint inventory — covering the ratified
`docs/specs/` documents of the containers.dev specification pinned at `rev-spec-113500f4`.
The ratified prose documents are **vendored** under `conformance/spec/113500f4/` with
SHA-256 fingerprints and a `consumer` / `authoring` scope marker per document. The
committed artifact `conformance/inventory/clauses.json` is a **human-reviewed** list of
atomic clause records (an LLM-assisted proposal step MAY draft them, but is never invoked
by CI). A new `clause generate` command **canonicalizes** that committed list against the
vendored prose — recomputing each clause's substance-anchored stable ID and
normalized-substance fingerprint, verifying the excerpt is present in the pinned document
under its recorded heading, sorting canonically — and a hermetic test regenerates it and
requires byte-identical output. Hand-authored clause-classification records
(`conformance/registry/clause-classifications/<doc>.json`) join to clauses by stable ID —
per-clause for consumer documents, with a document-scope not-applicable default permitted
only for authoring documents. The registry's violation classes V11–V14 are **generalized
from "constraint unit" to "inventory unit (constraint OR clause)"**, plus a new **V15**
for clause↔source integrity (strength-label ↔ keyword agreement; excerpt-present-at-anchor);
`certify` blocks on any unclassified or stale clause — wired only in the final phase, after
the full initial classification lands. A deterministic `clause diff` matches on the
substance fingerprint so **moves are first-class** (distinct from new/removed/changed),
which is how upstream prose drift surfaces as unclassified (blocking) work.

## Technical Context

**Language/Version**: Rust, Edition 2024, MSRV 1.95 (`unsafe_code = "deny"` workspace-wide)
**Primary Dependencies**: existing crate deps only — `serde`/`serde_json` (record
parsing + canonical serialization; `preserve_order` already on), `indexmap` (ordered
collections), `clap` (new `clause` subcommands on the existing `conformance` bin),
`sha2` (fingerprints + stable-ID hashes; already a `deacon-conformance` dep), `thiserror`
(domain errors), `tracing`. A small deterministic Markdown-heading/section reader is
written in-crate (ATX headings only) rather than adding a Markdown crate — the parse
surface needed (heading tree + section text spans + code-fence awareness) is small,
must be byte-deterministic across platforms, and a new dependency would be
disproportionate. **No new external crates.**
**Storage**: strict-JSON + vendored Markdown — vendored prose under
`conformance/spec/<rev>/` (with `manifest.json` fingerprints + per-document `scope`),
generated committed inventory at `conformance/inventory/clauses.json` (sibling of 020's
`constraints.json`), hand-authored clause-classification records under
`conformance/registry/clause-classifications/`. All version-controlled; no network at
generate/check/validate/diff/certify time, and no LLM ever in those paths.
**Testing**: `cargo nextest` via the existing hermetic conformance test-binary pattern
(`crates/conformance/tests/`); fixture prose under `fixtures/conformance/prose/` plus
assertions against the vendored pinned baseline. No Docker, no network, no model.
**Target Platform**: contributor tooling on Linux/macOS/Windows (must stay green in the
`dev-fast` Windows CI lane; byte-identical output across platforms — LF endings, no
path-dependent content; Markdown read as bytes with explicit `\n` handling).
**Project Type**: extension of the existing dev-only `deacon-conformance` library + CLI
crate (`publish = false`; NOT part of the `deacon` consumer CLI).
**Performance Goals**: not a driver — canonicalizing/verifying a few thousand clauses over
the eighteen vendored Markdown documents must simply feel instant (< 1 s); determinism
matters, speed does not.
**Constraints**: byte-identical regeneration across runs and platforms; offline-only and
LLM-free in every CI-facing command; fail-loud on fingerprint mismatch, missing excerpt at
anchor, strength/keyword contradiction, and stale/unclassified clauses (no partial or
silently-strict inventories); regeneration must never mutate hand-authored classification
records; `certify` on main never goes red during rollout (gate wired last).
**Scale/Scope**: 18 pinned prose documents (14 consumer, 4 authoring — the full ratified
`docs/specs/` set at `113500f4`) → an estimated ~1,500–3,500 clause units; one new module
family (`prose/`, `clause`, `clause_diff`) in one crate; two new record prefixes (`clu`,
`clc`); V11–V14 generalized + one new class (V15).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| # | Principle | Assessment |
|---|-----------|------------|
| I | Spec-Parity as Source of Truth | **Pass.** The feature *strengthens* spec-parity: it turns the pinned prose (part of the upstream spec repo at `113500f4`) into an enumerated, evidence-tracked surface. No deacon runtime behavior changes. |
| II | Consumer-Only Scope | **Pass.** Everything lands in the dev-only `deacon-conformance` crate; the consumer CLI is untouched. Authoring-document clauses are *inventoried* but classified `not-applicable` (per-document default) — inventorying them is bookkeeping, not authoring tooling. |
| III | Keep the Build Green | **Pass.** All new tests are hermetic and register in `.config/nextest.toml` following the existing conformance test binaries. The byte-identical regeneration check is itself a green-build gate. Gate wiring is sequenced last so `certify` never goes red on main (SC-008). |
| IV | No Silent Fallbacks — Fail Fast | **Pass — this feature is an application of the principle.** Fingerprint mismatch, missing excerpt at anchor, strength/keyword contradiction, unresolved ambiguity, stale/unclassified clauses → cause-specific hard errors / blocking violations; no partial inventory and no silently-strict promotion of ambiguous language (FR-014). |
| V | Idiomatic, Safe Rust | **Pass.** `thiserror` domain errors in the library, `anyhow` only at the bin boundary; no `unwrap` in runtime paths; pure synchronous file IO in a dev tool (no async); new focused modules (`prose/{mod,normalize,strength}`, `clause`, `clause_diff`) rather than growing `validate.rs`. |
| VI | Observability & Output Contracts | **Pass.** `clause` subcommands follow the existing `conformance` bin contract: results to stdout/files, diagnostics via `tracing` to stderr; deterministic byte-stable artifacts (no timestamps/absolute paths), reusing `inventory.rs`'s `render`/atomic-write discipline. |
| VII | Testing Completeness | **Pass.** Spec FR-027/FR-028 enumerate the mandatory acceptance tests (stable identities, multi-requirement splitting, strength detection, moved headings, changed text, ambiguous clauses, authoring-scope exclusions, determinism, traceability) — all planned as fixture-driven hermetic tests plus pinned-baseline assertions. |
| VIII | Subcommand Consistency & Shared Abstractions | **Pass.** Reuses the crate's loader/validation/report/certify plumbing, the ID grammar (`parse_id` extended with two prefixes), the `join_inventory`/`InventoryJoin` engine, the `write_inventory` atomic pattern, `render`/`canonicalize`, and the `inventory_paths_for` sibling-resolution convention. No duplicated bespoke loaders. |
| IX | Executable & Self-Verifying Examples | **N/A.** Dev-only tooling; `examples/` is consumer-facing and untouched. |

**Post-Phase-1 re-check**: the design artifacts below introduce no new violations. The two
new record prefixes (`clu`, `clc`) extend the existing closed ID grammar rather than
inventing a parallel identity scheme; V11–V14 are *generalized* (not duplicated) and V15 is
the one genuinely new class (prose has a source-text-integrity dimension schema constraints
lack). The one spec refinement (identity is substance-anchored, not location-based — research
Decision 2) is reflected back into the spec Assumptions in lockstep. No Complexity Tracking
entries needed.

## Project Structure

### Documentation (this feature)

```text
specs/021-normative-clause-inventory/
├── plan.md              # This file
├── research.md          # Phase 0 output (decisions 1–11)
├── data-model.md        # Phase 1 output (entities, ID scheme, violation classes)
├── quickstart.md        # Phase 1 output (developer walkthrough)
├── contracts/
│   ├── clause-inventory-schema.md        # clauses.json + spec manifest contract
│   ├── clause-classification-schema.md   # clause-classification record + V11–V15
│   └── cli-clause.md                     # `conformance clause …` CLI contract
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
conformance/
├── spec/
│   └── 113500f4/
│       ├── manifest.json                     # NEW: doc key + file + upstreamUrl + sha256 + scope + revision id
│       ├── *.md                              # NEW: all 18 ratified docs/specs Markdown files vendored byte-exact @113500f4
│       │                                     #   14 consumer (reference, devcontainerjson-reference, supporting-tools,
│       │                                     #   image-metadata, devcontainer-lockfile, devcontainer-id-variable,
│       │                                     #   parallel-lifecycle-script-execution, features-contribute-lifecycle-scripts,
│       │                                     #   features-user-env-variables, feature-dependencies, gpu-host-requirement,
│       │                                     #   declarative-secrets, secrets-support, features-legacyIds-deprecated-properties)
│       │                                     #   4 authoring (devcontainer-features{,-distribution}, devcontainer-templates{,-distribution})
├── inventory/
│   ├── constraints.json                      # EXISTING (020) — untouched
│   └── clauses.json                          # NEW: generated (canonicalized), committed, byte-stable
└── registry/
    ├── clause-classifications/               # NEW: hand-authored disposition records
    │   ├── <consumer-doc-key>.json            #   one per-clause file per consumer document (14 files)
    │   └── authoring.json                     #   document-scope not-applicable defaults for the 4 authoring docs
    └── sources/spec.json                     # MODIFIED (final phase): hand-written prose units superseded (FR-026)

crates/conformance/
├── src/
│   ├── lib.rs                                # MODIFIED: new module decls + CURRENT_SPEC_PIN + clause paths
│   ├── model.rs                              # MODIFIED: RecordType + Clu/Clc prefixes; ClauseInventory, ClauseUnit,
│   │                                         #   Strength, Testability, ClauseLocation, ClauseClassification, SpecManifest
│   ├── load.rs                               # MODIFIED: load clause classifications + clauses.json + spec manifest
│   ├── validate.rs                           # MODIFIED: V11–V14 generalized to inventory units; V15 clause↔source
│   ├── certify.rs                            # MODIFIED (last phase): block on unclassified/stale clauses
│   ├── report.rs                             # MODIFIED: clause inventory coverage section in report.{json,md}
│   ├── prose/
│   │   ├── mod.rs                            # NEW: ATX heading tree + section text spans + code-fence awareness
│   │   ├── normalize.rs                      # NEW: normalize_substance() + fingerprint (Decision 3)
│   │   └── strength.rs                       # NEW: detect_strength() RFC-2119 keyword map (Decision 4)
│   ├── clause.rs                             # NEW: clause unit model, substance-anchored IDs, canonicalize/write/compare
│   └── clause_diff.rs                        # NEW: fingerprint-keyed diff (new/removed/moved/changed) (Decision 9)
├── src/bin/conformance.rs                    # MODIFIED: `clause generate|check|diff|scaffold`; clause_paths_for()
└── tests/
    ├── clause_extraction.rs                  # NEW: fixture-driven segmentation/strength/ambiguity acceptance tests
    ├── clause_determinism.rs                 # NEW: byte-identical regeneration (pinned + fixtures)
    ├── clause_baseline.rs                    # NEW: pinned-baseline assertions (FR-028)
    ├── clause_diff.rs                        # NEW: new/removed/moved/changed + immaterial acceptance tests
    └── clause_classification_join.rs         # NEW: V11–V15 join validation tests

fixtures/conformance/
├── prose/                                    # NEW: fixture Markdown (multi-req paragraphs, moved headings, ambiguity, authoring)
└── clause-drift/                             # NEW: old/new fixture inventories for diff tests

.config/nextest.toml                          # MODIFIED: group overrides for the five new test binaries (all fast lanes)
conformance/RULES.md                          # MODIFIED: V11–V14 generalized wording + V15; inventory section
```

**Structure Decision**: extend the existing `deacon-conformance` crate in place — the
registry loader, ID grammar, `join_inventory`/`InventoryJoin` engine, violation-class
driver (`check_inventory`), `certify` gate, and deterministic `render`/atomic
`write_inventory` it already owns are exactly the substrate FR-023/FR-024/FR-026 require.
New logic is split into focused modules (`prose/{mod,normalize,strength}`, `clause`,
`clause_diff`) per constitution V rather than growing `validate.rs`. Generated data
(`conformance/inventory/clauses.json`) sits beside 020's `constraints.json`; vendored prose
(`conformance/spec/`) is a sibling of vendored schemas (`conformance/schemas/`), keeping the
hand-edited registry directory free of vendored third-party artifacts. Clause classifications
get their own `clause-classifications/` directory (not mixed with 020's `classifications/`)
because their record shape adds the document-scope variant.

## Complexity Tracking

No constitution violations to justify — table intentionally empty.
