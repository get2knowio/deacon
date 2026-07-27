# Phase 1 Data Model: Exploratory Parity Discovery

**Feature**: `025-exploratory-parity-discovery`
**Date**: 2026-07-27

All records are strict JSON. Unknown fields are rejected at load (matching every other record
kind in this repository). All writes are atomic — serialize to a unique temp file, then
`fs::rename` into place.

Two data roots, deliberately separate (research D6, D11):

| Root | Ownership | Reachable from `certify`? |
|---|---|---|
| `conformance/discovery/` | machine-produced + hand-triaged | **No** — sibling of `registry/`, no loader path reaches it |
| `conformance/registry/metamorphic.json` | hand-authored | Yes — it is an assertion the project makes |

---

## 1. Identity and hashing

All ids are substance-anchored using the existing `hash8` helper (SHA-256 truncated to 8 hex
chars), the same primitive behind `clu-` and `cst-` ids. Substance-anchoring means a record that
is reordered, re-annotated, or moved keeps its id; only a change to the thing itself changes it.

| Prefix | Entity | Hashed substance |
|---|---|---|
| `sig-` | Normalized signature | `channel ‖ path ‖ kind ‖ valueShapeClass` |
| `fnd-` | Finding | its `signature` (1:1 with signature; the finding *is* the signature's record) |
| `wit-` | Witness | `campaignId ‖ candidateId` |
| `cmp-` | Campaign | `seed ‖ canonical(pinnedInputSet) ‖ lane ‖ profile` |
| `cnd-` | Candidate input | `canonical(document) ‖ canonical(operations)` |
| `mop-` | Mutation operator | its declared name (stable, hand-assigned, not hashed) |
| `mrl-` | Metamorphic relation | its declared name (stable, hand-assigned, not hashed) |
| `cor-` | Corpus entry | `repository ‖ commit ‖ path` |

**Why `fnd-` is derived from the signature and not independently assigned**: FR-030 makes the
signature the deduplication key, so two findings with the same signature *are* one finding. If
ids were independently assigned, the invariant would have to be maintained by the merge logic
and could be violated by a bad merge. Deriving the id makes duplicate findings unrepresentable.

---

## 2. Normalized signature

The deduplication key (spec clarification Q2, research D3). Computed from
`parity_harness::normalize::diff`'s existing `ConfigDivergence` output — never re-derived.

```
Signature {
  id:              "sig-<hash8>"
  channel:         string        // one of the 11 declared channels
  path:            string        // ConfigDivergence::path, verbatim
  kind:            "ref-only" | "deacon-only" | "value"     // DiffKind::as_str()
  valueShapeClass: "present-absent" | "type-changed" | "ordering-changed" | "value-changed"
}
```

### Value-shape classification

The only new derivation. A pure function of the divergence:

| `DiffKind` | Condition | `valueShapeClass` |
|---|---|---|
| `RefOnly` / `DeaconOnly` | always | `present-absent` |
| `Value` | the two JSON types differ | `type-changed` |
| `Value` | both arrays, and one is a permutation of the other | `ordering-changed` |
| `Value` | otherwise | `value-changed` |

`ordering-changed` is classified before `value-changed` and is a distinct class rather than a
subcase, because declaration-order defects are a known recurring family in this codebase
(`BTreeMap` where the spec requires declaration order). Folding them into `value-changed` would
merge an order defect with an unrelated value defect at the same path.

**Concrete observed values never enter the signature.** They are retained on the witness, where
they are evidence rather than identity.

**Validation**: a signature whose `channel` is not in `channels.json` is **D1**.

---

## 3. Finding — `conformance/discovery/findings.json`

One record per distinct signature. Machine-created, hand-triaged.

```
Finding {
  id:              "fnd-<hash8>"          // derived from signature.id
  signature:       Signature
  witnesses:       [Witness]              // >= 1, declaration-ordered by first observation
  classification:  Classification | null  // null == untriaged (the visible bucket, FR-029)
  state:           FindingState
  firstObserved:   "cmp-<hash8>"          // campaign that first admitted it
  lastObserved:    "cmp-<hash8>"          // most recent campaign that reproduced it
  promotedTo:      "case-<id>" | null     // set only in state `promoted`
  splitFrom:       "fnd-<hash8>" | null   // provenance when a reviewer splits a merged finding
  notes:           string                 // reviewer prose; excluded from every hash
}
```

### Classification (FR-028) — closed set, exactly one

| Value | Meaning | Promotable? |
|---|---|---|
| `deacon-regression` | deacon is wrong; the reference and/or spec is right | Yes — becomes a behavior + a fix |
| `reference-quirk` | the reference diverges from the spec; deacon is right | Yes — behavior + waiver |
| `spec-ambiguity` | the spec does not decide the question | Yes — behavior with `spec: unspecified` |
| `unsupported-behavior` | deacon lacks the capability entirely | Yes — becomes a `gap-` record |
| `normalizer-defect` | the comparison machinery manufactured or hid the difference | **No** (FR-035) |
| `fixture-defect` | the generated input was invalid in a way the harness cannot express | **No** (FR-035) |

The last two are non-promotable because they describe a defect in the discovery machinery, not a
behavior of either implementation. Resolving them changes the normalizer or the generator.

### State machine

```
                    admitted by a campaign
                             │
                             ▼
                      ┌─────────────┐
                      │ untriaged   │  classification == null
                      └──────┬──────┘
                    reviewer assigns a classification
                             │
                             ▼
                      ┌─────────────┐
        ┌─────────────│   triaged   │─────────────┐
        │             └──────┬──────┘             │
        │                    │                    │
   reviewer splits    reviewer promotes    difference stops
        │                    │              reproducing
        ▼                    ▼                    ▼
  ┌───────────┐        ┌───────────┐    ┌──────────────────────┐
  │   split   │        │ promoted  │    │ no-longer-reproducing│
  └───────────┘        └───────────┘    └──────────────────────┘
   (children              terminal;       terminal-ish; a later
    carry splitFrom)      promotedTo      campaign may revive it
                          is set          back to `triaged`
```

Rules the transitions encode:

- **`untriaged` is visible, never implicit.** FR-029 requires a counted bucket, so "not yet
  looked at" can never read as "nothing found".
- **`no-longer-reproducing` is a state, not a deletion** (FR-033). The disappearance is
  information: it may mean a fix landed, or it may mean the generator stopped reaching the input.
  Deleting the record destroys the ability to tell those apart.
- **A `split` finding's children carry `splitFrom`** and the deduplication rule must not re-merge
  them (FR-032). Enforcement: signature-equality merging skips any finding with a non-null
  `splitFrom` chain reaching the same ancestor.
- **`promoted` requires `promotedTo` to resolve** to a real case (**D3**), so the queue can never
  claim coverage that does not exist.
- **`normalizer-defect` / `fixture-defect` cannot reach `promoted`** (**D2** guards the
  classification arity; the promotion path rejects these two by construction).

### Witness

```
Witness {
  id:            "wit-<hash8>"
  campaignId:    "cmp-<hash8>"
  candidateId:   "cnd-<hash8>"
  minimalInput:  <JSON document>     // the reduced fixture
  isMinimal:     boolean             // false => budget exhausted (FR-022)
  reductionSteps: [string]           // ordered catalogue step names applied
  observedValues: { deacon: <value|null>, reference: <value|null> }
  mutationOperators: ["mop-<name>"]  // FR-009 attribution
}
```

Witnesses are retained per finding (FR-032) so a merge can be reviewed and reversed. The concrete
observed values live here — evidence, not identity.

---

## 4. Campaign — `conformance/discovery/campaigns.json`

Provenance for every run. Append-only; a campaign record is never rewritten.

```
Campaign {
  id:              "cmp-<hash8>"
  seed:            string             // hex, the recorded seed (FR-001)
  lane:            "scheduled" | "invoked"
  tier:            "metamorphic" | "config-differential" | "container-differential" | "corpus"
  pinnedInputSet:  PinnedInputSet
  budget:          Budget
  outcome:         CampaignOutcome
}

PinnedInputSet {                       // all SEVEN elements required (FR-002)
  schemaPin:            string         // conformance/schemas/<pin>
  prosePin:             string         // conformance/spec/<pin>
  oracleVersion:        string         // exact, verified (FR-003)
  normalizerVersion:    string         // NORMALIZER_VERSION
  grammarVersion:       string         // constraints.json revision + fingerprint (research D1)
  mutationCatalogVersion: string       // the mutation OPERATOR set
  generatorVersion:     string         // PRNG algorithm identity + reduction-catalogue ORDER
}

Budget {
  wallClockSeconds:     integer        // 1800 for scheduled (research D10)
  perCandidateSeconds:  integer        // 60 hermetic / 300 container-backed
  shrinkStepsPerFinding: integer
  admissionCap:         integer        // 25 (research D10)
}

CampaignOutcome {
  candidatesGenerated:      integer
  candidatesExecuted:       integer
  candidatesDiscardedUnsafe: integer   // FR-011
  parseStageFailures:       integer    // numerator for the SC-002 ratio
  budgetExhausted:          boolean    // FR-005
  spaceCoveredFraction:     number     // reported when budgetExhausted (FR-005)
  mutationApplications:     { "<category>": integer }   // FR-010, all 11 keys always present
  signaturesObserved:       integer
  signaturesAdmitted:       integer
  signaturesSuppressed:     integer    // FR-034b — never silent
}
```

**`mutationApplications` always carries all eleven category keys**, including zeroes. A category
absent from the map is indistinguishable from a category that was never applied; FR-010 requires
zero to be reported as an explicit generation deficiency, which needs the key present.

**`grammarVersion` binds the campaign to the constraint inventory** (research D1). A re-vendored
schema pin regenerates the inventory, which changes this string, which correctly invalidates
every finding bound to the old value (Assumption 8) with no separate bookkeeping.

**`generatorVersion` covers the two things that determine output but are neither a grammar nor a
mutation**: the pseudorandom stream's algorithm identity (research D2) and the reduction
catalogue's *order* (§ 6). Both are reproducibility-critical — FR-001 depends on the stream,
FR-020 on the order — and folding either into `mutationCatalogVersion` would name it for
something it is not, so a deliberate change to reduction order would look like a change to the
mutation operators.

**Validation**: any `pinnedInputSet` element naming a revision absent from `revisions.json` is
**D5**.

---

## 5. Mutation operator catalogue (in code, not data)

Eleven categories mandated by FR-008. The catalogue lives in `mutate.rs` rather than as a data
file because each operator is executable logic, and `mutationCatalogVersion` pins its identity.

| Category | Operator sketch | Grammar input (research D1) |
|---|---|---|
| `unknown-field` | insert a key absent from the schema at a pointer | `additional-properties`, `property-existence` |
| `wrong-type` | replace a value with one of a different JSON type | `type` |
| `null-value` | replace a value with `null` | `type`, `value-shape` |
| `empty-value` | empty a collection or string in place | `array-shape`, `type` |
| `conflicting-source` | add a second config source (image + Dockerfile + compose) | `union-alternative` |
| `invalid-feature-id` | corrupt a Feature identifier's registry/path/tag shape | `property-existence` under `features` |
| `extends-cycle` | introduce a cycle into an `extends` chain | `property-existence` |
| `substitution-edge` | nest, self-reference, or leave unterminated a `${…}` token | `type` (string-valued fields) |
| `lifecycle-shape` | switch between the permitted string/array/object forms | `union-alternative` |
| `compose-combination` | vary service count, `runServices`, override-file ordering | `union-alternative`, `array-shape` |
| `ordering-change` | permute a declaration-ordered collection | `array-shape` |

Each application records its `mop-<name>` on the witness (FR-009), which is what lets a candidate
name the operators that produced it and what lets shrinking un-apply one operator as a reduction
step (research D5).

---

## 6. Reduction step catalogue (in code, ordered)

Ordered because greedy reduction is order-sensitive and FR-020 requires the same finding and seed
to yield the identical minimal input. The order is part of **`generatorVersion`** — not
`mutationCatalogVersion`, which names the mutation operator set and would misdescribe it.

1. `drop-optional-key` — remove a key the grammar does not mark `required`
2. `un-apply-mutation` — reverse one recorded mutation operator
3. `empty-collection` — replace a non-empty array/object with an empty one
4. `collapse-extends-level` — inline one `extends` parent and remove the link
5. `drop-compose-service` — remove one service not referenced by `service`/`runServices`
6. `minimize-scalar` — replace a scalar with the schema-minimal value of its own type
7. `drop-feature` — remove one entry from `features`

Every step keeps the intermediate schema-plausible, so almost every probe is informative
(research D5). `isMinimal` is true only when all seven have been applied once with no step
preserving the signature — which makes FR-021 a finite, checkable claim.

---

## 7. Metamorphic relation — `conformance/registry/metamorphic.json`

Hand-authored, **inside** the registry, because a relation is an assertion the project makes and
references `clu-`/`bhv-` ids only the registry loader resolves (research D11).

```
MetamorphicRelation {
  id:             "mrl-<name>"
  transformation: string      // what is applied to the input
  effect:         "invariance" | "sensitivity"
  ground:         "clu-<hash8>" | "bhv-<name>"    // required (FR-045)
  channels:       ["chan-…"]  // channels the relation asserts over
  rationale:      string
}
```

### Mandated relation families (FR-044)

| `mrl-` | Effect | Transformation |
|---|---|---|
| `mrl-formatting-invariance` | invariance | reindent, rewrap, change insignificant whitespace |
| `mrl-comment-invariance` | invariance | insert JSONC comments and trailing commas |
| `mrl-key-order-invariance` | invariance | permute keys within an unordered map |
| `mrl-path-relocation` | invariance | relocate the workspace to a different absolute path |
| `mrl-lifecycle-equivalence` | invariance | switch between equivalent lifecycle command forms |
| `mrl-extends-flattening` | invariance | replace an `extends` chain with its hand-flattened equal |
| `mrl-declaration-order-sensitivity` | **sensitivity** | permute a declaration-ordered collection |

The last is the only sensitivity relation in the mandated set and it is the one that catches what
the differential cannot: if deacon and the reference *both* wrongly ignore declaration order, the
differential is clean and the defect is invisible. A sensitivity relation asserts the result
**must** change, so consistent-wrongness fails it.

`mrl-path-relocation` compares modulo the declared path tokenization (FR-046) and reports any
residual the tokenization does not account for — which makes it a live check on the tokenizer as
well as on deacon.

**Validation**: unresolvable `ground`, unknown `effect`, or a duplicate transformation is **V31**;
a mandated family with no record is **V32**.

---

## 8. Corpus entry — `conformance/discovery/corpus.json`

The 33 pinned entries, moved out of the Python fetcher into Rust-owned strict JSON so the
immutable-reference check runs hermetically (research D8).

```
CorpusEntry {
  id:             "cor-<hash8>"
  name:           string
  repository:     string     // "owner/repo"
  commit:         string     // 40-hex, immutable — a branch or tag is D4
  path:           string     // workspace root within the repository
  contentDigest:  string | null   // null until first materialization; verified thereafter
  notes:          string
}
```

**`commit` must be a 40-hex object name.** A branch name, a tag, `HEAD`, or `latest` is **D4**,
rejected at load — hermetically, on every PR, without network access. This is the point of moving
the manifest into Rust: a validation that only runs when the network is up is a validation that
does not run.

**`contentDigest` is null exactly once**, at first materialization. Every later fetch verifies it
(FR-051); a mismatch fails that entry loudly rather than comparing against unexpected content. A
non-null digest that goes missing on re-authoring is **D4**.

Corpus content is never vendored (FR-053) — this file records provenance, not bytes.

---

## 9. Reviewable candidate (generated, `target/discovery/candidates/<fnd-id>/`)

Not version-controlled. Assembled per FR-024 from records above, self-contained per FR-027.

```
target/discovery/candidates/<fnd-id>/
├── fixture/            # the minimal fixture, as a materializable workspace tree
├── context.json        # operations + argv; the campaign and seed; pinnedInputSet
├── raw.json            # both sides' evidence, unnormalized  ← separate from normalized
├── normalized.json     # both sides' normalized evidence + the diff
├── provenance.json     # oracle version + verification result; mutation operators applied
└── mapping.json        # suggested behavior mapping, or an explicit no-match
```

`raw.json` and `normalized.json` are separate files, mirroring the committed-snapshot layout — raw
and normalized evidence must never be conflated (FR-014, the FR-016 precedent from 022).

`mapping.json` carries either a resolvable `bhv-` id or `{"match": "none"}`. It never invents an
id (FR-025): a suggestion that fabricates a behavior identity would make the reviewer's job
verifying a plausible-looking id rather than deciding one.

---

## 10. Relationships

```
Campaign ─1:N─> Candidate ─produces─> Divergence ─classified─> Signature
                                                                   │ 1:1
                                                                   ▼
                       Witness ─N:1─> Finding ──promotedTo──> Case (registry)
                          │                                        ▲
                          └── references Campaign + Candidate      │
                                                                   │
MetamorphicRelation ──ground──> Clause | Behavior ─────covered by──┘
CorpusEntry ─materializes─> Candidate  (network lane only; never a mutation seed, FR-008a)
```

The one arrow that crosses the root boundary is `Finding.promotedTo → Case`, and it points
**out** of the discovery root into the registry. Nothing in the registry points back. That
asymmetry is what makes the queue unreachable from `certify` (research D6): following references
from the registry can never arrive at a finding.

---

## 11. Validation summary

Registry-side, emitted by `conformance validate`, blocking a PR via `registry_valid`:

| Class | Statement |
|---|---|
| **V31** | metamorphic relation integrity: unresolvable `ground`, unknown `effect`, duplicate transformation |
| **V32** | a mandated relation family (FR-044) has no relation record |

Discovery-side, emitted by `conformance discovery check`, blocking a PR via a new hermetic test:

| Class | Statement |
|---|---|
| **D1** | malformed queue record, or a signature naming an undeclared channel |
| **D2** | a finding with zero or more than one classification while in state `triaged` or later |
| **D3** | a `promoted` finding whose `promotedTo` does not resolve to a real case |
| **D4** | a corpus entry with a non-immutable reference, or a digest that was recorded and then removed |
| **D5** | a finding or campaign whose `pinnedInputSet` names a revision absent from `revisions.json` |

D-classes are numbered separately from the V-series on purpose: they are emitted by a different
command over a different data root. Folding them into V-numbering would imply the registry
validator can see the queue, which research D6 says it must not.
