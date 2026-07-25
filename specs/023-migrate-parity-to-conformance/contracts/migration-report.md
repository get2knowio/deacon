# Contract: No-Coverage-Loss Report

**Output**: `target/conformance/migration-report.{json,md}` (derived, git-ignored) · **Producer**: `migration report` · **Gate**: `migration check`

This report is the acceptance evidence for the feature and the precondition for deleting anything (FR-039–FR-045).

## Inputs

`conformance/migration/baseline.json` (frozen) + `conformance/migration/mapping.json` (hand-authored) + the registry (`cases.json`, `behaviors/*.json`, `channels.json`, `waivers/`, `extensions.json`, `residuals.json`).

## Structure

```jsonc
{
  "schemaVersion": 1,
  "baselineRevision": "98c26a5",
  "totals": {
    "before": { "units": 111, "behaviors": 25, "channels": 11, "fixtures": 37, "exceptions": 16 },
    "after":  { "cases": 0, "variants": 0, "behaviors": 0, "channels": 0, "fixtures": 0, "exceptions": 0 }
  },
  "accounting": {
    "migrated": 0, "deduplicated": 0, "residual": 0, "retired": 0,
    "unaccounted": [ { "unit": "…", "program": "…", "assertion": "…" } ]
  },
  "errorPaths":  { "before": 0, "preserved": 0, "weakened": [ { "unit": "…", "lost": "direction|diagnostic" } ] },
  "deduplication": [ { "behavior": "bhv-…", "absorbedUnits": ["…"], "cases": ["case-…"], "rationale": "…" } ],
  "residualQueue": [ { "id": "res-…", "units": ["…"], "blockedCarrier": "…", "missingCapability": "…", "followUp": "…" } ],
  "retired":     [ { "unit": "…", "rationale": "…" } ],
  "strictnessImprovements": [ { "unit": "…", "detail": "…", "characterizedAs": "case-…|wvr-…|issue" } ],
  "deletedCarriers": [ "parity_corpus_tier1" ],
  "deletableCarriers": [ "parity_corpus_merged" ],
  "deletionBlockers": [ { "carrier": "…", "reason": "…" } ],
  "violations": []
}
```

## Failure conditions

Each failure names the specific item, its origin program, and what it asserted — never an aggregate count.

| # | Condition | Requirement |
|---|---|---|
| 1 | `accounting.unaccounted` non-empty | FR-040, FR-041 |
| 2 | A before-behavior, channel, fixture, or characterized exception with no counterpart | FR-041 |
| 3 | `errorPaths.weakened` non-empty — a rejection lost its direction or diagnostic expectation | FR-042 |
| 4 | `totals.after.behaviors > totals.before.behaviors` | SC-005 — denominator inflation |
| 5 | `migrated + deduplicated + residual + retired ≠ before.units` — every unit needs exactly one disposition; raw case count is NOT the measure, since a residual conserves coverage without producing a case (FR-013) | SC-005 |
| 6 | A `retired` entry with no rationale | FR-040 |
| 7 | A `strictnessImprovements` entry with no `characterizedAs` | FR-036 — suppressed rather than characterized |
| 8 | Regeneration is not byte-identical | FR-043, SC-012 |

## Determinism (FR-043)

No timestamps, no absolute paths, no hostnames; arrays sorted by stable key. The Markdown rendering is a pure function of the JSON. The report is reviewable as a version-controlled diff even though the file itself is generated.

## Anti-gaming (FR-045)

The report reads the baseline; it never writes it. A baseline edit is a separate, reviewable diff requiring justification. Lowering the baseline to make the report pass therefore surfaces as a visible baseline diff, not as a green report.

> The original wording also cited `baseline check` (V25) as a second, automatic line of defence. That gate is **retired** (T099): regeneration and carrier deletion are mutually exclusive, since `baseline generate` enumerates live carriers and a deleted carrier necessarily drops its units. Anti-gaming now rests on the diff being reviewable, which was always the load-bearing half — `conformance/migration/baseline.json` is frozen evidence, and `baseline_archive.rs` checks its internal integrity without regenerating it.

## Deletion linkage

`deletableCarriers` lists carriers where every unit is `equivalent`/`stricter` in the equivalence ledger **and** no residual names the carrier as `blockedCarrier`. A carrier absent from this list may not be deleted (FR-034, FR-037).

Two fields were added to the shape above during implementation, both because the single list could not express the state the reviewer actually needed:

- `deletionBlockers` — the contract said which carriers may go but not *why* the rest may not, and "why" is the actionable half. Note that a carrier can be equivalence-clean and still blocked: `parity_up_exec` carries the only evidence for `bhv-exec-container-id-metadata`, so deleting it would trade a green ledger for a V5 uncovered behavior.
- `deletedCarriers` — a carrier that has already been deleted is absent from the live registry and therefore from both lists, so a report over a completed deletion read "No carrier is deletable yet".
