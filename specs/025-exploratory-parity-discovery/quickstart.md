# Quickstart: Exploratory Parity Discovery

**Feature**: `025-exploratory-parity-discovery`

Five workflows: run a campaign, triage what it found, promote a finding, add a metamorphic
relation, and re-pin the real-world corpus. Everything here is **development-only** — none of it
is a `deacon` subcommand.

---

## Prerequisites by tier

| Tier | Oracle + Node | Docker | Network |
|---|---|---|---|
| `metamorphic` | — | — | — |
| `config-differential` | required | — | — |
| `container-differential` | required | required | — |
| `corpus` | required | — | required |

Provision the pinned oracle:

```bash
npm install -g @devcontainers/cli@$(jq -r .version fixtures/parity-corpus/oracle.json)
```

A missing or wrong-version oracle **fails the campaign loudly**. There is no skip.

---

## 1. Run a campaign

```bash
# The cheapest useful run — no oracle, no Docker, no network.
cargo run -p parity-harness --bin discovery-campaign -- \
  --seed 0x5eed1234 --tier metamorphic

# The scheduled tier.
cargo run -p parity-harness --bin discovery-campaign -- \
  --seed 0x5eed1234 --tier config-differential --budget-seconds 1800

# Or via the profile, which runs every registered discovery binary.
make test-discovery
```

`--seed` is required and never defaulted. Record it — it is the reproducibility input, and a
finding you cannot re-derive is a finding nobody can act on.

**Reading the outcome.** The campaign prints a single JSON document to stdout:

```json
{
  "id": "cmp-9f3a2b71",
  "seed": "0x5eed1234",
  "outcome": {
    "candidatesGenerated": 4820,
    "parseStageFailures": 191,
    "signaturesObserved": 7,
    "signaturesAdmitted": 7,
    "signaturesSuppressed": 0,
    "mutationApplications": { "unknown-field": 512, "wrong-type": 498, "...": 0 }
  }
}
```

Three numbers to check before anything else:

- **`parseStageFailures / candidatesGenerated` must be under 10%** (SC-002). Above that, the
  generator is producing garbage and the campaign explored the parser rather than the tool.
- **A zero in `mutationApplications`** is a named generation deficiency (FR-010), not a
  non-event. Some category never successfully applied.
- **`signaturesSuppressed > 0`** means the admission cap was hit. The campaign still exited `0` —
  suppression is reported, never silent, and never fails the run.

**A campaign exits `0` whether it finds nothing or forty things.** Non-zero means the machinery
failed: an unverifiable oracle, a normalization failure, an unwritable data root.

---

## 2. Triage the queue

```bash
cargo run -p deacon-conformance -- discovery report
$EDITOR target/discovery/queue.md
```

The report separates states that are easy to conflate:

| Bucket | Means |
|---|---|
| `untriaged` | nobody has looked yet — **counted**, so it never reads as "nothing found" |
| `triaged` | classified, awaiting a decision |
| `no-longer-reproducing` | stopped reproducing; names the campaign that last saw it |
| `promoted` | now carried by a real case, named |
| `pin-stale` | observed under pins that no longer match; awaiting re-evaluation |

Classify one:

```bash
cargo run -p deacon-conformance -- discovery triage fnd-3c9e11a4 \
  --classification deacon-regression \
  --notes "extends child overrides parent remoteUser; reference keeps the parent"
```

**Two classifications are dead ends by design.** `normalizer-defect` and `fixture-defect` describe
a defect in the discovery machinery, not a behavior of either implementation, and cannot be
promoted (FR-035). Fix the normalizer or the generator instead.

**When to reach for `split`**: a single signature merged witnesses that turn out to have different
causes. Splitting is permanent — the deduplication rule never re-merges a split lineage, so a
reviewer's judgement is not silently reverted by the next campaign.

```bash
cargo run -p deacon-conformance -- discovery split fnd-3c9e11a4
```

---

## 3. Promote a finding

Promotion is **entirely manual**. No command writes into the registry; `scaffold` prints to stdout
and stops.

```bash
cargo run -p deacon-conformance -- discovery scaffold fnd-3c9e11a4
```

That emits a skeleton behavior, case, and fixture layout with `UNREVIEWED` sentinels the loader
rejects. Then, by hand:

