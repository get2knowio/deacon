# Phase 0 Research: Exploratory Parity Discovery

**Feature**: `025-exploratory-parity-discovery`
**Date**: 2026-07-27

Every decision below was checked against the code, not inferred. Where a decision rejects a
conventional choice, the rejection reason is a property this feature actually needs, not a
preference.

---

## Decision 1 — The generator's grammar is the committed constraint inventory, not the schemas

**Decision**: Generation draws its grammar from `conformance/inventory/constraints.json`
(609 units at the current pin), not from re-parsing `conformance/schemas/<pin>/*.json`.

**Rationale**: The inventory is already exactly what a constrained generator needs, and it is
already governed. Its non-annotation units carry the generative content:

| Kind | Count | Generative use |
|---|---|---|
| `type` | 187 | the value domain to draw from, and the wrong-type mutation's target set |
| `property-existence` | 117 | which keys may appear at a pointer |
| `array-shape` | 41 | element type, tuple-vs-list, min/max |
| `additional-properties` | 38 | whether an unknown-field mutation is legal or near-valid there |
| `required` | 20 | which keys a *valid* instance must carry — the difference between valid and near-valid |
| `union-alternative` | 18 | the branch set for `oneOf`/`anyOf` shapes (Compose vs Dockerfile vs image) |
| `enum` / `const` | 14 | exact legal values, and the near-miss set one edit away |
| `value-shape` / `default` | 18 | scalar constraints and omission semantics |

Three properties come free by using it:

1. **The grammar pin is already a recorded revision.** `constraints.json` carries a `revision`
   field that V14 validates against the registry schema pin. FR-002 requires the grammar
   version in the pinned input set; it is already there and already guarded.
2. **A schema pin bump automatically surfaces as a generation-input change.** Re-vendoring
   regenerates the inventory, `inventory diff` enumerates the delta, and every finding bound to
   the old revision is correctly invalidated (FR-002, Assumption 8) with no separate bookkeeping.
3. **No second extraction path.** FR-015 forbids a second normalization definition; the same
   argument applies one level up to schema interpretation. Two views of the pinned schema
   surface that could disagree is the identical defect class.

**Alternatives considered**:
- *Re-parse the vendored schemas directly.* Rejected: duplicates the extraction logic that
  `inventory generate` owns, creates an ungoverned second view of the same bytes, and would
  drift silently because nothing cross-checks the two.
- *Hand-author a generation grammar.* Rejected outright — it would re-import the very
  maintainer imagination this feature exists to escape. A hand-written grammar generates the
  shapes its author thought of, which is what curated fixtures already do.

**Consequence for the pinned input set**: `constraints.json`'s `revision` and content
fingerprint are two of the six FR-002 elements.

---

## Decision 2 — The pseudorandom stream is in-repo, not from an external crate

**Decision**: Implement the campaign PRNG inside the discovery module — SplitMix64 seed
expansion feeding a xoshiro256\*\* stream, ~40 lines of pure integer arithmetic, unit-tested
against published reference vectors. No new dependency.

**Rationale**: FR-001 requires that a recorded seed reproduce an identical candidate sequence,
and FR-034 requires findings to persist across campaigns — which means a seed recorded today
must still reproduce after arbitrary dependency updates. `rand` explicitly does **not** offer
value-stream stability across versions; its own documentation treats the stream as an
implementation detail. Depending on it would make every finding's reproducibility hostage to a
`cargo update`, and a security advisory could force a bump that silently invalidates the entire
recorded corpus with no signal.

Making the stream a property of *our* committed code inverts that: the algorithm identity becomes
an explicit component of `generatorVersion` — the **seventh** element of the pinned input set
(FR-002, data-model § 4), which also carries the reduction-catalogue order (D5). A deliberate
change to either is then a recorded, reviewable pin change, exactly like `NORMALIZER_VERSION` and
for the identical reason.

This also matches the workspace's stated posture (023 D6: existing dependencies only) and
carries no `unsafe` (Principle V): xoshiro256\*\* is wrapping integer arithmetic and shifts.

**Alternatives considered**:
- *`rand` + `rand_chacha`, version-pinned.* Rejected: pinning a dependency specifically to
  freeze behavior fights Principle V's dependency-hygiene rule ("keep dependencies current"),
  and still breaks under a forced advisory bump. It trades a maintenance obligation for a
  guarantee it cannot actually deliver.
