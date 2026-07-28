# Contract: Findings Queue

**Feature**: `025-exploratory-parity-discovery`
**Location**: `conformance/discovery/findings.json` — a sibling of `registry/`, **not** inside it

## The unreachability guarantee

The registry loader (`crates/conformance/src/load.rs`) enumerates *named* subdirectories under
`conformance/registry/` — `cases/`, `behaviors/`, `sources/`, `waivers/`, `classifications/`,
`clause-classifications/`, `obligation-dispositions/`. There is no wildcard walk at the registry
root, so a sibling of `registry/` has no code path that reaches it. `certify` consumes the loaded
record only.

This makes the guarantee **structural**, not conventional: an unreviewed finding cannot influence
a release gate because there is no function that could carry it there. The distinction matters
because the failure mode is silent — a finding quietly joining the denominator — which is the
class of mistake 024 D1 documented when a scenario dimension was nearly added to `dimensions.json`.

**One reference crosses the boundary, and it points outward**: `Finding.promotedTo → case-<id>`.
Nothing in the registry points back. Following references from the registry can never arrive at a
finding.

## Record schema

```json
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "fnd-<hash8>",
      "signature": {
        "id": "sig-<hash8>",
        "channel": "chan-structured-output",
        "path": "configuration.remoteUser",
        "kind": "value",
        "valueShapeClass": "type-changed"
      },
      "witnesses": [
        {
          "id": "wit-<hash8>",
          "campaignId": "cmp-<hash8>",
          "candidateId": "cnd-<hash8>",
          "minimalInput": { },
          "isMinimal": true,
          "reductionSteps": ["drop-optional-key", "un-apply-mutation"],
          "observedValues": { "deacon": null, "reference": null },
          "mutationOperators": ["mop-wrong-type"]
        }
      ],
      "classification": "deacon-regression",
      "state": "triaged",
      "firstObserved": "cmp-<hash8>",
      "lastObserved": "cmp-<hash8>",
      "promotedTo": null,
      "splitFrom": null,
      "notes": ""
    }
  ]
}
```

Unknown fields are rejected at load, matching every other record kind here.

## Invariants

| # | Invariant | Enforced by |
|---|---|---|
| Q1 | `id` is derived from `signature.id` — two findings with the same signature are the same record | id derivation (duplicates are unrepresentable) |
| Q2 | `witnesses` is non-empty and declaration-ordered by first observation | D1 |
| Q3 | `signature.channel` resolves in `channels.json` | D1 |
| Q4 | exactly one `classification` once `state` is `triaged` or later; `null` only while `untriaged` | D2 |
| Q5 | `state == "promoted"` ⟺ `promotedTo` is non-null **and** resolves to a real case | D3 |
| Q6 | `classification ∈ {normalizer-defect, fixture-defect}` ⇒ `state != "promoted"` | D2 + the promotion path |
| Q7 | a finding with a non-null `splitFrom` is never re-merged into its ancestor | deduplication skips split lineages |
| Q8 | every `firstObserved` / `lastObserved` resolves in `campaigns.json` | D1 |
| Q9 | every `pinnedInputSet` element on the referenced campaign names a revision in `revisions.json` | D5 |
| Q10 | a finding in state `split` has ≥2 children naming it in `splitFrom`, carries no `classification` of its own, and is never a merge target | D1 + D2 |

**Q10 spelled out**, because the parent's fate after a split is easy to leave undefined: the
parent becomes an inert ancestor. It keeps its witnesses as historical record, stops accepting
new ones, and surrenders classification to its children — a split exists precisely because one
classification could not describe them all. A parent that kept a classification would assert the
judgement the split rejected.

## Deduplication (FR-030 – FR-032)

**Merge rule**: a new divergence whose signature equals an existing finding's signature appends a
witness to that finding. It does not create a record.

**Non-merge rules**, both load-bearing:

- **Distinct signatures stay distinct even when they map to the same behavior** (FR-031). They may
  be *reported* grouped under that behavior, but grouping is a view, not a merge — merging would
  destroy the ability to tell whether a fix addressed one cause or all of them.
- **A split lineage is never re-merged** (FR-032). Without this, a reviewer's judgement that two
  witnesses have different causes is silently reverted by the next campaign, and the split becomes
  unrepeatable work.

**Admission cap** (FR-034b): a campaign admits at most `budget.admissionCap` newly distinct
signatures. Excess signatures are counted in `signaturesSuppressed` and reported. Exceeding the
cap **never** fails the campaign — that would make discovery gate on its own output.

Suppression is always visible. A campaign that repeatedly hits its cap is itself a signal that
something systemic is diverging, and a silent truncation would read as "we found 25 things"
instead of "we found many more than we can review".

## Reproduction lifecycle (FR-033)

A finding that stops reproducing moves to `no-longer-reproducing`, retaining the campaign that
last observed it. It is **not** deleted.

Deleting it would destroy the ability to distinguish two very different situations: a fix landed,
or the generator stopped reaching that input. The first is success; the second is a coverage
regression in the discovery machinery itself. Only the retained record makes them separable.

A later campaign that reproduces it moves it back to `triaged`, keeping its classification —
re-triaging a finding a reviewer already judged would be wasted work.

## Pin invalidation (Assumption 8)

Findings are claims about a specific pinned pair of implementations. On a pin change — a
re-vendored schema surface, a new oracle version, a `NORMALIZER_VERSION` bump — a finding's
`pinnedInputSet` no longer matches the current one.

Such findings are **re-evaluated, not carried forward**. `discovery report` lists them as
pin-stale; the next campaign under the new pins either reproduces them (they return to `triaged`
with a new witness) or does not (they move to `no-longer-reproducing`). Carrying them forward
unchanged would assert that a difference observed against oracle *v0.80* still holds against
*v0.81* — a claim nothing verified.

## Violation classes (`conformance discovery check`)

| Class | Statement |
|---|---|
| **D1** | malformed queue record; empty `witnesses`; a signature naming an undeclared channel; an unresolvable campaign reference |
| **D2** | a finding with zero or more than one classification while in state `triaged` or later; a non-promotable classification in state `promoted` |
| **D3** | a `promoted` finding whose `promotedTo` does not resolve to a real case |
| **D4** | *(corpus)* a non-immutable reference, or a digest recorded then removed |
| **D5** | a finding or campaign whose `pinnedInputSet` names a revision absent from `revisions.json` |

Numbered separately from the registry's V-series because they are emitted by a different command
over a different data root. Folding them into V-numbering would imply the registry validator can
see the queue — which is precisely what this contract exists to prevent.
