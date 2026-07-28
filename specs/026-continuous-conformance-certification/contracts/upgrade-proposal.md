# Contract: Stable Oracle Upgrade Proposal

The reviewed change that authorizes advancing the stable oracle pin. There is no other authorization path,
and no automated process may advance the pin (FR-028, SC-006).

**Produced by**: `cargo run -p parity-harness --bin oracle-upgrade-propose --from <v> --to <v>`
**Validated by**: `cargo run -p deacon-conformance -- drift proposal check`
**Path**: `target/drift/upgrade-proposal.{json,md}` — git-ignored; the bundle is review input, not a record.

Schema in data-model §5.

## The seven sections

All seven keys MUST be present (FR-029). Each is a distinct question a reviewer must be able to answer before
accepting the upgrade:

| Section | Answers |
|---|---|
| `schemaDrift` | Did the pinned schema documents change, and which inventory constraints are affected? |
| `specificationDrift` | Did the pinned normative prose change, and which clauses are affected? |
| `cliSurfaceDrift` | Did the reference CLI's flags, subcommands, or output shapes change? |
| `referenceBehaviorDrift` | Where does the candidate reference behave differently from the current one? |
| `snapshotDifferences` | Which committed snapshots would change, and how? |
| `newlyFailingCases` | Which cases pass today and fail against the candidate? |
| `affectedDispositions` | Which behaviors, waivers, and gaps would need re-review? |

## Present-but-empty vs missing

`"entries": []` means **investigated, nothing found**. A missing key means **not investigated** and is
`V33-incomplete`.

This distinction is the contract's load-bearing property. Collapsing them would let an analysis that never
ran read as a clean result — the same defect the coverage model found twice (a `jsonSubset: {}` assertion
that matched anything, and a `contains` assertion that could not see appended output). An unrun section must
never be indistinguishable from a clean one.

## Rejection

A bundle missing any section is rejected and **cannot authorize an upgrade** (FR-030). Rejection happens in
the hermetic lane, so an incomplete bundle is caught on a PR without provisioning two oracles.

## Determinism

Regenerating from the same before/after pins MUST produce byte-identical output (FR-031, SC-007):

- no timestamp, no absolute path, no hostname;
- entries sorted by a stable key, never by discovery order;
- `inputState.registryDigest` records what the bundle was computed from, so a proposal built against a dirty
  working tree is recognizable as such (spec edge case).

## Acceptance

Accepting a proposal is a human act with three parts, in one reviewed change:

1. Advance the pin in `conformance/registry/revisions.json` and `fixtures/parity-corpus/oracle.json`.
2. Re-record affected snapshots through the **reviewed record path** (`conformance-snapshot refresh`) — the
   git diff is the review surface (FR-032).
3. Update affected dispositions, waivers, and gaps as the `affectedDispositions` section indicates.

No step is automatable, and none is performed by `oracle-upgrade-propose`.

## Canary evidence

A canary run may inform the decision but MUST NOT support it unless every input was pinned by immutable
identifier and the run was hermetic (FR-033). Otherwise the bundle records it as informational only, and the
`referenceBehaviorDrift` section must not cite it as evidence.