- *Hash-based derivation (seed + counter → SHA-256 → bytes) using the existing `sha2`.*
  Rejected as the primary stream on cost grounds — a shrink pass makes many thousands of draws
  and hashing each is wasteful — but this is the fallback if the PRNG's test vectors ever prove
  awkward to maintain. Noted, not adopted.

**Complexity note**: this is the one place the plan reimplements something a crate provides.
It is tracked in the plan's Complexity Tracking table with this justification.

---

## Decision 3 — The normalized signature derives from the existing `ConfigDivergence`

**Decision**: Compute the signature from `normalize::diff`'s existing output rather than
introducing a comparison path. The clarified composition (channel + observable path +
difference kind + value-shape class) maps onto what is already there:

```
ConfigDivergence { kind: DiffKind, path: String, deacon: Option<Value>, reference: Option<Value> }
DiffKind = RefOnly | DeaconOnly | Value
```

- **channel** — supplied by the caller (`chan-structured-output`, `chan-exit-code`, …); the
  observers already partition evidence this way.
- **observable path** — `ConfigDivergence::path` verbatim.
- **difference kind** — `DiffKind::as_str()`: `ref-only` / `deacon-only` / `value`.
- **value-shape class** — the one new derivation, a pure function of the two `Option<Value>`s.
  `RefOnly`/`DeaconOnly` classify as `present-absent`. `Value` classifies as `type-changed`
  when the two JSON types differ, `ordering-changed` when both are arrays that are permutations
  of each other, and `value-changed` otherwise.

The signature id is `hash8` over the tuple — the same helper that produces `clu-` and `cst-`
ids, so signature ids are substance-anchored in the same sense: they survive a reordering of
the finding record and change only when the difference itself changes.

**Rationale**: FR-015 forbids a second normalization path, and a signature computed from
independently re-diffed values would be exactly that — a second opinion on what differs, able to
disagree with the one the comparison used. Deriving from the comparison's own output makes
disagreement structurally impossible.

The value-shape class is the level at which "same defect" is true. Structure alone
(channel+path+kind) merges a missing `remoteUser` with a wrongly-typed `remoteUser`; including
concrete values splits one defect across every generated value. `ordering-changed` earns its
place separately because declaration-order defects are a known real class in this codebase
(`BTreeMap`-vs-`IndexMap` violations) and collapsing them into `value-changed` would hide a
family the project has already been bitten by.

**Alternatives considered**:
- *Hash the whole normalized diff.* Rejected: every generated value produces a distinct hash, so
  deduplication does nothing and the queue grows with campaign volume — the failure mode FR-030
  exists to prevent.
- *Channel + path only.* Rejected: merges genuinely distinct defects at the same path, and a
  merged finding cannot be split back into its causes because the distinguishing information
  was never recorded.

---

## Decision 4 — Code splits across the two existing crates along the hermetic/live line

**Decision**: No new workspace crate. Hermetic logic goes in `deacon-conformance`; live
execution goes in `parity-harness`.

| Concern | Crate | Why |
|---|---|---|
| grammar loading, generation, mutation catalogue | `deacon-conformance` | pure data → data; needs the inventory loader that already lives there |
| shrink strategy (which reductions, in what order) | `deacon-conformance` | pure; the *predicate* is injected by the caller |
| signature computation | `deacon-conformance` | pure function over diff output |
| findings-queue model, loader, validation | `deacon-conformance` | strict-JSON records, same shape as every other record kind |
| report rendering | `deacon-conformance` | byte-stable, no I/O beyond the write |
| campaign driver, oracle invocation, evidence capture | `parity-harness` | already owns `exec`/`oracle`/`prereq`/`observe`/`normalize` |
| corpus fetch | `parity-harness` | the only network-touching code |
| injected-difference proof | `parity-harness` | reuses `inject.rs`'s sealed boundary |

**Rationale**: This is the 022 precedent applied unchanged — "the hermetic data/validation/
staleness logic lives in `deacon-conformance`; the live execution/observation/record logic in
`parity-harness`." Following it means the generator, shrinker, and signature are unit-testable
in the fast lane with no oracle, no Docker, and no network, which is what makes FR-055's
hermeticity claim cheap to hold rather than a thing to be careful about.

