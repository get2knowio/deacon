# Contract: Clause Classification + Violation Classes V11–V15

Hand-authored registry records under
`conformance/registry/clause-classifications/<doc>.json`, one collection file per document
key (consumer docs) plus an `authoring.json` holding the document-scope defaults. Strict
JSON, `deny_unknown_fields`, `camelCase`, ID-sorted like every registry collection.
`clause generate` NEVER touches these files.

## Record — `ClauseClassification`

Closed model; exactly one of `clause` / `document` present.

```json
// Per-clause (required for every consumer-doc clause and every `ambiguous` clause)
{
  "id": "clc-reference-oncreatecommand-run-once-must-a1b2c3d4",
  "clause": "clu-reference-oncreatecommand-run-once-must-a1b2c3d4",
  "disposition": "behavior-mapped",
  "behaviors": ["bhv-up-lifecycle-oncreate-once"],
  "rationale": null,
  "notes": null
}

// Document-scope default (permitted ONLY for `authoring`-scope documents)
{
  "id": "clc-doc-features",
  "document": "features",
  "disposition": "not-applicable",
  "rationale": "Feature-authoring document; deacon consumes published features but never authors or validates them (constitution II)."
}
```

| Field | Rules |
|---|---|
| `id` | Per-clause: `clc-` + the exact tail of the `clu-` id (structural mirror; V13 if broken). Document-scope: `clc-doc-<key>`. |
| `clause` XOR `document` | Exactly one present (V13). `clause` MUST exist in the committed inventory (V11 if stale). `document` MUST be an `authoring`-scope manifest key (V13 / `DocumentScopeOnConsumerDoc`). |
| `disposition` | `behavior-mapped` \| `non-testable` \| `not-applicable` (reuses 020's `Disposition`; `non-testable` is prose "informative"). |
| `behaviors` | Non-empty, all existing behavior IDs iff `behavior-mapped`; absent/empty otherwise (V13). Many clauses → one behavior is allowed (FR-010). |
| `rationale` | REQUIRED non-empty for `non-testable`/`not-applicable`; optional for `behavior-mapped`. |
| `notes` | Optional free text (e.g. supersession of a retired prose source unit). |

**Effective disposition of a clause** (research Decision 7):
1. A per-clause record → that disposition.
2. Else, if the clause's document `scope` is `authoring` AND `testability` ≠ `ambiguous`
   → the document-scope default.
3. Else → **unclassified** (V12, blocking).

Exactly one effective disposition; zero or two per-clause records → V12.

## Violation classes

V11–V14 are the **generalized** inventory-unit classes (the same `join_inventory` engine
serves 020's constraints and 021's clauses). V15 is new (prose source integrity).

| Class | Trigger | Where |
|---|---|---|
| **V11** stale | `clc` references a `clu` id absent from `clauses.json` (delete/re-point in the same change — waiver-style self-invalidation) | `validate` + `certify` |
| **V12** unclassified | clause with no effective disposition (incl. an unresolved `ambiguous` clause; incl. a clause whose only cover would be an *invalid* document-scope default), or classified more than once | `validate` + `certify` |
| **V13** malformed | `behaviors`/`rationale` arity vs disposition, `clc-` id-tail ≠ `clu-` tail, `clause`-XOR-`document` broken, or document-scope default on a `consumer` document | `validate` + `certify` |
| **V14** provenance | spec manifest fingerprint mismatch, inventory `revision` ≠ the `rev-spec-*` pin, or committed `clauses.json` ≠ canonicalized regeneration (byte-identity) | `validate` + `certify` |
| **V15** clause↔source | `strength` label contradicts excerpt RFC-2119 keywords, a `descriptive` clause hides a mandatory keyword, or an `excerpt` is absent from the pinned document under its `anchor` | `validate` + `certify` |

**Certification semantics** (wired last — research Decision 10): `certify` exits 1 iff any
gap OR any uncovered in-profile behavior OR any V11–V15 (constraint OR clause). Waivers are
listed but non-blocking; `not-applicable`/`non-testable` dispositions never block. No flag
bypasses this; no path invokes a model or the network.

**Drift = unclassified/stale by construction**: a revision bump re-canonicalizes the
inventory; changed substance yields new IDs (V12) and orphans old classifications (V11);
moves keep IDs (no new work). There is no separate "drift item" record type — V11+V12
mechanically enumerate the exact review queue (research Decision 8/9). `clause scaffold`
emits `UNREVIEWED`-sentinel skeletons (loader-rejected) so the queue is tractable without
ever auto-deciding a disposition.
