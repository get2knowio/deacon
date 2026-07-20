# Data Model: Schema Constraint Inventory

All new data is strict JSON (`deny_unknown_fields` on load — constitution IV "strict
on mistakes": these are deacon-owned modeled formats). Three artifact families:
**vendored schemas** (third-party, byte-exact), **generated inventory** (machine-owned,
committed), **classifications** (hand-authored registry records).

## 1. Schemas manifest — `conformance/schemas/<rev-pin>/manifest.json`

```json
{
  "schemaVersion": 1,
  "revision": "rev-schema-113500f4",
  "documents": [
    {
      "key": "base",
      "file": "devContainer.base.schema.json",
      "upstreamUrl": "https://raw.githubusercontent.com/devcontainers/spec/113500f4/schemas/devContainer.base.schema.json",
      "sha256": "a0883c0405ff433db188849d458fb20b9c0d73e0ba1a6e44c1d83f3b485408dd"
    },
    {
      "key": "feature",
      "file": "devContainerFeature.schema.json",
      "upstreamUrl": "https://raw.githubusercontent.com/devcontainers/spec/113500f4/schemas/devContainerFeature.schema.json",
      "sha256": "671fcd80cbb3510793412746b35505d12b6a4b8e68d38a01960e440aa5af32eb"
    }
  ]
}
```

| Field | Rules |
|---|---|
| `revision` | MUST name an existing `rev-` record of kind `schema` in `registry/revisions.json` (V14 on mismatch). |
| `documents[].key` | Document key used in constraint IDs and diff match keys. Lowercase `[a-z0-9]+`, unique. |
| `documents[].sha256` | Verified against the vendored file bytes before every parse; mismatch → hard error (V14 / `ManifestFingerprintMismatch`). |
| `documents[].upstreamUrl` | Provenance only — never fetched by any command. |

**Relationships**: manifest → revision record (registry); manifest → vendored files
(same directory). Adding an optional external schema (P4) = adding a manifest entry +
vendored file; nothing else changes shape.

## 2. Constraint unit — element of `conformance/inventory/constraints.json`

```json
{
  "schemaVersion": 1,
  "revision": "rev-schema-113500f4",
  "units": [
    {
      "id": "cst-base-forwardports-type-3fa9c214",
      "document": "base",
      "pointer": "/definitions/devContainerCommon/properties/forwardPorts",
      "kind": "type",
      "substance": { "type": "array" },
      "context": null
    }
  ]
}
```

| Field | Rules |
|---|---|
| `id` | `cst-<doc>-<slug>-<kind code>-<hash8>` (research Decision 6). Grammar-valid per `parse_id` with new `cst` prefix. Unique; collision at generation is a hard error. |
| `document` | Manifest document key. |
| `pointer` | RFC 6901 JSON Pointer to the schema object owning the facet (definition site — Decision 3). |
| `kind` | Closed enum, kebab-case: `property-existence`, `required`, `type`, `enum`, `const`, `default`, `union-alternative`, `all-of`, `conditional`, `additional-properties`, `array-shape`, `value-shape`, `reference`, `annotation`, `unmodeled-keyword`. |
| `substance` | Canonicalized JSON value of the facet — the testable rule itself (e.g. the type set, the enum members, the required-property name, the ref target pointer, the additional-properties mode). For `unmodeled-keyword`: `{ "keyword": …, "value": … }` verbatim. |
| `context` | Nullable. Composition context when the owning object sits inside a branch: `{ "branch": "oneOf", "index": 2 }` or `{ "condition": "/definitions/x/if" }`. |

**Identity**: `hash8` = the first 8 characters of the lowercase hex SHA-256 digest of
`document ‖ pointer ‖ kind ‖ canonical(substance)` (`‖` = a fixed separator byte).
The `<slug>` component is truncated to at most **48 characters** (deterministic
truncation, never elided mid-run) — readability only; identity lives in the hash
inputs. Substance participates ⇒ material change ⇒ new ID (drift-forcing). Annotations are
their own units, so their churn never moves a testable unit's ID.

**Nullability facet**: `"null"` appearing in a type union is represented inside the
`type` unit's substance (`{"type": ["string","null"], "nullable": true}`) — one unit,
explicit flag — so null-handling assertions (FR-023) have a single stable target.

**Ordering**: `units` sorted by `id`; file is canonical JSON (sorted keys, 2-space
indent, LF, trailing newline). Byte-identical regeneration is contract (V14).

**State transitions** (unit lifecycle across a revision bump):

```
(absent) ──extract──▶ unclassified (V12 blocking) ──human cls record──▶ classified
classified ──substance change upstream──▶ old id absent (cls goes V11-stale)
                                          + new id unclassified (V12)   ── review ──▶ classified
```

## 3. Classification record — `conformance/registry/classifications/{base,feature}.json`

Collection envelope identical to other registry files. Record:

```json
{
  "id": "cls-base-forwardports-type-3fa9c214",
  "constraint": "cst-base-forwardports-type-3fa9c214",
  "disposition": "behavior-mapped",
  "behaviors": ["bhv-readconfig-wrong-type-forwardports-rejected"],
  "rationale": null,
  "notes": "Supersedes retired src-schema-forwardports-type."
}
```

| Field | Rules |
|---|---|
| `id` | `cls-` + the exact tail of the referenced `cst-` id (structural mirror; V13 if mismatched). |
| `constraint` | MUST exist in the committed inventory (V11 when stale). Exactly one classification per constraint (V12 covers zero and duplicates). |
| `disposition` | Closed enum: `behavior-mapped`, `non-testable`, `not-applicable`. |
| `behaviors` | Non-empty and every ID an existing behavior iff `behavior-mapped`; MUST be absent/empty otherwise (V13). |
| `rationale` | REQUIRED non-empty for `non-testable` and `not-applicable`; optional for `behavior-mapped`. |

**Certification semantics** (wired last — Decision 10): any V11/V12/V13/V14 violation
is blocking in `validate`; `certify` additionally fails while any unit is
unclassified or any classification is stale — the constraint-level analogue of "a gap
always blocks". `not-applicable` and `non-testable` never block (they are the honest
out-of-consumer-scope / no-behavior dispositions and remain visible in reports).