1. **Author the behavior** in `conformance/registry/behaviors/<area>.json` with all three axes.
   A finding does not tell you the disposition — it tells you what differs. Deciding whether
   deacon is wrong, the reference is wrong, or the spec is silent is the review.
2. **Copy the minimal fixture** from `target/discovery/candidates/fnd-3c9e11a4/fixture/` into
   `conformance/fixtures/fx-<name>/`.
3. **Author the case** in `conformance/registry/cases/<area>.json` with a **full**
   `scenarioContext` — every scenario dimension assigned, or none (V26).
4. **Flip the `odp-cmb-*` dispositions** the new case covers off `gap`, in the **same commit**.
   Skipping this leaves the coverage report claiming a hole that is filled — the trap 024
   documented.
5. Validate and record the promotion:

```bash
cargo run -p deacon-conformance -- validate
cargo run -p deacon-conformance -- coverage check
cargo run -p deacon-conformance -- discovery check
```

Step 5's third command is what closes the loop: it fails **D3** if the finding claims
`promotedTo` a case that does not exist.

**Tolerating instead of fixing**: author a scoped `wvr-` waiver with a rationale and an `expires`,
then reference it from a scoped `allowedDifferences` entry on the case. Never a blanket scope — a
waiver whose difference stops reproducing must fail as stale, and a blanket one cannot (FR-041).

---

## 4. Add a metamorphic relation

Relations live **in** the registry, because they are assertions the project makes.

```jsonc
// conformance/registry/metamorphic.json — the committed `mrl-comment-invariance` record
{
  "id": "mrl-comment-invariance",
  "transformation": "insert JSONC line comments and trailing commas at every legal position",
  "effect": "invariance",
  "ground": "bhv-readconfig-malformed-jsonc-rejected",
  "channels": ["chan-structured-output"],
  "rationale": "The grounding behavior fixes the parse dialect as JSON with Comments and records that only a hard syntax error is rejected. A comment and a trailing comma are well-formed in that dialect and denote no member, so neither can alter the value the document describes."
}
```

**A `ground` is either a `bhv-` behavior or a `clu-` clause**, and both are in use — but a
`clu-` id is a *substance-anchored slug*, not a short hash: the committed
`mrl-key-order-invariance` grounds on
`clu-json-reference-this-property-allows-you-to-override-the-feature-desc-bebdddcd`. Copy the
real id from `conformance/inventory/clauses.json`; an invented one is V31.

Then:

```bash
cargo run -p deacon-conformance -- validate       # V31/V32
cargo run -p parity-harness --bin discovery-campaign -- --seed 0x1 --tier metamorphic
```

**`ground` is not optional and not decorative.** An ungrounded invariance relation that is wrong
does not fail — it *passes*, asserting something the spec never said. V31 blocks that.

**Consider a sensitivity relation.** Invariance relations catch a tool reading meaning that is not
there. Sensitivity relations catch a tool *ignoring* meaning that is — and they are the only thing
that catches deacon and the reference being consistently wrong together, which the differential
cannot see because both sides agree.

---

## 5. Re-pin the real-world corpus

```jsonc
// conformance/discovery/corpus.json — the committed `images-python` entry
{
  "id": "cor-eb074204",
  "name": "images-python",
  "repository": "devcontainers/images",
  "commit": "31b61b521d55926d62c748b659f24ae71774c0e3",
  "path": "src/python",
  "contentDigest": null,
  "notes": "Dockerfile build with multiple official features."
}
```

```bash
cargo run -p deacon-conformance -- discovery check      # D4, hermetic — no network needed
cargo run -p parity-harness --bin discovery-campaign -- --seed 0x2 --tier corpus
```

- **`id` is derived from the entry's own substance, never hand-authored.** A made-up id is
  **D4** ("a corpus id that does not derive from its own substance"). Add the entry with a
  placeholder, run `discovery check`, and take the id the violation names.
- **`commit` must be a 40-hex object name.** A branch, tag, `HEAD`, or `latest` is **D4**,
  rejected hermetically on every PR. This is why the manifest is Rust-owned data rather than
  Python: a validation that only runs when the network is up is a validation that does not run.