The shrinker deserves a specific note: it takes the reproduction predicate as a parameter rather
than calling the oracle itself. That keeps the reduction *strategy* hermetic and unit-testable
against a synthetic predicate, while the live campaign supplies the real one. Without this split
the shrinker could only be tested by running a campaign.

**Alternatives considered**:
- *A fifth crate, `deacon-discovery`.* Rejected: nothing needs isolating, and it would need to
  depend on both existing crates to reach `normalize` and the inventory loader — reproducing
  the dependency shape the current split already has, with an extra compilation unit and a
  third place to look for parity vocabulary.
- *All of it in `parity-harness`.* Rejected: the queue records are registry-adjacent data whose
  loader and validator belong with the other loaders, and putting them in the live crate would
  make queue validation require Docker to compile-and-test in practice.

---

## Decision 5 — Shrinking is structural delta-debugging over parsed JSON, not text ddmin

**Decision**: Reduce the parsed configuration document with an ordered catalogue of structural
steps: remove an optional key, empty a collection, collapse one `extends` level, replace a
scalar with the schema-minimal value of its own type, drop a Compose service, un-apply one
mutation operator. Never reduce at the byte or line level.

**Rationale**: Text-level ddmin on JSON produces syntactically broken intermediates. Each one
fails at the document-parse stage, which (a) cannot reproduce a signature that lives past
parsing, so the reduction is wasted, and (b) costs a full oracle invocation to discover — and
the oracle invocation is the expensive step of the entire feature. A campaign whose shrinker
spends most of its budget on malformed intermediates is the same pathology SC-002 guards against
at generation time, relocated to minimization.

Structural steps keep every intermediate schema-plausible, so nearly every probe is informative.
They also make FR-021 checkable: "minimal with respect to the declared catalogue" is a finite,
enumerable claim — apply each of the N steps once, confirm none preserves the signature — rather
than an unfalsifiable assertion about all possible smaller inputs.

The catalogue is ordered and the order is part of `generatorVersion`, because FR-020 requires
the same finding and seed to yield the identical minimal input, and greedy reduction is
order-sensitive.

**Alternatives considered**:
- *`proptest`/`quickcheck` integrated shrinking.* Rejected on three counts: their shrinkers are
  coupled to their generators, so adopting the shrinker means adopting their generation model
  instead of the constraint inventory (killing Decision 1); their shrink order is not stable
  across versions (the Decision 2 problem again); and the shrink predicate here is an expensive
  external process, which their designs assume is cheap.
- *Reduce toward a fixed minimal document rather than by steps.* Rejected: it discards the
  mutation provenance that FR-009 requires the candidate to carry.

---

## Decision 6 — The findings queue sits at `conformance/discovery/`, structurally out of reach

**Decision**: `conformance/discovery/` — a sibling of `registry/`, alongside the existing
`inventory/`, `migration/`, `obligations/`, `snapshots/`, `spec/`, `schemas/`.

**Rationale**: Verified against `crates/conformance/src/load.rs`: the registry loader enumerates
*named* subdirectories under `conformance/registry/` (`cases/`, `behaviors/`, `sources/`,
`waivers/`, `classifications/`, `clause-classifications/`, `obligation-dispositions/`). It has no
wildcard directory walk at the registry root, so a sibling of `registry/` cannot be picked up —
not by convention, but because there is no code path that would reach it. `certify` consumes the
loaded record only.

This makes the clarified guarantee — an unreviewed finding can never influence a release gate —
a property of the directory layout rather than a rule someone must remember. That distinction
matters here specifically: the failure mode is silent (a finding quietly joins the denominator),
which is precisely the class of mistake 024 D1 documented when a scenario dimension was nearly
added to `dimensions.json`.

**Alternatives considered**:
- *Inside `conformance/registry/`.* Rejected: either the loader rejects the unknown collection
  (noisy but survivable) or someone wires it in and unreviewed findings reach `certify`.
- *Git-ignored under `target/`.* Rejected: FR-034 needs cross-campaign persistence and FR-030's
  deduplication needs to see prior campaigns' signatures. A queue that evaporates re-reports
  every known finding on every nightly run.

---

## Decision 7 — The pipeline proof reuses the sealed `EvidenceSource` injection boundary

