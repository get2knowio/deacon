# `conformance/migration/` — the parity→conformance migration record

Committed data for feature `023-migrate-parity-to-conformance`. Two files, two
different owners. **The ownership boundary is the point of this directory** — it
mirrors the V14 provenance discipline already used for
`conformance/inventory/constraints.json` (machine-owned) versus
`conformance/registry/classifications/*.json` (hand-authored).

| File | Owner | Written by | Hand edits |
|---|---|---|---|
| `baseline.json` | **machine** | `baseline generate` **only** | rejected as **V25** drift |
| `mapping.json` | **hand-authored** | never written by any generator | the normal way to change it |

## `baseline.json` — machine-owned

The frozen, mechanically enumerated inventory of pre-migration coverage: one
record per baseline unit, where a unit is *the finest granularity for which the
pre-migration system reports an independent outcome* (FR-049).

- **Regenerate, never hand-edit**:
  `cargo run -p deacon-conformance -- baseline generate --freeze <sha>`
- **Verify** (recompute in memory, byte-compare, never write):
  `cargo run -p deacon-conformance -- baseline check`
- Deterministic: records sorted by `id`, no timestamps, no absolute paths, no
  machine-specific values. Regeneration on an unchanged tree is byte-identical.
- `revision` is the freeze commit and participates in **V25**. `baseline
  generate` refuses to overwrite a frozen baseline without `--force`, so
  re-running can never silently *lower* the bar the conservation claim is
  measured against (FR-045).
- `assertion` text is authored once, at freeze, in the generator's authored
  tables (`crates/conformance/src/baseline.rs`) so regeneration reproduces it
  byte-for-byte. It records what the unit asserted *before* migration and is
  **immutable** thereafter — rewriting it post hoc would let the coverage proof
  be satisfied by lowering the bar.

Corpus units are derived by calling the *production* discovery functions
(`discover_tier1_cases`, `discover_error_cases`), never an independent directory
walk — re-walking is exactly how 24 Tier-1 cases were once miscounted as 25
(research D1).

## `mapping.json` — hand-authored

The explicit baseline-unit → destination mapping (`migrated` / `deduplicated` /
`residual` / `retired`). Equal counts do not prove conservation; this table is
the proof. **Generation never writes it.** `migration scaffold` emits skeleton
records to *stdout* carrying `"UNREVIEWED"` sentinels the loader rejects, exactly
as `inventory scaffold` / `clause scaffold` do.

Orphans are structurally impossible in both directions: a baseline unit with no
mapping entry, and a mapped case id no unit reaches, are both **V21**.

## Related

- Residual records live in `conformance/registry/residuals.json` (hand-authored,
  non-blocking — representation debt, not a coverage gap; distinct from
  `gaps.json`, which continues to block certification).
- The derived before/after conservation report is written to
  `target/conformance/` and is git-ignored — it is never committed.
- Rules and violation classes: `conformance/RULES.md`.
