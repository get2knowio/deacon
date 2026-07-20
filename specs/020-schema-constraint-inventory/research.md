# Research: Schema Constraint Inventory

All Technical Context unknowns resolved. Empirical inputs gathered on 2026-07-20 from
the pinned upstream revision (`devcontainers/spec` @ `113500f4`, matching the
registry's `rev-schema-113500f4`):

| File | Size | Definitions | External refs | SHA-256 |
|---|---|---|---|---|
| `devContainer.base.schema.json` | 24,142 B | `Mount`, `buildOptions`, `composeContainer`, `devContainerCommon`, `dockerfileContainer`, `imageContainer`, `nonComposeBase` | **none** (9 internal `#/definitions/...` refs) | `a0883c0405ff433db188849d458fb20b9c0d73e0ba1a6e44c1d83f3b485408dd` |
| `devContainerFeature.schema.json` | 12,885 B | `Feature`, `FeatureOption`, `Mount` | **none** (3 internal refs) | `671fcd80cbb3510793412746b35505d12b6a4b8e68d38a01960e440aa5af32eb` |
| `devContainer.schema.json` (composite) | 447 B | — | `./devContainer.base.schema.json` + **two live GitHub URLs** (vscode/codespaces editor schemas) | not vendored (Decision 2) |

Keyword profile (base + feature): `type` ×192, `additionalProperties` ×34,
`properties` ×21, `oneOf` ×7, `allOf` ×4, `anyOf` ×2, `enum` ×12, `default` ×12,
`required` ×11, `patternProperties` ×2, plus top-level `unevaluatedProperties`,
`$schema`, and the VS Code JSONC directives `allowComments`/`allowTrailingCommas` on
the base schema. Estimated constraint-unit count: **400–800**.

## Decision 1 — Vendor the pinned schemas under `conformance/schemas/<rev>/`

**Decision**: Commit byte-exact copies of the two mandatory schemas at
`conformance/schemas/113500f4/`, alongside a `manifest.json` recording, per file:
logical document key (`base`, `feature`), filename, upstream URL at the pinned commit,
and SHA-256. The manifest references `rev-schema-113500f4`, keeping
`conformance/registry/revisions.json` the single pin authority. Extraction verifies
each file's SHA-256 against the manifest before parsing and hard-fails on mismatch
(FR-005).

**Rationale**: FR-002/SC-004 require fully offline PR/release testing; vendoring is
the only arrangement where CI never fetches. Fingerprints make "vendored copy ≠
claimed upstream revision" a detectable, blocking error instead of silent drift.
Network is used exactly once, by the human vendoring a revision (documented in
quickstart.md).

**Alternatives considered**: (a) git submodule of `devcontainers/spec` — pulls in the
whole spec repo for two files, submodule init breaks "clone and test offline";
(b) fetch-and-cache at test time — violates the no-network constraint outright;
(c) storing schemas inside `conformance/registry/` — rejected to keep the hand-edited
registry directory free of vendored third-party artifacts.

## Decision 2 — The editor-composite schema is NOT vendored in this feature

**Decision**: `devContainer.schema.json` (the 447-byte `allOf` composite) is excluded
from the mandatory surface. Its two live-URL editor-schema refs are precisely the
"do not silently follow live URLs" hazard; the machinery still supports it later via
Decision 5's rule that a relative ref may resolve **only** to another explicitly
pinned document. Attempting to inventory a document whose refs point at unpinned
locations fails with `UnresolvedExternalRef` naming the URL (User Story 4, P4).

**Rationale**: The user input makes editor-composite schemas optional ("may be
included only when pinned explicitly"). The composite adds zero constraints of its
own beyond composition edges; its value would only come from pinning the two
Microsoft editor schemas, which have no upstream revision discipline (they live on
`main`) and are editor-only surface (would be classified `not-applicable` wholesale).

## Decision 3 — Definition-site attribution; `$ref` is an edge, never inlined

**Decision**: The extraction walk visits every schema object **exactly once, at the
JSON Pointer where it is defined**. Constraints are attributed to their definition
site. A `$ref` emits a single `reference`-kind constraint unit (an edge from usage
pointer to target pointer); the target's own constraints are extracted at the
target's pointer, not duplicated per usage.

**Rationale**: This single rule answers three spec edge cases at once, deterministically:
- *Recursion*: a self-referential but productive schema is inventoried finitely
  (each pointer visited once) — no expansion, no blowup.
- *Multi-path composition*: a definition referenced from several `allOf` branches
  yields one set of units plus one `reference` unit per usage — the "chosen once and
  applied deterministically" answer required by the spec's edge-case list.
- *Provenance*: every unit's pointer is a real location in the pinned document, so
  SC-006 traceability is trivial.

**Alternatives considered**: full ref-inlining/flattening (constraints re-attributed
to every usage site) — rejected: exponential duplication potential, ambiguous
provenance, and unstable IDs when a shared definition gains a new usage.

## Decision 4 — Constraint-kind taxonomy with a fail-faithful catch-all

**Decision**: Each schema object's keywords decompose into typed facets, one unit per
facet: `property-existence`, `required`, `type` (with a `nullable` flag when `"null"`
is a type member), `enum`, `const`, `default`, `union-alternative` (one per
`oneOf`/`anyOf` arm, recording arm index), `all-of` (composition edge), `conditional`
(`if`/`then`/`else`, preserving the condition pointer as context),
`additional-properties` (tri-state: `false` / schema / absent-is-open, plus
`unevaluatedProperties` and `patternProperties` recorded under this family),
`array-shape` (`items`, `minItems`, `maxItems`, `uniqueItems`), `value-shape`
(string/number assertions: `pattern`, `minLength`, `minimum`, …), `reference`
(Decision 3), `annotation` (`title`, `description`, `examples`, `markdownDescription`,
`$schema`, JSONC directives — carriers of no testable behavior), and
**`unmodeled-keyword`** — any keyword the extractor does not recognize becomes its own
unit carrying the keyword name and raw value.

**Rationale**: The typed kinds cover every category the user input enumerates. The
`unmodeled-keyword` catch-all applies constitution IV's "faithful on the unmodeled"
side to the extractor itself: a future upstream keyword (or a draft keyword we chose
not to model) can never silently vanish — it lands in the inventory and, being
unclassified, blocks until a human disposes of it. This is the same fail-loud posture
as the registry's gap semantics.

**Alternatives considered**: erroring on unknown keywords — rejected: upstream adding
an annotation keyword would break generation needlessly; the catch-all keeps
generation total while still forcing human review.

## Decision 5 — Reference resolution and cycle policy

**Decision**: Only fragment refs into a pinned document resolve: `#/...` within the
same document, or (future, Decision 2) a relative path that names another manifest
entry plus an optional fragment. Resolution failures are typed, blocking errors:
`UnresolvedRef` (target pointer absent), `UnresolvedExternalRef` (URL or path outside
the pinned set), `MalformedRef` (unparseable pointer). Cycle policy: since refs are
never inlined, only **pure `$ref` chains** (`a → b → … → a` where every node is a
ref-only schema) are unproductive; the resolver follows chains with a visited stack
and reports `RefCycle` listing the full chain. Schemas that recurse through structural
keywords (productive recursion) are fine per Decision 3.

**Rationale**: Matches the spec's demand for explicit cycle/malformed/unresolved
errors with cause-specific messages (FR-009), with the minimal cycle definition that
is actually unproductive under definition-site attribution. Both pinned schemas are
verified all-internal today, so these paths are exercised by fixtures (as the spec's
acceptance tests require) rather than by the baseline.

## Decision 6 — Stable ID scheme: readable slug + substance hash

**Decision**: Constraint unit IDs use a new `cst` prefix in the registry ID grammar:
`cst-<doc>-<slug>-<kind>-<hash8>`, where `<doc>` is the manifest document key
(`base`, `feature`), `<slug>` is a bounded slugification of the trailing JSON-Pointer
segments (lowercased, non-alphanumerics collapsed to `-`, truncated to keep IDs
readable), `<kind>` is a short kind code, and `<hash8>` is the first 8 hex chars of
SHA-256 over `(document key, full JSON Pointer, kind, canonical substance)`. The slug
is for human readability only; identity is carried by the hash inputs. Collisions
(astronomically unlikely at 8 hex chars over ≤ ~1k units) are detected at generation
and fail loudly; the remedy documented in the contract is widening that unit's hash,
never silent renumbering.

**Rationale**: Satisfies both halves of the spec's identity rule: *stable* (same
substance + location ⇒ same ID across regenerations, so hand-authored classifications
attach durably) and *drift-forcing* (material change ⇒ new hash ⇒ new ID ⇒
unclassified drift item; disposition inheritance by name similarity is structurally
impossible). Hash inputs exclude annotations, so description-wording churn does not
move IDs (SC-005's immaterial-change requirement).

**Alternatives considered**: (a) pointer-only IDs (no substance hash) — rejected: a
materially changed constraint would keep its ID and silently inherit its disposition,
violating FR-017; (b) opaque full-hash IDs — rejected: unreviewable classification
files; (c) sequential numbering — rejected: order-dependent, unstable under upstream
insertions.

## Decision 7 — Committed inventory + hermetic byte-identical regeneration check

**Decision**: `conformance inventory generate` writes
`conformance/inventory/constraints.json`: canonical JSON (sorted object keys, units
sorted by ID, 2-space indent, LF line endings, trailing newline, no timestamps or
absolute paths — the exact discipline `report.rs` already follows). A hermetic test
(`inventory_determinism.rs`) regenerates from the vendored schemas into a temp dir
and asserts byte equality with the committed artifact; `inventory check` exposes the
same comparison on the CLI. Substance values preserve upstream JSON semantics
(enum/array order preserved; object keys sorted only where JSON objects are
order-insensitive).

**Rationale**: Implements clarified FR-019 and SC-002. Reuses the crate's proven
determinism pattern rather than inventing a second serialization discipline.
Cross-platform byte-identity is guaranteed by construction: no environment input
enters the artifact.

## Decision 8 — Classifications: hand-authored `cls` records, joined by ID (V11–V14)

**Decision**: New record type `Classification` (prefix `cls`) in
`conformance/registry/classifications/{base,feature}.json` (one file per document
key, envelope + ID-sorted like every other collection). Shape:
`{ id: "cls-<same tail as the constraint id>", constraint: "cst-…", disposition:
"behavior-mapped" | "non-testable" | "not-applicable", behaviors: [ "bhv-…" ]
(required non-empty iff behavior-mapped, else forbidden), rationale (required for
non-testable / not-applicable), notes? }`. Four new violation classes join inventory
and registry at validation time:
- **V11** — stale: classification references a constraint ID absent from the
  committed inventory (must be deleted/re-pointed in the same change; waiver-style
  self-invalidation).
- **V12** — unclassified: inventory unit with no classification (this IS the
  "unclassified drift item" — drift needs no separate record type), or with more
  than one.
- **V13** — bad mapping: `behavior-mapped` references a nonexistent behavior, or the
  behaviors/rationale arity rules are violated.
- **V14** — provenance mismatch: manifest fingerprint check fails, the inventory's
  recorded revision differs from the registry's schema pin, or the committed
  inventory fails the byte-identity check during validation.

**Rationale**: Implements clarified FR-020/FR-021 with the registry's existing
enforcement vocabulary (violation classes, blocking `validate`). Making "drift item"
= "unclassified unit" avoids a parallel drift bookkeeping system: a revision bump
regenerates the inventory, changed substance produces new IDs, and V11+V12 mechanically
enumerate exactly the review workload. A `scaffold` subcommand emits skeleton `cls`
records (disposition intentionally invalid-until-edited) to make classifying ~400–800
units tractable without ever auto-deciding a disposition.

**Alternatives considered**: storing dispositions on generated units — rejected:
regeneration would rewrite human decisions (violates clarification Q1); a separate
drift-item record type — rejected: duplicates what V12 already expresses.

## Decision 9 — Diff semantics: keyed by (document, pointer, kind); substance decides "changed"

**Decision**: `conformance inventory diff <old> <new>` compares two inventory files.
Units are matched on `(document key, JSON Pointer, kind)`: present-right-only →
**added**; present-left-only → **removed**; matched but different canonical substance
→ **materially changed** (old + new substance shown). `annotation`-kind differences
are reported under a separate non-material section. Output is deterministic (sorted
by the match key) in JSON and Markdown forms, mirroring `report`. A
moved-but-identical constraint is therefore removed + added — the documented
Assumption from the spec, honored exactly.

**Rationale**: Implements FR-016 with a match key that is strictly coarser than the
ID (which also hashes substance), so "same place, new substance" reads as *changed*
rather than as an unrelated add/remove pair — the most reviewable presentation of a
revision bump — while staying fully deterministic.

## Decision 10 — Rollout inside the feature: certify wiring is the last phase

**Decision**: Work sequences as: (1) vendoring + extraction + committed inventory +
diff, with validation of the *inventory itself* (V14) active; (2) classification
records land for 100% of units, with V11–V13 active in `validate`; (3) only then does
`certify` gain the "unclassified/stale constraint blocks certification" rule
(FR-015/FR-018), plus report sections and the retirement of the two superseded
hand-written schema source units (FR-022) — their `behaviors` links move onto `cls`
records mapping the corresponding generated `cst` units (`src-schema-features-type` →
the `#/definitions/devContainerCommon/properties/features` additional-properties/type
units; `src-schema-forwardports-type` → the `forwardPorts` array-shape/type units).
The feature merges as one CI-gated PR in which all three phases are complete, so the
gate on main is never red and never weakened (SC-008).

**Rationale**: Implements clarification Q3/Q5. Ordering inside the branch keeps every
intermediate commit's `validate`/`certify` green for bisectability, and the
single-PR merge preserves the repo's established speckit delivery shape (019 merged
as one PR).

## Decision 11 — Behavior-mapping evidence rule (no evidence-free behaviors)

**Decision**: During classification (tasks T025/T026), a consumer-runtime constraint
is disposed as follows, in order:
1. **An existing behavior covers it** → `behavior-mapped` to that behavior. Prefer
   extending an existing behavior's statement over minting near-duplicates.
2. **No behavior exists, but deacon evidence does** (an existing hermetic/integration
   test exercises the constraint — common: deacon's strict config parser is heavily
   tested) → create the behavior with honest three-axis values AND a `case-` record
   whose `executable.binary` names that existing test binary (the registry's case
   model already supports non-parity binaries, e.g. `case-auto-forward` →
   `integration_auto_forward`), then `behavior-mapped`.
3. **No evidence exists at all** → either write the small hermetic test in-feature
   (bounded: config-parse/shape tests, not new Docker suites) and proceed per (2), or
   the honest state is a **gap — which blocks certify and therefore blocks the
   feature's merge** until resolved. Creating an *uncovered* behavior or a decorative
   waiver to dodge this is forbidden (the registry's core anti-pattern).

**Rationale**: Without this rule, T025/T026 would either explode in scope (a covered
behavior for every schema constraint) or dead-end (unclassified and gap both block,
by design). The rule keeps SC-008 achievable honestly: evidence is *located or
created*, never asserted. It also preserves R-rule integrity — statuses stay
evidence-backed claims.

**Alternatives considered**: a fourth "consumer-runtime, evidence pending"
disposition — rejected: it is a gap with a euphemism, exactly the "different but
acceptable" state the three-axis model exists to eliminate.

## Resolved unknowns summary

No NEEDS CLARIFICATION markers remain. The one item deferred from `/speckit.clarify`
(granularity mechanics and ID derivation) is resolved by Decisions 3, 4, and 6.
No deferrals are created by this plan; there is no "## Deferred Work" carried into
tasks.md from research.
