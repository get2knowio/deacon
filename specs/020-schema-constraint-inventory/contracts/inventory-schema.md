# Contract: Schemas Manifest + Constraint Inventory Files

Normative shapes live in data-model.md §1 (manifest) and §2 (inventory). This
contract fixes the file-level guarantees consumers (tests, `validate`, `certify`,
`diff`) may rely on.

## `conformance/schemas/<rev-pin>/manifest.json`

1. `revision` names an existing `schema`-kind revision record in the registry; the
   `<rev-pin>` directory name equals that record's `pin` value.
2. Every `documents[].file` exists as a sibling; its SHA-256 equals
   `documents[].sha256`. Verified before every parse; mismatch is blocking (V14).
3. Vendored schema files are byte-exact upstream copies — never reformatted, never
   patched. Updating them means re-vendoring at a new pin into a NEW `<rev-pin>`
   directory (plus a new revision record), never editing in place.
4. `documents[].key` values are unique, lowercase alphanumeric, and are the `<doc>`
   component of constraint IDs — renaming a key is an identity-breaking change and
   is treated like a revision bump.

## `conformance/inventory/constraints.json`

1. **Committed and canonical**: sorted object keys, units sorted by `id`, 2-space
   indent, LF endings, trailing newline, no timestamps/absolute paths. Regeneration
   from unchanged pinned inputs is byte-identical on every platform (enforced by the
   hermetic determinism test and `inventory check`; drift is V14).
2. `revision` equals the manifest's revision (and therefore the registry pin).
3. Unit `id`s are unique and grammar-valid (`cst` prefix). The `hash8` component is
   derived exclusively from `(document, pointer, kind, canonical substance)` — no
   environment, ordering, or annotation input.
4. **Completeness**: every keyword of every vendored schema document is represented —
   by a typed-kind unit, an `annotation` unit, or an `unmodeled-keyword` unit.
   Nothing is dropped at extraction time (spec FR-012; constitution IV).
5. **No partial writes**: generation either produces a complete valid file (temp +
   rename) or fails with a cause-specific error and leaves the previous file intact.
6. The file is machine-owned: hand edits are forbidden and are caught as V14
   (regeneration mismatch) on the next validate/CI run.

## Baseline assertions (FR-024)

`inventory_baseline.rs` pins observable facts about the real committed inventory,
including at minimum: the `forwardPorts` array-type unit and `features`
object/additional-properties units exist with expected substance (the units that
supersede the two retired hand-written source records); the base document's
top-level `oneOf` container-variant alternatives are present as `union-alternative`
units; a known nullable union carries `nullable: true`; and total unit count per
document is asserted with an exact number (updated consciously on re-vendoring —
a cheap, high-signal drift tripwire).