**Decision**: The FR-042a injected-difference proof injects through
`parity_harness::inject::perturb_source`, the existing sealed-trait entry point.

**Rationale**: `inject.rs` already establishes the property this proof needs and cannot easily
re-establish: the entry points are generic over a **sealed** `EvidenceSource` trait that no
observer output can implement, so injecting into an observer's *return* value does not compile.
A proof that could inject downstream of the comparison would demonstrate nothing about whether
the pipeline works — it would be asserting on data it planted past the part under test. Reusing
the boundary inherits that guarantee instead of re-arguing it.

`InjectionInapplicable` is inherited too, and matters as much: a perturbation that never landed
must fail loudly rather than be counted as "the pipeline found nothing", which is the exact
distinction FR-042a draws.

**What is new**: the assertion target. `coverage-regressions` asserts a *channel verdict* flips
clean → failing. This proof asserts a *pipeline traversal*: the difference surfaces, minimizes to
a stable signature, produces a complete candidate, classifies, and is promotable. That needs its
own verdict type; the injection primitive underneath is shared unchanged.

---

## Decision 8 — The corpus manifest becomes Rust-owned strict JSON; fetching stays in the network lane

**Decision**: Move the 33 pinned entries from `fetch_realworld_corpus.py`'s `ENTRIES` tuple into
a strict-JSON manifest the Rust side loads and validates. Fetching remains network-lane-only.

**Rationale**: FR-050 (reject any non-immutable reference) must be checkable **hermetically** —
it is a property of the manifest, not of a fetch, and a validation that only runs when the
network is up is a validation that does not run on most PRs. That requires the manifest to be
loadable by hermetic Rust code. The entries are already commit-pinned and already inventoried as
`realworld::*` baseline units under `res-realworld-corpus-not-vendored`, so this is a
representation change, not a new coverage claim.

**What is genuinely new**: the per-entry content digest (FR-049/FR-051). The current manifest has
no digest, so first materialization records one and every later run verifies it. This closes a
real hole — GitHub's contents API at a pinned SHA is expected to be stable, but "expected" is not
"verified", and an unverified fetch means comparing against content nobody checked.

**Deliberately left to `/speckit.tasks`**: whether the Python fetcher is retired once the Rust
fetch lands, or kept as an exploratory aid. Both are defensible; it does not affect the design.

---

## Decision 9 — One new nextest profile, with an explicit allow-list

**Decision**: A `discovery` profile whose `default-filter` is an explicit `binary(=…)`
allow-list, plus exclusion of every discovery binary from the `default-filter` of `default`,
`dev-fast`, `full`, `ci`, `mvp-integration`, and `parity`.

**Rationale**: This is the 018 lesson taken verbatim. The parity profile's filter is an explicit
allow-list precisely because a `parity_*` glob wrongly captured the hermetic guards
`parity_harness_faults` and `parity_registry_check`. A `discovery_*` glob would make the same
mistake with the hermetic discovery guards, and the symptom — a hermetic guard silently not
running in the fast lane — is invisible until it matters.

Exclusion from `parity` as well as the PR profiles is deliberate: discovery and live parity
certification answer different questions and have different budgets, and a discovery campaign
inside the parity lane would push it past its window.

**Enforcement**: extend `parity_registry_check` to cover the discovery lane — registry ↔
`tests/*.rs` ↔ `.config/nextest.toml` agreement, and the assertion that `deacon --help` gains
nothing. That check already exists and already fails on drift; FR-057 is asking for exactly its
shape, so extending it is strictly better than adding a parallel checker.

---

## Decision 10 — Two scheduled cadences; the container tier is invoked-only

**Decision**: The 30-minute **nightly** campaign spends its entire budget on the
configuration-resolution tier. The corpus canary runs **weekly** in the network-backed lane. The
container-backed tier is explicitly-invoked only. Admission cap: 25 newly distinct signatures per
campaign.

**On the corpus cadence** (corrected during `/speckit.analyze`): an earlier draft made the corpus
invoked-only alongside the container tier. That contradicted its stated purpose — US7 calls it an
*ecological canary*, and a canary that runs only when someone remembers to invoke it cannot warn
anyone. It gets a weekly schedule of its own. Weekly rather than nightly because the corpus
changes only when someone re-pins it: nightly runs would mostly re-confirm the previous night at
network cost, and the signal being watched for (the ecosystem drifting away from what deacon
handles) moves on the order of weeks, not hours.

