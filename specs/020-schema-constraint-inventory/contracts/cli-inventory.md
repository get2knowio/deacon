# CLI Contract: `conformance inventory …`

Dev-only subcommands on the existing `conformance` bin
(`cargo run -p deacon-conformance -- inventory <cmd>`). NOT part of the `deacon`
consumer CLI. Shared conventions with `validate`/`report`/`certify`: results to
stdout or `--out` files; diagnostics via `tracing` to stderr; deterministic outputs
(no timestamps, no absolute paths); `--registry <dir>` and `--schemas <dir>` and
`--inventory <file>` overrides exist so tests run against fixtures. **No subcommand
performs network IO, ever.**

## `inventory generate [--schemas <dir>] [--out <file>]`

Reads the manifest + vendored schemas, verifies SHA-256 fingerprints, extracts, and
writes the canonical inventory (default `conformance/inventory/constraints.json`).

- Exit 0: inventory written (byte-stable; rewriting identical content is fine).
- Exit 1: any extraction error — `MalformedSchema`, `MalformedRef`, `UnresolvedRef`,
  `UnresolvedExternalRef`, `RefCycle` (message lists the full chain),
  `ManifestFingerprintMismatch`, `IdCollision`. Never writes a partial file (write
  is temp-file + rename, matching `cache/disk.rs::save_index` discipline).

## `inventory check [--schemas <dir>] [--inventory <file>]`

Regenerates in memory and byte-compares against the committed inventory.

- Exit 0: identical.
- Exit 1: differs (`InventoryOutOfDate`, with a compact unit-level summary of
  added/removed/changed IDs) or any generate-class error.
- This is the CLI face of the hermetic determinism test; CI runs the test, humans
  run `check`.

## `inventory diff <old.json> <new.json> [--format json|md] [--out <file>]`

Deterministic revision diff per data-model §4 (match key: document, pointer, kind).

- Exit 0: diff produced (including an empty diff).
- Exit 1: either input unreadable/malformed.
- Output sorted by match key; `--format md` renders the human review document.

## `inventory scaffold [--inventory <file>] [--registry <dir>]`

Emits skeleton `cls-` records (to stdout) for every currently unclassified
constraint unit, with `disposition` set to the sentinel `"UNREVIEWED"` — a value the
loader REJECTS — so scaffolded output cannot be committed unedited. Never writes
into the registry itself.

- Exit 0: skeletons emitted (possibly zero).
- Exit 1: inventory/registry unreadable.

## Interactions with existing commands

- `validate`: additionally loads manifest + inventory + classifications and enforces
  V11–V14 (see classification-schema.md). All violations reported in one run,
  consistent with existing behavior.
- `report`: gains an inventory section (unit counts by kind/document, disposition
  tallies, unclassified list). Byte-stable as before.
- `certify`: (final phase only) fails when any V11/V12/V13/V14 exists — i.e. exit 1
  iff gap OR uncovered in-profile behavior OR unclassified/stale/duplicated
  constraint OR provenance breakage. `not-applicable`/`non-testable` dispositions
  never block. No flag can bypass this (no silent weakening).
