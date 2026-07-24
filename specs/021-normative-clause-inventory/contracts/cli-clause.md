# CLI Contract: `conformance clause …`

Dev-only subcommands on the existing `conformance` bin
(`cargo run -p deacon-conformance -- clause <cmd>`). NOT part of the `deacon` consumer
CLI. Shared conventions with `validate`/`report`/`certify`/`inventory`: results to stdout
or `--out` files; diagnostics via `tracing` to stderr; deterministic outputs (no
timestamps, no absolute paths); `--registry <dir>`, `--spec <dir>`, and `--clauses <file>`
overrides exist so tests run against fixtures. **No subcommand performs network IO, and no
subcommand invokes an LLM — ever.** These are pure functions of the committed records and
the vendored pinned prose.

## `clause generate [--spec <dir>] [--clauses <file>]`

**Canonicalizes** the committed clause records (it does NOT invent clauses — segmentation
is out-of-band authoring work, research Decision 1). Reads the committed clause records +
the spec manifest + vendored prose, verifies SHA-256 fingerprints, then for each authored
record: recomputes `normalize_substance(excerpt)` → `fingerprint`, recomputes the
substance-anchored `id`, verifies the excerpt is present in the pinned document under its
recorded heading, merges records that share a normalized substance into one unit with
combined `locations`, sorts canonically, and writes the byte-stable inventory (default
`conformance/inventory/clauses.json`).

- Exit 0: inventory written (byte-stable; rewriting identical content is fine).
- Exit 1: any integrity error — `SpecFingerprintMismatch`, `ExcerptNotFoundAtAnchor`
  (names the clause + heading), `StrengthKeywordMismatch` (names labeled vs detected),
  malformed record. Never writes a partial file (temp-file + `fs::rename`, matching
  `inventory::write_inventory` / `cache/disk.rs::save_index` discipline).

## `clause check [--spec <dir>] [--clauses <file>]`

Regenerates in memory and byte-compares against the committed inventory.

- Exit 0: identical.
- Exit 1: differs (`ClauseInventoryOutOfDate`, with a compact unit-level summary of
  new/removed/moved/changed IDs) or any generate-class error.
- This is the CLI face of the hermetic determinism/provenance test; CI runs the test,
  humans run `check`.

## `clause diff <old.json> <new.json> [--format json|md] [--out <file>]`

Deterministic revision diff per data-model §4 (match key: normalized-substance
fingerprint). Reports `new_clauses`, `removed`, `moved`, `changed`, and `nonMaterial`.

- Exit 0: diff produced (including an empty diff).
- Exit 1: either input unreadable/malformed.
- Output sorted by `(document, heading, id)`; `--format md` renders the human review
  document. Moves are first-class and non-blocking; material changes surface as
  removed-old-id + new-new-id sharing a heading.

## `clause scaffold [--clauses <file>] [--registry <dir>]`

Emits skeleton `clc-` records (to stdout) for every currently unclassified clause, with
`disposition` set to the sentinel `"UNREVIEWED"` — a value the loader REJECTS (closed
`Disposition` enum, no such variant) — so scaffolded output cannot be committed unedited.
For clauses in an `authoring`-scope document it emits a single per-document skeleton where
one does not yet exist; for consumer-doc and `ambiguous` clauses it emits per-clause
skeletons. Never writes into the registry itself.

- Exit 0: skeletons emitted (possibly zero).
- Exit 1: inventory/registry/manifest unreadable.

## Interactions with existing commands

- `validate`: additionally loads the spec manifest + clause inventory + clause
  classifications and enforces V11–V15 (see clause-classification-schema.md). V11–V14 are
  the *generalized* classes (already run for 020's constraints); V15 is clause-specific.
  All violations reported in one run, consistent with existing behavior.
- `report`: gains a clause-inventory section (unit counts by strength/testability/document,
  disposition tallies, unclassified list, ambiguous-pending list). Byte-stable as before.
- `certify`: (final phase only) fails when any V11/V12/V13/V14/V15 exists for the clause
  inventory — i.e. exit 1 iff gap OR uncovered in-profile behavior OR unclassified/stale
  clause OR provenance/source-integrity breakage. `not-applicable`/`non-testable`
  dispositions never block. No flag can bypass this (no silent weakening); no path invokes
  a model.