**Rationale**: This resolves the item `/speckit.clarify` deferred to planning. At the clarified
per-candidate ceilings — 60s hermetic, 5 minutes container-backed — a single container-backed
candidate can consume a sixth of the scheduled window, so six of them would consume it entirely.
Sharing one budget between tiers lets the slow tier starve the fast one, and the fast tier is
where nearly all the exploration happens (in practice `read-configuration` against the oracle
returns in about a second, so a 30-minute hermetic campaign reaches thousands of candidates
rather than the ~30 the worst-case ceiling implies).

The cap of 25 is set from reviewer throughput, not from machine capacity: a nightly run that
admits more than a couple of dozen genuinely new signatures has produced a backlog nobody clears
before the next run, and per FR-034b the excess is *reported*, not discarded silently — so a
campaign that keeps hitting the cap is itself a visible signal that something systemic is
diverging.

**Alternatives considered**:
- *One budget shared across tiers, weighted.* Rejected: the weighting would need tuning against
  the machine, which makes campaign volume environment-dependent and undermines the
  reproducibility FR-001 asserts.
- *No admission cap, rely on deduplication.* Rejected: deduplication collapses *repeats* of one
  defect; it does nothing about one generator change legitimately surfacing hundreds of distinct
  signatures at once, which is the case that destroys the queue.

---

## Decision 11 — Metamorphic relations are registry data; findings are not

**Decision**: The relation catalogue lives at `conformance/registry/metamorphic.json` as
hand-authored `mrl-` records, validated by the existing `validate` command (new classes V31–V32).
The findings queue lives outside the registry (Decision 6) and is validated by a separate
hermetic `discovery check` command with its own D-class violations.

**Rationale**: The split is principled rather than cosmetic. A metamorphic relation is an
**assertion the project makes** — "reordering these keys must not change the result, and here is
the clause that says so." That is the same kind of object as an applicability rule or a behavior:
hand-authored, reviewed, stable, and referencing `clu-`/`bhv-` ids that only the registry loader
can resolve. FR-045's ground requirement is structurally identical to the `ground` that 024
already requires on `rule-` records, so it gets the same validation treatment.

A finding is a **candidate for an assertion** — machine-produced, unreviewed, possibly wrong. It
must not be able to reach `certify`, and Decision 6 makes that structural.

**Violation classes**:

| Class | Where | Statement |
|---|---|---|
| **V31** | `validate` | metamorphic relation integrity: unresolvable `ground`, unknown `effect`, duplicate transformation id |
| **V32** | `validate` | a mandated relation family (FR-044) has no relation record |
| **D1** | `discovery check` | malformed queue record |
| **D2** | `discovery check` | a finding with zero or more than one classification |
| **D3** | `discovery check` | a promoted finding naming a case that does not resolve |
| **D4** | `discovery check` | a corpus entry with a non-immutable reference or a missing digest |
| **D5** | `discovery check` | a finding whose pinned input set names a revision not in `revisions.json` |

D-classes are numbered separately from V-classes on purpose: they are emitted by a different
command over a different data root, and folding them into the V-series would imply the registry
validator can see the queue, which Decision 6 says it must not.

---

## Decision 12 — Metamorphic evaluation needs no oracle, and that is load-bearing

**Observation, recorded because it shapes sequencing**: FR-048 requires relations to be evaluable
against deacon alone. This makes the metamorphic tier the only part of discovery that runs with
**neither** Node/oracle **nor** Docker **nor** network.

That does not license running it in the PR lane — FR-055 is absolute and the reason is
stochasticity, not resource cost. But it does mean US6 can be built and exercised before any
oracle provisioning exists, and it means a contributor with no devcontainer CLI installed can
still develop and test that story locally. Sequencing should take advantage of this: the
metamorphic tier is the cheapest complete vertical slice through generation → comparison →
signature → candidate, and building it first exercises the whole hermetic spine before the live
differential is wired up.

---

## Resolved unknowns

Every `NEEDS CLARIFICATION` from Technical Context is resolved above:

