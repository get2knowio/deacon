# Contract: Classification Records + Violation Classes V11–V14

Normative record shape lives in data-model.md §3. This contract fixes the
enforcement semantics added to `validate` / `certify` and the RULES.md lockstep.

## Files

`conformance/registry/classifications/<doc-key>.json` — standard registry collection
envelope (`schemaVersion`, `records` ID-sorted). One file per manifest document key.
Hand-authored; regeneration of the inventory NEVER touches these files.

## Record rules (loader + V13)

1. `id` = `cls-` + exact tail of `constraint`'s `cst-` id. Mismatch → V13.
2. `disposition` ∈ { `behavior-mapped`, `non-testable`, `not-applicable` }. Any other
   value (including the scaffold sentinel `UNREVIEWED`) is a SCHEMA-class load
   failure.
3. `behaviors`: non-empty, all resolving to existing `bhv-` records, iff
   `behavior-mapped` (else must be absent/empty). Violations → V13.
4. `rationale`: required non-empty for `non-testable` and `not-applicable` → V13.

## Join rules (V11 / V12 / V14)

- **V11 (stale)**: `constraint` not present in the committed inventory. Remedy:
  delete or re-point the record in the same change that updated the inventory.
  Waiver-style self-invalidation — stale human records never linger.
- **V12 (unclassified / duplicated)**: an inventory unit with zero classifications
  (this is the *unclassified drift item* — no separate drift record type exists) or
  more than one. Every unit of every kind requires exactly one — including
  `annotation` and `unmodeled-keyword` units (expected dispositions: `non-testable`
  and a conscious human choice, respectively).
- **V14 (provenance)**: manifest fingerprint mismatch; inventory `revision` ≠
  registry schema pin; or committed inventory ≠ in-memory regeneration.

All four classes are reported by `validate` alongside V1–V10 in a single run, and
all four block `certify` (final phase wiring). `not-applicable` / `non-testable`
dispositions are listed in `report` but never block — they are the honest
consumer-only-scope boundary, kept visible per spec FR-014.

## Evidence rule for `behavior-mapped` (research Decision 11)

A classification may only map to behaviors that exist AND are covered under the
existing coverage rules. Creating a new behavior during classification requires a
`case-` record naming real evidence (typically an existing deacon test binary via
`executable.binary`). A consumer-runtime constraint with no locatable or
cheaply-creatable evidence is honestly a **gap** — it blocks, and that is the
intended pressure. Uncovered decorative behaviors and evidence-free waivers are
forbidden.

## RULES.md lockstep

`conformance/RULES.md` gains a "Constraint inventory" section documenting V11–V14,
the disposition arity table, and the drift workflow (re-vendor → regenerate →
V11/V12 enumerate the review queue → classify → green), updated in the same PR that
lands the enforcement (the RULES.md/validate.rs lockstep rule already governing
R1–R8 / V1–V10).

## Migration (FR-022)

`src-schema-features-type` and `src-schema-forwardports-type` are removed from
`conformance/registry/sources/schema.json` in the same change that lands their
replacement `cls-` records (`behavior-mapped` to the same behaviors:
`bhv-readconfig-wrong-type-features-rejected`,
`bhv-readconfig-wrong-type-forwardports-rejected`). Existing behavior records keep
their IDs; only the evidence pointer moves. Post-migration, `sources/schema.json`
holds zero hand-written records (empty collection retained for the loader), and any
future schema-derived evidence enters via constraint units + classifications only.
