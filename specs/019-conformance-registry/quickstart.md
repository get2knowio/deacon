# Quickstart: Conformance Registry

## Validate the registry (what PR CI runs)

```bash
cargo run -p deacon-conformance -- validate
# or the hermetic test that gates every PR:
cargo nextest run -E 'binary(=registry_valid)'
```

Exit 0 = structurally valid. Violations print one line each with a class code (V1–V10 /
SCHEMA), the offending record ID, and a message; all violations are reported in one run.

## Generate coverage reports

```bash
cargo run -p deacon-conformance -- report
# → target/conformance/report.json (machine-readable, byte-deterministic)
# → target/conformance/report.md   (human-readable)
```

## Evaluate strict certification (what the release workflow runs)

```bash
cargo run -p deacon-conformance -- certify
```

Exit 0 only when the active profile (`prof-linux-amd64-docker-0870`) has no gap records
and no uncovered in-profile behaviors. Waivers don't block; they're listed in the output.

## Record a new divergence (the common contributor flow)

1. Add/extend a **behavior** in `conformance/registry/behaviors/<area>.json` with all
   three axes — e.g. `spec: conformant`, `reference: divergent`,
   `decision: intentional-divergence`. Check `conformance/RULES.md` for the
   contradiction rules (R1–R8).
2. Link it from the **source unit(s)** that mandate it (`sources/*.json`), or add those
   units (provenance: revision + locator).
3. Cover it: a **case** in `cases.json` referencing the executable test, or a **waiver**
   in `waivers/` (rationale + `expires` required), or — if it's genuinely unresolved — a
   **gap** in `gaps.json` with `decision: unresolved-gap`.
4. `cargo run -p deacon-conformance -- validate`, fix what it flags, commit. Never edit
   a stable ID after merge.

## Recording rules of thumb

- No test case *or waiver* yet? Then `reference: unknown` → `decision: unresolved-gap` →
  gap record. Statuses are evidence-backed claims, not intentions (rules R8→R4→R7). A
  waiver counts as evidence because the parity harness verifies it keeps reproducing.
- Deacon-only capability? `ext-` record + behaviors with `decision: deacon-extension`
  and `spec: unspecified`/`not-applicable` — it will never be reported as a divergence.
- Waiver expired (V6)? Re-review: either extend `expires` with an updated rationale in
  the same edit, or delete the waiver and fix the divergence.
- Bumping a source pin (`revisions.json`)? Re-verify every source unit citing that
  revision in the same PR, and update the `verifiedAgainst` file or V7 fails.

## Deterministic testing knobs

- `--registry <dir>` points validation at a fixture registry
  (`fixtures/conformance/...`).
- `--today 2027-01-19` pins "today" for waiver-expiry tests (expiry is valid *through*
  the stated date).