| Unknown | Resolved by |
|---|---|
| Grammar source for constrained generation | D1 — the committed constraint inventory |
| Deterministic randomness with cross-version stability | D2 — in-repo xoshiro256\*\* |
| Signature computation without a second comparison path | D3 — derive from `ConfigDivergence` |
| Where the code lives | D4 — hermetic/live split across the two existing crates |
| Shrink algorithm | D5 — structural delta-debugging with a declared ordered catalogue |
| Queue location and unreachability from `certify` | D6 — `conformance/discovery/`, verified against `load.rs` |
| How the pipeline proof avoids cheating | D7 — reuse the sealed `EvidenceSource` boundary |
| Corpus manifest ownership and digest verification | D8 — Rust-owned strict JSON, fetch stays network-lane |
| Lane wiring and its enforcement | D9 — explicit allow-list profile, enforced by `parity_registry_check` |
| Budget apportionment between tiers (deferred by clarify) | D10 — scheduled campaign is hermetic-tier only; cap 25 |
| Where relations vs findings are validated | D11 — V31/V32 in `validate`; D1–D5 in `discovery check` |

---

## Measured thresholds (T114) — SC-002 and SC-004

A threshold nobody measured is a guess. These are the **observed** values from three real
`config-differential` campaigns against the verified pinned oracle (`@devcontainers/cli`
0.87.0) on `prof-linux-amd64-docker-0870`, run 2026-07-27. Each campaign started from an
empty queue so its findings were minimized on admission rather than deferring to a record
an earlier campaign had already made.

| Campaign | Seed | Candidates | Parse-stage failures | **SC-002** (≤10%) | Minimized samples | **SC-004** median (≥0.80) |
|---|---|---|---|---|---|---|
| 1 | `0xa1b2c3d4` | 200 | 0 | **0.00%** ✅ | 2 | 0.6219 ⚠️ (n=2) |
| 2 | `0x5eed0002` | 40 | 1 | **2.50%** ✅ | 4 | **0.8516** ✅ |
| 3 | `0x0badc0de` | 25 | 0 | **0.00%** ✅ | 7 | **0.8889** ✅ |

**SC-002 holds with a wide margin.** The worst observed document-syntax failure rate is
**2.50%**, against a 10% ceiling — the generator is exploring the tool, not the parser. The
campaign additionally reports `trivialFailureFraction` against a declared
`trivialFailureCeiling` of 0.1 and did not breach it in any run.

**SC-004 holds on the pooled sample.** Pooling the 13 independent minimizations across all
three campaigns:

```
n = 13    median = 0.8438 ✅    mean = 0.7921    min = 0.4000    max = 0.9286
sorted: 0.400 0.400 0.763 0.824 0.839 0.842 0.844 0.861 0.861 0.889 0.918 0.929 0.929
```

The pooled median of **0.8438** clears the 0.80 floor. Campaign 1's apparent 0.6219 is an
**n=2 artifact**, not a regression — see the measurement note below.

**Measurement note — why n is much smaller than the finding count.** Only 13 of the 66
emitted candidates carry a live reduction. A candidate re-emitted for a signature the queue
already carries deliberately does **not** re-minimize: `campaign.rs` routes it through
`Reduction::not_attempted` with the reason *"the findings queue already carries this
signature, and the reduced input recorded when it was admitted still stands"*, and the
already-reduced input lives on the admitting **witness** (`minimalInput` +
`reductionSteps`), not on the re-emitted candidate. That is a deliberate cost gate (FR-022)
and it states its reason rather than claiming minimality — but it means **the campaign
report does not surface an SC-004 aggregate**, so the metric has to be recomputed from
candidate provenance as done here. Campaign 1 generated 200 candidates over the same
signature set, so nearly every emission was a re-emission and only 2 live reductions
survived; campaigns 2 and 3 were deliberately run with smaller `--candidates` counts to
raise the number of first-observations and thus the sample size.

The two 0.400 outliers are both small inputs (5 → 3 nodes): a document already near-minimal
has little to remove, so a low *fraction* there is not a shrinker deficiency. SC-004
specifies a **median** precisely to tolerate them.

**Follow-up worth considering** (not required by SC-004, which the pooled data satisfies):
have the campaign outcome carry a reduction aggregate (count and median over the campaign's
own live minimizations) so the threshold is readable from the report instead of recomputed
from `target/discovery/candidates/*/provenance.json`.
