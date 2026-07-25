# Contract: Equivalence Ledger (equivalent-or-stricter gate)

**Output**: `target/parity/equivalence.json` · **Producer**: `parity-harness --bin equivalence-report` · **Lane**: `--profile parity` only (needs the pinned oracle + Docker)

Deletion of any superseded program is gated on this ledger (FR-033–FR-038). It exists because "the new thing passes" is not evidence — the new thing must not pass where the old thing *failed*.

## Comparison basis (spec A-002)

Compared on the **outcome**, not on message text. Message wording, ordering, and formatting differences are not relations.

| Relation | Definition | Effect |
|---|---|---|
| `equivalent` | Both paths report the same outcome for the unit | Permits deletion |
| `stricter` | The replacement reports a difference the legacy path did not | **Permitted**; reported; the new difference MUST be characterized (FR-036) |
| `more-permissive` | The legacy path reported a difference the replacement does not | **Blocks deletion** (FR-035) until fixed or recorded as an explicit justified accepted difference |

## Record

```jsonc
{
  "unit": "parity_corpus_tier1::node-ts",
  "carrier": "parity_corpus_tier1",
  "legacyOutcome": "pass",
  "replacementOutcome": "diverge",
  "relation": "stricter",
  "detail": "deacon-only key `customizations.vscode.settings` was pruned by the legacy normalizer and is now compared",
  "characterizedAs": "wvr-…|case-…|issue#…"
}
```

`detail` is required for `stricter` and `more-permissive`. `characterizedAs` is required for `stricter` — an uncharacterized new difference is suppression, not an improvement.

## Deletion predicate (FR-034, FR-037)

A carrier is deletable **iff**:

1. Every unit it carries appears in the ledger, **and**
2. every such unit's `relation ∈ {equivalent, stricter}`, **and**
3. no `ResidualRecord` names it as `blockedCarrier`, **and**
4. the coverage report accounts for every unit it carried.

A blocked deletion names the specific unsatisfied condition — which unit, which relation, which residual.

## Expected findings (research D3)

Removing the legacy `prune` is expected to produce a **cluster of `stricter` relations across the 48 corpus units**: `prune` drops every null, empty object, empty array, and empty string value plus `configFilePath`, and ranks `DiffKind::DeaconOnly` lowest as "usually default noise". Differences it was hiding become visible on migration. This is the feature working as intended (FR-036), but each one needs characterizing, so budget for it rather than treating a wave of new differences as a migration regression.

`replace_hex12` (any 12-char lowercase-hex run → `<ID>`) is applied to both sides, so it cannot manufacture a false `equivalent` between differing *documents* — but it can mask a genuine difference between two distinct hex values. Units whose evidence contains hex-shaped data warrant explicit review when their relation is `equivalent`.

## Fail-loud preconditions (Constitution IV)

A missing or mismatched oracle, unavailable Docker, or a normalization failure **fails** the run with a cause-specific error. There is no skip-to-pass and no `#[ignore]`; a ledger that cannot be produced is not an empty ledger.