- **`contentDigest: null` exactly once.** First materialization records it; every later fetch
  verifies it. A mismatch fails that entry rather than comparing against unexpected content.
- **Content is never vendored** (FR-053). The manifest records provenance; the bytes stay upstream.

---

## Where things live

| Path | What | Version-controlled? |
|---|---|---|
| `conformance/discovery/findings.json` | the queue | yes — outside the registry |
| `conformance/discovery/campaigns.json` | campaign provenance | yes |
| `conformance/discovery/corpus.json` | 33 pinned entries | yes |
| `conformance/registry/metamorphic.json` | relation catalogue | yes — inside the registry |
| `target/discovery/queue.{json,md}` | rendered report | no |
| `target/discovery/candidates/<fnd-id>/` | reviewable candidates | no |

**The queue is unreachable from `certify` by construction**, not by convention: the registry
loader enumerates named subdirectories under `conformance/registry/`, and `conformance/discovery/`
is a sibling of that directory, not a member. No function walks from one to the other.

---

## Troubleshooting

**"Campaign exits 1 immediately."** A prerequisite failed. Check the oracle's exact version
against `fixtures/parity-corpus/oracle.json` — the harness verifies exactly, not
greater-than-or-equal, and a near-miss version fails.

**"Same seed, different candidates."** Something in the pinned input set moved. Compare the
campaign's recorded `pinnedInputSet` against the current one; a re-vendored schema pin changes
`grammarVersion`, which legitimately changes the stream. This is invalidation working, not a bug.

**"A finding I fixed keeps coming back."** Check its state. If it is `no-longer-reproducing` it is
being *reported*, not re-found — the record is retained deliberately, so that "a fix landed" stays
distinguishable from "the generator stopped reaching that input".

**"`discovery check` fails D5 after a pin bump."** Expected. Findings are claims about a specific
pinned pair of implementations; on a pin change they are re-evaluated, not carried forward. Run a
campaign under the new pins and let each finding reproduce or lapse.

**"The queue has 200 untriaged findings."** Check `signaturesSuppressed` across recent campaigns.
Repeatedly hitting the admission cap usually means one generator change surfaced a systemic
divergence rather than many independent ones — triage a few and look for a shared cause before
working through the list.

**"I ran both CLIs on the candidate's fixture and the outputs look identical."** You are
comparing **raw** output; the signature is defined on **normalized** output. This is the one
step of the SC-017 reproduction that is not self-evident from the candidate, so do it
explicitly:

```bash
# 1. The fixture IS the reproduction input. Copy it somewhere fresh.
cp -r target/discovery/candidates/<fnd-id>/fixture/. /tmp/repro/

# 2. Verify your oracle equals the candidate's `pinnedInputSet.oracleVersion` — exactly.
devcontainer --version

# 3. Run the argv recorded in context.json on both sides.
devcontainer read-configuration --workspace-folder /tmp/repro > ref.json
deacon           read-configuration --workspace-folder /tmp/repro > deacon.json

# 4. Compare the way the pipeline does. `raw.json` and `normalized.json` in the candidate
#    are committed side by side precisely so you can see what the rules changed.
diff <(jq -S . ref.json) <(jq -S . deacon.json)
```

If step 4 shows no difference at the signature's `path` while `normalized.json` does, the
difference is produced by a **named normalization rule**, not by the two CLIs disagreeing.
The usual one is `drop_absent_optional`, which applies to **deacon's `configuration` block
only**: deacon serializes every modeled optional unconditionally, so the rule elides its
empty values — but it cannot distinguish *"the author wrote `"secrets": {}`"* from *"deacon
emitted an unset optional"*. The reference's `configuration` is an echo of the authored
document, so its authored empty survives. A fixture that authors an empty value for any key
in `ABSENT_OPTIONAL_KEYS` therefore yields a `ref-only` signature at that path with
**identical raw output on both sides**.

That is a finding about the observer, not about deacon's behavior — triage it
`normalizer-defect`, which is a deliberate dead end (it cannot be promoted, FR-035). The
underlying fix is on deacon: `skip_serializing_if` so absent optionals are omitted, which
retires the rule entirely (`specs/023-migrate-parity-to-conformance/tasks.md#T111`).
