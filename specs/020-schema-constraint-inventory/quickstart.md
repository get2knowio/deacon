# Quickstart: Schema Constraint Inventory

Developer walkthrough for the machinery this feature adds to the dev-only
`deacon-conformance` crate. Everything below is offline except the one-time
vendoring step.

## Everyday commands

```bash
# Regenerate the committed inventory from the vendored pinned schemas
cargo run -p deacon-conformance -- inventory generate

# Verify the committed inventory is exactly what regeneration produces (CI does this too)
cargo run -p deacon-conformance -- inventory check

# Full registry validation — now includes the inventory/classification join (V11–V14)
cargo run -p deacon-conformance -- validate

# Emit skeleton classification records for anything unclassified
cargo run -p deacon-conformance -- inventory scaffold > /tmp/skeletons.json

# Compare two inventory revisions (drift review)
cargo run -p deacon-conformance -- inventory diff old-constraints.json conformance/inventory/constraints.json --format md
```

Hermetic tests: `cargo nextest run -E 'binary(=inventory_extraction) + binary(=inventory_determinism) + binary(=inventory_baseline) + binary(=inventory_diff) + binary(=classification_join)'`
(all in the fast lanes; no Docker, no network).

## Classifying a constraint

1. Find the unit in `conformance/inventory/constraints.json` (or via a V12 violation
   from `validate`).
2. Add ONE record to `conformance/registry/classifications/<doc>.json`:
   - Consumer-runtime and tested → `behavior-mapped` + the `bhv-` id(s).
   - Consumer-runtime but no behavior yet → create/extend the behavior first (or,
     honestly, leave it unclassified — it blocks, which is the point).
   - Authoring/editor-only → `not-applicable` + rationale.
   - Title/description/JSONC directive → `non-testable` + rationale.
3. `validate` until clean. Never delete a unit to go green — units are machine-owned.

## Re-vendoring on an upstream pin bump (the drift workflow)

1. **(network, one-time, human)** Download the schema files at the new commit into
   `conformance/schemas/<newpin>/`; compute `sha256sum`; write the new
   `manifest.json`; add the `rev-schema-<newpin>` revision record.
2. `inventory generate` → the committed inventory rewrites under the new revision.
3. `inventory diff` old vs new → human-readable review document.
4. `validate` now enumerates the exact review queue: V11 = stale classifications to
   delete/re-point, V12 = new/changed units to classify. Nothing inherits a
   disposition by name; `certify` blocks until the queue is empty.
5. Classify, delete stale records, commit everything in one PR.

## Guard rails to remember

- `conformance/inventory/constraints.json` is generated — hand edits are detected
  (V14) and rejected in CI.
- Classification files are hand-authored — `inventory generate` never touches them.
- No command fetches the network. An unpinned `$ref` (e.g. the editor-composite
  schema's live GitHub URLs) fails with `UnresolvedExternalRef` by design.
- Fixture schemas for the error paths (cycles, malformed, unresolved refs) live
  under `fixtures/conformance/schemas/` — extend those, not the vendored pinned
  copies, which are byte-exact upstream artifacts and never edited.
