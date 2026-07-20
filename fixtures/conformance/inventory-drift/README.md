# Drift fixtures — `fixtures/conformance/inventory-drift/`

Two hand-authored fixture schema revisions (`old/` and `new/`) for the constraint
inventory **drift diff** tests (feature `020-schema-constraint-inventory`, User Story 3,
tasks T032–T033). They are **NOT vendored upstream artifacts** — like everything under
`fixtures/conformance/schemas/`, they are minimal synthetic schemas authored for this
feature's tests only, carrying no upstream provenance. The real pinned schemas live under
`conformance/schemas/<rev-pin>/` and are never edited.

Each subdirectory holds a `drift.json` schema plus a sibling `manifest.json`
(fingerprinted by SHA-256, document key `drift`). Inventories are **generated in the
test** from these schemas via the extraction pipeline (`generate_inventory`) — nothing is
precomputed or committed here, so the fixtures can never go stale relative to the
extractor. If you edit a `drift.json`, recompute its SHA-256 (`sha256sum drift.json`) into
the sibling `manifest.json`, or `generate_inventory` fails the fingerprint check.

## Designed diff (`new` vs `old`)

The two revisions differ so that `inventory diff old new` deterministically yields:

| Bucket        | Count | Units |
|---------------|-------|-------|
| `added`       | 2     | `/definitions/newbie` (pure add) + `/definitions/leafmoved` (the move-in) |
| `removed`     | 2     | `/definitions/goner` (pure remove) + `/definitions/leaf` (the move-out) |
| `changed`     | 1     | `/properties/widened` `type` — widened `string` → `["string","null"]` |
| `nonMaterial` | 1     | `/properties/documented` `annotation` — description reworded only |

The **moved-but-identical** constraint (`definitions/leaf` → `definitions/leafmoved`,
both `{"type":"boolean"}`) is reported as one `removed` + one `added` — the diff does NOT
attempt fuzzy move-tracking (spec Assumption: *"A moved-but-identical constraint is
reported as removed + added"*). Because the match key is `(document, pointer, kind)` — NOT
the substance-hashing `id` — the type-widening at `/properties/widened` reads as a single
`changed` entry (`oldId != newId`, old + new substance shown) rather than an unrelated
add/remove pair. The `annotation`-kind description change is segregated into `nonMaterial`
because annotations carry no testable behavior (spec Assumption: *"Descriptive metadata is
non-testable, not invisible"*).

All other units (`/`, `/properties/stable`, the `documented`/`widened`
`property-existence`, the `stable`/`documented` `type`) are byte-identical across the two
revisions and are therefore not reported.
