# Contract: Metamorphic Relation Catalogue

**Feature**: `025-exploratory-parity-discovery`
**Location**: `conformance/registry/metamorphic.json` — **inside** the registry

## Why relations live in the registry and findings do not

A metamorphic relation is an **assertion the project makes**: "reordering these keys must not
change the result, and here is the clause that says so." It is hand-authored, reviewed, stable,
and references `clu-` / `bhv-` ids that only the registry loader resolves. That is the same kind
of object as an applicability rule.

A finding is a **candidate for an assertion** — machine-produced, unreviewed, possibly wrong.

The split follows from what each thing *is*, not from where it is convenient to put it.

## Record schema

```json
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "mrl-key-order-invariance",
      "transformation": "permute the key order within an unordered JSON object",
      "effect": "invariance",
      "ground": "clu-a1b2c3d4",
      "channels": ["chan-structured-output"],
      "rationale": "Object member order carries no meaning in JSON, and the configuration schema declares no ordered object. A result that changes under key permutation is reading order it must not read."
    }
  ]
}
```

## The two effects

| `effect` | Assertion | Catches |
|---|---|---|
| `invariance` | the transformation MUST NOT change the normalized result | a tool reading meaning that is not there |
| `sensitivity` | the transformation MUST change the normalized result | a tool ignoring meaning that *is* there |

**Sensitivity relations are the ones the differential cannot replace.** If deacon and the
reference both wrongly ignore declaration order, the differential comparison is clean and the
defect is invisible to it — both sides agree, and agreeing is what the differential checks. A
sensitivity relation asserts the result *must* change, so consistent-wrongness fails it.

This is why FR-043 mandates both kinds rather than treating sensitivity as an optional extra.

## Mandated families (FR-044)

Every row must have at least one record, or **V32**.

| Id | Effect | Transformation | Ground kind |
|---|---|---|---|
| `mrl-formatting-invariance` | invariance | reindent, rewrap, alter insignificant whitespace | clause |
| `mrl-comment-invariance` | invariance | insert JSONC comments and trailing commas | clause |
| `mrl-key-order-invariance` | invariance | permute keys within an unordered map | clause |
| `mrl-path-relocation` | invariance | relocate the workspace to a different absolute path | behavior |
| `mrl-lifecycle-equivalence` | invariance | switch between equivalent lifecycle command forms | clause |
| `mrl-extends-flattening` | invariance | replace an `extends` chain with its hand-flattened equal | behavior |
| `mrl-declaration-order-sensitivity` | **sensitivity** | permute a declaration-ordered collection | clause |

## The ground requirement (FR-045)

Every relation MUST name a `ground` that resolves to a normative clause (`clu-`) or a recorded
behavior (`bhv-`). A relation with no ground, or with one that does not resolve, is **V31**.

This mirrors the `ground` that 024 already requires on applicability rules, and for the same
reason: without it, a relation records an author's intuition about what *ought* to be irrelevant.
An ungrounded invariance relation that happens to be wrong does not fail — it *passes*, silently,
while asserting something the spec never said. A grounded one can be checked by reading the clause.

`mrl-path-relocation` and `mrl-extends-flattening` are grounded on behaviors rather than clauses
because both concern resolution mechanics the prose describes operationally rather than in a
single normative sentence.

## Path relocation and the tokenizer (FR-046)

`mrl-path-relocation` compares **modulo the declared path tokenization** and reports any residual
difference the tokenization does not account for.

That residual is the interesting output. A leaked absolute path that the tokenizer misses shows up
here as a relation failure, which means this relation is simultaneously a check on deacon and a
check on the normalizer. A residual difference should therefore be triaged carefully: it is as
likely to be a `normalizer-defect` as a `deacon-regression`, and misfiling it as the latter sends
someone to fix code that is correct.

## Evaluation (FR-048)

Relations are evaluated against **deacon alone** — no oracle, no Docker, no network.

This makes the metamorphic tier the only part of discovery that runs with none of the three
prerequisites. Two consequences worth acting on:

- A contributor with no devcontainer CLI installed can develop and test this story locally.
- It is the cheapest complete vertical slice through generation → comparison → signature →
  candidate, so building it first exercises the entire hermetic spine before any live oracle
  provisioning exists (research D12).

It does **not** license running it in the PR lane. FR-055 is absolute, and its reason is
stochasticity, not resource cost.

## Failure output (FR-047)

A relation failure produces a candidate naming the relation, the transformation applied, both
inputs, and both normalized outputs — the same reviewable-candidate shape as a differential
finding, so both enter one triage pipeline.

The signature for a metamorphic finding uses the relation's channel plus the divergence between
the two *deacon* outputs. Its `kind` and `valueShapeClass` are computed identically, so a
metamorphic finding and a differential finding at the same path deduplicate against each other —
which is correct: they are the same defect observed two ways.

## Violation classes (`conformance validate`)

| Class | Statement |
|---|---|
| **V31** | unresolvable or missing `ground`; unknown `effect`; duplicate `transformation`; a `channels` entry not in `channels.json` |
| **V32** | a mandated relation family (FR-044) has no record |

Both block a PR via the existing hermetic `registry_valid` test.
