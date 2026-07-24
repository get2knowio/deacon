# Data Model: Normative Clause Inventory

All new data is strict JSON (`deny_unknown_fields`, `camelCase`, matching every
`deacon-conformance` record). Four artifact families, paralleling feature 020:
**vendored prose** (third-party, byte-exact Markdown), **generated clause inventory**
(canonicalized from authored records, committed), **clause classifications**
(hand-authored registry records), and command-only **diff** output. New enums/structs
land in `crates/conformance/src/model.rs`; new prefixes join the closed ID grammar in
`RecordType`/`parse_id`.

## 1. Spec manifest — `conformance/spec/<rev-pin>/manifest.json`

```json
{
  "schemaVersion": 1,
  "revision": "rev-spec-113500f4",
  "documents": [
    {
      "key": "reference",
      "file": "devcontainer-reference.md",
      "upstreamUrl": "https://raw.githubusercontent.com/devcontainers/spec/113500f4/docs/specs/devcontainer-reference.md",
      "sha256": "<64-hex captured at vendoring>",
      "scope": "consumer"
    },
    {
      "key": "features",
      "file": "devcontainer-features.md",
      "upstreamUrl": "https://raw.githubusercontent.com/devcontainers/spec/113500f4/docs/specs/devcontainer-features.md",
      "sha256": "<64-hex>",
      "scope": "authoring"
    }
  ]
}
```

| Field | Rules |
|---|---|
| `revision` | MUST name an existing `rev-` record of kind `spec` in `registry/revisions.json` (V14 on mismatch). |
| `documents[].key` | Document key used in clause IDs, classification files, and diff sort order. Lowercase `[a-z0-9-]+`, unique. |
| `documents[].sha256` | Verified against the vendored file bytes before every parse; mismatch → hard error (V14 / `SpecFingerprintMismatch`). |
| `documents[].upstreamUrl` | Provenance only — never fetched by any command. |
| `documents[].scope` | Closed enum `consumer` \| `authoring`. Gates the document-scope disposition default (§3): a document-scope `not-applicable` record is permitted only for `authoring` documents (V13 otherwise). |

All **18** ratified `docs/specs/` documents at `113500f4` are mandatory (14 consumer, 4
authoring — the full set enumerated in research Decision 6). The two rows above are
illustrative; the manifest lists every document. Adding a future ratified doc at a new pin =
a manifest entry + vendored file.

## 2. Clause unit — element of `conformance/inventory/clauses.json`

```json
{
  "schemaVersion": 1,
  "revision": "rev-spec-113500f4",
  "units": [
    {
      "id": "clu-reference-oncreatecommand-run-once-must-a1b2c3d4",
      "document": "reference",
      "strength": "must",
      "testability": "directly-testable",
      "fingerprint": "9f2a…<64 hex sha256 of normalized substance>",
      "locations": [
        {
          "heading": "Lifecycle scripts > onCreateCommand",
          "anchor": "oncreatecommand",
          "ordinal": 1,
          "excerpt": "`onCreateCommand` - This command is the first of three commands ... and MUST be run only once."
        }
      ],
      "context": null
    }
  ]
}
```

| Field | Rules |
|---|---|
| `id` | `clu-<doc>-<substance-slug>-<strength>-<hash8>` (research Decision 2). Grammar-valid per `parse_id` with new `clu` prefix. Unique; identical normalized substance in one document ⇒ the **same** unit (its locations merge), so there is no collision case for genuine duplicates. |
| `document` | Manifest document key. |
| `strength` | Closed enum: `must`, `should`, `may`, `algorithm`, `io-contract`, `descriptive`. |
| `testability` | Closed enum: `directly-testable`, `indirectly-testable`, `informative`, `ambiguous`, `not-applicable`. |
| `fingerprint` | Full lowercase-hex SHA-256 of `normalize_substance(excerpt)` (Decision 3). The **distinct fingerprint field** the clarification requires; drift's material-vs-immaterial decision reads this. |
| `locations` | Non-empty array; one entry per place the same normalized substance appears. Each: `heading` (human path), `anchor` (GitHub-style slug of the owning heading), `ordinal` (1-based position within that heading — provenance/order only, never an identity input), `excerpt` (verbatim source substring; the human-readable field). Sorted by `(anchor, ordinal)`. |
| `context` | Nullable. Optional structural note, e.g. `{ "inCodeFence": true }` for an I/O contract shown in a fenced block, or `{ "listContext": "onCreateCommand bullet" }`. |

