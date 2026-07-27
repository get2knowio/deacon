# Contract: injected-regression harness

**Command**: `cargo run -p parity-harness --bin coverage-regressions`
**Test binary**: driven under `--profile parity` only.

Proves each observable channel is **live** — that a difference visible on that channel turns
the suite red. A green suite whose channels are inert is worse than no suite, because it is
trusted.

## The injection point

A regression is applied to the **evidence source**: the completed process result, the
`docker inspect` document, or the file bytes — *before* any observer runs.

```text
        ┌──────────┐   raw artifact    ┌──────────┐   evidence   ┌─────────┐
system ─┤ capture  ├──────────────────►│ observer ├─────────────►│ compare │
        └──────────┘         ▲         └──────────┘              └─────────┘
                             │
                    INJECT HERE (legal)        INJECT HERE (forbidden — FR-065b)
```

**Why not one step later.** Perturbing what an observer *returns* would let a dead observer —
one that ignores its input and always returns empty — appear live: the perturbed return value
differs, so the channel reports `detected` while observing nothing. Injecting upstream closes
this. A dead observer ignores the perturbed source, returns its usual value, no difference
appears, and the channel is correctly reported `inert`.

**Why not source mutation.** One edit to deacon perturbs several channels or none, so it
cannot target a channel; it needs a rebuild per mutation; and it risks leaving a dirty tree,
which FR-066 forbids.

## Perturbation kinds

A closed set. Each is declarative, reversible, and applies to a named source.

| Kind | Applies to | Effect |
|---|---|---|
| `set-json-pointer` | inspect documents, structured output | Sets one pointer to a literal |
| `remove-json-pointer` | inspect documents, structured output | Removes one pointer |
| `set-exit-code` | process result | Replaces the exit status |
| `append-bytes` | stdout, stderr, file content | Appends a marker |
| `remove-path` | filesystem listing | Drops one entry |

A perturbation MUST be reverted by the harness on success **and** on unwind. The tree is
verified unmodified after the run (FR-066).

## Verdicts

| Verdict | Condition |
|---|---|
| `detected` | ≥1 case failed, and the failure is attributed to the record's `channel` |
| `inert` | Every record for the channel went undetected |

Attribution matters: a case that fails for an unrelated reason does **not** count as
detection. The reported failure must name the channel under test, or the record is inert.

## Exit codes

| Exit | Meaning |
|---|---|
| `0` | Every declared channel has ≥1 `detected` record |
| `1` | ≥1 channel is `inert`, or a regression could not be reverted, or V30 |

An inert channel is a **failure**, not a warning (FR-067). The whole point is that this run is
the only thing standing between a dead channel and a trusted green suite.

## Isolation and safety

| Requirement | Contract |
|---|---|
| Never in the ordinary case set | Regressions live in their own binary; a normal run cannot apply one (FR-070) |
| Never leaves a regression applied | Revert on success and unwind; post-run tree verification |
| Reproducible | Same inputs → same `detected`/`inert` classification (FR-069) |
| Fail-loud prerequisites | Missing Docker or oracle fails with a cause-specific error, never a skip |

## Report

`target/conformance/regressions.json` — byte-stable, git-ignored:

```jsonc
{
  "schemaVersion": 1,
  "channels": [
    { "channel": "chan-image", "verdict": "detected",
      "records": [ { "id": "reg-chan-image-label", "detectedBy": ["case-build-image-metadata-labels"] } ] },
    { "channel": "chan-file-content", "verdict": "inert", "records": [ ... ] }
  ],
  "inertCount": 1
}
```

`inertCount` is the number SC-006 requires to be zero.