## 4. Revision diff (command output, not committed)

Match key `(document, pointer, kind)` over two inventory files (Decision 9):

```json
{
  "schemaVersion": 1,
  "old": { "revision": "rev-schema-113500f4" },
  "new": { "revision": "rev-schema-abcdef12" },
  "added": [ { "id": "cst-…", "document": "…", "pointer": "…", "kind": "…", "substance": … } ],
  "removed": [ … ],
  "changed": [ { "document": "…", "pointer": "…", "kind": "…", "oldId": "cst-…", "newId": "cst-…", "oldSubstance": …, "newSubstance": … } ],
  "nonMaterial": [ … annotation-kind differences … ]
}
```

Deterministically sorted by match key. Markdown twin for human review. The diff is
advisory tooling; the *enforcement* of drift is V11/V12 against the regenerated
committed inventory.

## 5. Model extensions in `crates/conformance/src/model.rs`

- `RecordType` gains `Constraint` (`cst`) and `Classification` (`cls`); `parse_id`
  grammar unchanged otherwise. Existing V2 duplicate/sort checks extend to
  classification collections automatically via the loader.
- New structs: `SchemasManifest`, `ManifestDocument`, `ConstraintInventory`,
  `ConstraintUnit`, `ConstraintKind`, `UnitContext`, `Classification`,
  `Disposition`, plus `diff` output types. All `deny_unknown_fields`, camelCase.
- New error variants (thiserror, cause-specific per FR-009): `MalformedSchema`,
  `MalformedRef`, `UnresolvedRef`, `UnresolvedExternalRef`, `RefCycle { chain }`,
  `ManifestFingerprintMismatch`, `IdCollision`, `InventoryOutOfDate`.

## 6. Violation classes added (documented in conformance/RULES.md in lockstep)

| Class | Meaning | Blocking |
|---|---|---|
| V11 | Stale classification (constraint ID not in committed inventory) | validate + certify |
| V12 | Constraint unclassified, or classified more than once | validate + certify |
| V13 | Classification shape/linkage broken (behavior missing, arity rules, id-tail mismatch) | validate + certify |
| V14 | Provenance broken (manifest fingerprint, revision pin mismatch, committed inventory ≠ regeneration) | validate + certify |