**Identity** (Decision 2): `hash8` = first 8 chars of the lowercase-hex SHA-256 of
`document ‖ normalize_substance(excerpt)` (`‖` = separator byte `0x1f`, reusing 020's
`HASH_SEPARATOR`). **Location is excluded from the hash**, so a pure move preserves the ID
(and its disposition); a material change (obligation or strength reworded ⇒ different
normalized substance) mints a new ID (drift-forcing). `strength` appears in the ID string
and, because a strength change always co-occurs with an excerpt change, is covered by the
fingerprint too. `<substance-slug>` is a bounded (≤ 48-char) slug of the normalized
substance's leading tokens — readability only; identity lives in the hash.

**Normalized substance** (Decision 3): `normalize_substance(excerpt)` = lowercase, strip
Markdown formatting (emphasis, inline-code backticks, link syntax, list bullets, block-quote
markers), collapse whitespace runs to one space, trim; preserve RFC-2119 keywords, code-span
contents, identifiers, and meaningful punctuation. Pure, deterministic, unit-tested in
isolation. Two excerpts differing only in whitespace/markdown reflow normalize equal ⇒ same
fingerprint ⇒ immaterial (SC-005).

**Ordering**: `units` sorted by `id`; file is canonical JSON (`to_string_pretty`, 2-space
indent, LF, trailing newline — 020's `render` discipline). Byte-identical regeneration is the
contract (V14).

**State transitions** (clause lifecycle across a revision bump):

```
(absent) ──author+generate──▶ unclassified (V12 blocking) ──human clc record──▶ classified
classified ──pure move upstream──▶ same id, locations[] change (reported "moved", NON-blocking, disposition kept)
classified ──substance/strength change──▶ old id absent (clc goes V11-stale)
                                          + new id unclassified (V12) ── review ──▶ classified
authored ambiguous ──▶ testability="ambiguous", no doc-scope cover allowed ──▶ V12 until human resolves
```

## 3. Clause classification — `conformance/registry/clause-classifications/<doc>.json`

Collection envelope identical to other registry files. Two record shapes in one closed
model (`ClauseClassification`), exactly one of `clause` / `document` present:

**Per-clause** (required for every consumer-document clause and every `ambiguous` clause):

```json
{
  "id": "clc-reference-oncreatecommand-run-once-must-a1b2c3d4",
  "clause": "clu-reference-oncreatecommand-run-once-must-a1b2c3d4",
  "disposition": "behavior-mapped",
  "behaviors": ["bhv-up-lifecycle-oncreate-once"],
  "rationale": null,
  "notes": "Supersedes retired src-spec-lifecycle-up prose link."
}
```

**Document-scope default** (permitted only for `authoring`-scope documents):

```json
{
  "id": "clc-doc-features",
  "document": "features",
  "disposition": "not-applicable",
  "rationale": "Feature-authoring document; deacon consumes published features but never authors or validates them (constitution II)."
}
```

| Field | Rules |
|---|---|
| `id` | Per-clause: `clc-` + the exact tail of the referenced `clu-` id (structural mirror; V13 if mismatched). Document-scope: `clc-doc-<document key>`. |
| `clause` XOR `document` | Exactly one present (V13 otherwise). `clause` MUST exist in the committed inventory (V11 when stale). `document` MUST be an `authoring`-scope manifest key (V13 otherwise). |
| `disposition` | Closed enum, reusing 020's `Disposition`: `behavior-mapped`, `non-testable` (the "informative" flavor for prose), `not-applicable`. |
| `behaviors` | Non-empty and every ID an existing behavior iff `behavior-mapped`; absent/empty otherwise (V13). Several clauses MAY map to one behavior (FR-010). |
| `rationale` | REQUIRED non-empty for `non-testable` and `not-applicable`; optional for `behavior-mapped`. |

**Resolution order for a clause** (research Decision 7): a per-clause record wins; else, if
the clause's document is `authoring`-scope AND its `testability` ≠ `ambiguous`, a
document-scope default applies; else the clause is unclassified (V12, blocking). Exactly one
effective disposition per clause; zero or two per-clause records is V12.

**Certification semantics** (wired last — Decision 10): any V11/V12/V13/V14/V15 is blocking
in `validate`; `certify` additionally fails while any clause is unclassified or any
classification is stale — the prose analogue of "a gap always blocks."
`not-applicable`/`non-testable` never block.

## 4. Revision diff (command output, not committed)

Match key **normalized-substance fingerprint** over two clause-inventory files
(Decision 9 — deliberately NOT 020's `(document, pointer, kind)` key, so moves are visible):

```json
{
  "schemaVersion": 1,
  "old": { "revision": "rev-spec-113500f4" },
  "new": { "revision": "rev-spec-abcdef12" },
  "new_clauses": [ { "id": "clu-…", "document": "…", "strength": "…", "locations": [ … ] } ],
  "removed": [ … ],
  "moved":   [ { "id": "clu-…", "oldLocations": [ … ], "newLocations": [ … ] } ],
  "changed": [ { "document": "…", "heading": "…", "oldId": "clu-…", "newId": "clu-…", "oldExcerpt": "…", "newExcerpt": "…" } ],
  "nonMaterial": [ { "id": "clu-…", "oldExcerpt": "…", "newExcerpt": "…" } ]
}
```

- **new**: fingerprint present only on the right.
- **removed**: fingerprint present only on the left.
- **moved**: same id (fingerprint) both sides, `locations` differ → old/new locations shown;
  **non-blocking**, disposition preserved.
- **changed**: a removed old-id and a new new-id share a heading (a material rewrite reads as
  changed for the reviewer, though mechanically it is remove-old + add-new).
- **nonMaterial**: same id both sides, excerpt bytes differ but fingerprint equal
  (whitespace/reflow) — never reported as material (SC-005).

Deterministically sorted by `(document, heading, id)`. Markdown twin for human review. The
diff is advisory; the *enforcement* of drift is V11/V12 against the regenerated committed
inventory.

## 5. Model extensions in `crates/conformance/src/model.rs`

- `RecordType` gains `ClauseUnit` (`clu`) and `ClauseClassification` (`clc`); `parse_id`
  grammar regex extends its prefix alternation to `…|cst|cls|clu|clc`. Existing V2
  duplicate/sort/prefix checks (`all_ids`) extend to the new collections automatically via
  the loader.
- New structs: `SpecManifest`, `SpecDocument` (with `DocumentScope` enum), `ClauseInventory`,
  `ClauseUnit`, `Strength` enum, `Testability` enum, `ClauseLocation`, `ClauseClassification`
  (with a `clause`-XOR-`document` invariant), plus `clause_diff` output types. All
  `deny_unknown_fields`, `camelCase`, kebab-case wire values for enums.
- New error variants (thiserror, cause-specific per FR-003/FR-008/FR-014): `SpecFingerprintMismatch`,
  `ExcerptNotFoundAtAnchor { clause, heading }`, `StrengthKeywordMismatch { clause, labeled,
  detected }`, `AmbiguousClauseUnclassified { clause }`, `ClauseInventoryOutOfDate`,
  `DocumentScopeOnConsumerDoc { document }`.

## 6. Violation classes (documented in conformance/RULES.md in lockstep)

V11–V14 are **generalized** from "constraint unit" to "inventory unit (schema constraint OR
prose clause)" — the same `join_inventory`/`InventoryJoin` engine runs for both inventories.
V15 is new (prose-only source integrity).

| Class | Meaning | Blocking |
|---|---|---|
| V11 | Stale classification — a `cls`/`clc` record references a unit ID (`cst`/`clu`) absent from its committed inventory | validate + certify |
| V12 | Unclassified — an inventory unit with no effective disposition (no per-unit record, and, for a clause, no permitted document-scope cover), or classified more than once; an unresolved `ambiguous` clause is V12 by construction | validate + certify |
| V13 | Malformed classification — behavior missing/arity, id-tail mismatch, `clause`-XOR-`document` broken, or a document-scope default applied to a non-`authoring` document | validate + certify |
| V14 | Provenance — spec/schema manifest fingerprint mismatch, inventory `revision` ≠ registry pin of the matching kind, or committed inventory ≠ canonicalized regeneration (byte-identity) | validate + certify |
| V15 (new) | Clause↔source integrity — `strength` label contradicts excerpt keywords (Decision 4), a `descriptive` clause hides a mandatory RFC-2119 keyword, or a clause `excerpt` is absent from the pinned document under its recorded heading | validate + certify |
