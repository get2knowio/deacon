# Phase 0 Research: Deterministic Conformance Coverage

**Feature**: 024-deterministic-conformance-coverage
**Date**: 2026-07-26

Every decision below was settled by reading the current code and registry, not by
reasoning about what the machinery "must" do. Where a measurement contradicted the
obvious design, the measurement won and the decision says so.

## Measured baseline

Taken from the tree at `611c8a8`, so later phases can detect drift rather than argue
from memory.

| Quantity | Value |
|---|---|
| Behaviors | 27 (`read-configuration` 13, `observable-state` 5, `exec` 2, others 1 each) |
| Cases | 88 — 77 declarative, 11 legacy |
| Declarative operations | 82 — `read-configuration` 65, `up` 13, `exec` 4 |
| Operations with zero cases | 7 — `build`, `down`, `run-user-commands`, `templates apply`, `outdated`, `upgrade`, `doctor` |
| Cases declaring a context | **0** of 77 |
| Behaviors declaring applicability | **0** of 27 |
| Channel observations | exit-code 70, structured-output 53, stderr 7, container-state 6, stdout 4, temporal 2, filesystem 1, image 1, injected-process 1, process-graph 1, **file-content 0** |
| Environment dimensions | 4 (`dim-os`, `dim-arch`, `dim-runtime`, `dim-oracle`) |
| Active profiles | 1 (`prof-linux-amd64-docker-0870`) |
| Gaps / residuals | 0 gaps; 14 residuals (8 queued, 6 permanent) |
| Clause inventory | 250 units — 51 `behavior-mapped`, 156 `non-testable`, 12 `not-applicable` |
| Violation classes in use | V1–V24 (V25 retired) |
| Fixtures | 46 directories under `conformance/fixtures/` |

Two figures drive most of the design. **Zero cases declare a context** means the
applicability machinery is untested in production, so this feature is its first real
consumer. **51 behavior-mapped clauses resolve onto 27 behaviors** means the clause
inventory is already fully mapped and is *not* a reservoir of undiscovered behaviors —
see Decision 9.

---

## Decision 1 — Scenario dimensions get their own namespace; they MUST NOT join `dimensions.json`

**Decision**: Declare the six scenario dimensions in a new registry file
`conformance/registry/scenario.json` under an `sdim-` id prefix, with applicability rules
in `conformance/registry/applicability.json`. The existing `dim-*` dimensions and
`profiles.json` remain untouched and continue to mean *environment*.

**Rationale**: This looked like an obvious extension of `dimensions.json` until the
profile evaluator was read. Two facts make it unworkable:

1. `CertificationProfile.context` is an `IndexMap<String, String>` that assigns each
   declared dimension **exactly one** value (`model.rs:295`). A profile would have to
   declare itself to *be* the `up` operation, or *be* the Compose configuration source.
   That is a category error — a profile describes where evidence can be gathered, not
   what is being exercised.
2. Worse, `applies_in_profile` treats a condition on a dimension the profile does **not**
   assign as **unsatisfied** (`validate.rs:2677`, and its own doc comment says so). So
   adding `dim-operation` and then constraining any behavior by it would silently drop
   that behavior **out of profile** — removing it from the coverage denominator entirely.
   A feature whose entire purpose is to stop the denominator from hiding things would, on
   its first commit, hide things. The failure would also be invisible: `certify` counts
   only in-profile behaviors, so the number would go *down* and still be green.

**Alternatives considered**:

- *Reuse `dim-*` and let a profile assign a wildcard value.* Rejected: it changes the
  meaning of `applies_in_profile` for every existing behavior and of V10 for every
  existing case, converting a targeted addition into a semantic migration of machinery
  that currently works.
- *Encode scenario context as free-text tags on cases.* Rejected: no closed value set means
  no denominator, and a denominator is the deliverable.
- *A second, parallel profile concept ("scenario profiles").* Rejected: profiles are
  selectors for evidence gathering. Scenario context is a property of a case, not of a
  lane.

**Consequence**: `Condition` is reusable verbatim for scenario applicability (it is just
dimension + value subset), but the *evaluator* is new, because scenario conditions are
matched against a case's declared scenario context, not against a single active
assignment.

---

## Decision 2 — Two obligation kinds, machine-owned, mirroring the inventory/classification split

**Decision**: Generate obligations into `conformance/obligations/obligations.json`
(machine-owned, the sole output of `coverage generate`) and hand-author their resolutions
in `conformance/registry/obligation-dispositions/<area>.json`. Generation never writes a
disposition file; hand edits to the generated file are a provenance violation.

Two kinds, never multiplied (spec FR-019):

- `obl-bhv-<hash8>` — a behavior paired with a context its own applicability requires.
- `obl-cmb-<hash8>` — a valid pair, or a selected high-risk triple, of scenario-dimension
  values, partitioned by operation.

**Rationale**: This is the exact shape 020 and 021 already established and validated twice
(`inventory/constraints.json` + `registry/classifications/`, `inventory/clauses.json` +
`registry/clause-classifications/`). Reusing it means the drift workflow, the
regenerate-and-byte-compare determinism test, the scaffold-with-`UNREVIEWED`-sentinel
authoring flow, and the V11–V14 violation semantics all transfer rather than being
reinvented. It also means a reviewer already knows how to read the diff.

**Alternatives considered**:

- *One obligation kind: behavior × combination.* Rejected on arithmetic. 27 behaviors ×
  the per-operation pair space produces an obligation set in the thousands, each needing a
  hand-authored disposition. The clarification session settled this; the code confirms the
  disposition files would be unreviewable.
- *Store dispositions inline on the generated file.* Rejected: generation would then
  overwrite human judgement, the failure mode V14 exists to prevent.

---

## Decision 3 — Enumerate the pair space; do not compute a covering array

**Decision**: `coverage generate` enumerates **all valid pairs** per operation as
obligations. It does not compute a minimal covering array, and it does not generate cases.

**Rationale**: Pairwise testing tools (IPOG, AETG) exist to produce a *small set of test
cases* covering all pairs. That is not the problem here. Here the pairs **are** the
denominator: authors write cases, and the report says which pairs remain uncovered.
Enumeration is trivially deterministic and trivially explainable; a covering array is
neither, and a generated array would additionally imply that the tool chooses what to
test, which is precisely the judgement FR-016 reserves for humans.

**Alternatives considered**:

- *Generate a covering array and require a case per row.* Rejected: it fabricates
  scenario combinations nobody chose, and a greedy covering array is sensitive to input
  ordering, so the obligation set would churn on unrelated edits.

---

## Decision 4 — Split the declarative runner by resource group

**Decision**: Replace the single test function with a fixed set of driver functions, one
per `resourceGroup` value, across two binaries: `parity_conformance_runner`
(config-only, no Docker) and a new `parity_conformance_docker` (Docker-backed, including
the error-path tier). Per-case timeouts come from `tokio::time::timeout` inside the driver;
bounded concurrency from a `JoinSet` with a semaphore.

**Rationale**: The current driver is **one** `#[tokio::test]` iterating every declarative
case serially (`parity_conformance_runner.rs:88`). Three requirements are unsatisfiable
against that shape:

1. **FR-077b (5-minute per-case timeout)** — nextest's `slow-timeout` is per *test*. With
   one test, a single hung case consumes the whole lane's budget and reports as one
   failure with no attribution.
2. **FR-077 (`resourceGroup` honored)** — `resourceGroup` is already declared data on 12
   cases, and nextest cannot act on it, because grouping is per test binary/function. The
   declaration is currently inert.
3. **FR-077a (30-minute tier budget)** — serial execution of a case set that must grow to
   cover ten operations will not fit. Concurrency is required, and concurrency requires
   knowing which cases may share a daemon.

This also corrects a latent inaccuracy: `fixtures/parity-corpus/registry.json` records
`parity_conformance_runner` as `docker_required: false` while 12 of its cases carry
`resourceGroup: docker-shared` and do drive Docker. The split makes the registry entry
true again rather than papering over it.

**SC-013 is preserved.** Resource groups are a closed set, so adding a case with an
existing group stays a pure data edit. Only introducing a *new* resource group would need
a new driver function, and that is a deliberate infrastructure change, not case authoring.

**Alternatives considered**:

- *One test function per case, generated by a macro.* Rejected: it makes adding a case a
  code change in substance even if a macro hides it, and it defeats SC-013's intent.
- *Keep one function and enforce timeouts internally only.* Rejected: it fixes FR-077b but
  leaves `resourceGroup` inert and the budget unreachable.

---

## Decision 5 — Inject regressions at the evidence-source boundary

**Decision**: A regression is applied to the **raw captured artifact** — the completed
process result, the `docker inspect` document, the file bytes — *before* any observer runs.
Mutating an observer's returned `RawChannelEvidence` is forbidden and is checked.

**Rationale**: The obvious cheap implementation (perturb the evidence an observer returned)
has a hole that defeats the story's purpose. If an observer is dead — always returns empty
regardless of input — then perturbing its *output* still produces a difference, so the
channel is reported live while observing nothing. Injecting *upstream* of the observer
closes this: a dead observer ignores the perturbed source, returns the same value, no
difference appears, and the channel is correctly reported **inert**.

Source mutation of deacon itself was considered and rejected: one source edit typically
perturbs several channels or none, so it cannot target a channel; it needs a rebuild per
mutation; and it can leave a dirty tree, which FR-066 forbids.

**Alternatives considered**:

- *Mutate the container/image for real (e.g. `docker container update`).* Rejected as the
  default: slower, runtime-specific, and not available for every channel. Retained as an
  option for a small number of container channels where the source document alone cannot
  express the perturbation.

---

## Decision 6 — A new `coverage` command group, leaving `report` untouched

**Decision**: Add `cargo run -p deacon-conformance -- coverage <generate|check|report>`,
writing `target/conformance/coverage-{pairwise,triples,operations,observables}.{json,md}`.
`report.json` keeps its existing schema; it gains only a small summary block referencing the
new artifacts.

**Rationale**: `report.json` has a versioned contract (`contracts/report-schema.md`) and is
byte-compare tested. Four new report families are a large addition; grafting them into the
existing document would churn a stable contract for consumers that do not need them. A
sibling command reuses `report.rs`'s byte-stability conventions (ordered maps, no
timestamps, no absolute paths) without renegotiating the existing schema.

---

## Decision 7 — Split `cases.json` by area

**Decision**: Move to `conformance/registry/cases/<area>.json`, mirroring
`behaviors/<area>.json`. The loader gains directory-aware loading for cases.

**Rationale**: `cases.json` is already 88 records in one file, and this feature multiplies
it across ten operations. Per-area files keep review diffs local to the area being changed
and match the precedent the behaviors already set. The loader change is small and its
determinism is already covered by the byte-stability tests.

**Note**: this is a mechanical migration with a real risk of silent record loss. It must
land as its own commit with a count assertion before and after, not folded into a content
change.

---

## Decision 8 — Five new violation classes, V26 – V30

V25 is retired, so the next free class is V26. Each is stated in `RULES.md` in the same
row format as V1–V24, keeping the `validate.rs` ↔ `RULES.md` lockstep the index exists to
make checkable.

| Class | Statement |
|---|---|
| **V26** | scenario-model integrity: a dead dimension value, an applicability rule naming an unknown dimension or value, or a rule carrying no ground |
| **V27** | obligation provenance: committed obligations ≠ regenerated, or an obligation referencing a removed dimension value |
| **V28** | an applicable obligation with zero dispositions, or with more than one |
| **V29** | malformed disposition: a rationale naming no ground, a high-risk triple dispositioned by rationale or waiver rather than a case, or a disposition whose scope resolves to no obligation (stale) |
| **V30** | injected-regression integrity: a declared channel with no regression record, or a regression targeting a channel that has no observer |

V28 and V29 block `certify`; V26 and V27 block `validate` (and therefore every PR); V30
blocks the injected-regression acceptance run.

---

## Decision 9 — New behaviors come from three named sources, not from invention

**Decision**: Behaviors added by this feature MUST originate from one of: (a) the observable
contract of an operation that currently has none — the seven uncovered operations; (b) a
clause currently classified `non-testable` **whose ground was the absence of a Docker-backed
tier**, re-reviewed once that tier exists; (c) a named field from US5 whose comparison has no
behavior to attach to. Any other addition needs a stated source unit, as V4 already requires.

**Rationale**: The intuitive plan — mine the clause inventory for unmapped behaviors — does
not work, and measuring said so. All 51 `behavior-mapped` clauses already resolve onto the
existing 27 behaviors (validate passes, and V15 would flag a dangling reference). The clause
inventory is fully mapped; it is not a backlog. The 156 `non-testable` clauses are the real
reservoir, and some of them are non-testable only because nothing could observe a container
when they were classified.

**Consequence**: Re-reviewing `non-testable` classifications is in scope and is a
*classification* edit, so it goes through the hand-authored path and shows up in a diff.
Growth of the denominator is guarded by SC-014: a new behavior arrives with its obligations
dispositioned or it fails validation.

---

## Decision 10 — Budget enforcement is measured and asserted, not aspirational

**Decision**: The Docker driver records its own wall clock into the run-report fragment and
asserts the 30-minute tier budget. nextest's `slow-timeout` on the binary is set above it as
a backstop, not as the primary mechanism.

**Rationale**: A `slow-timeout` failure reports "the binary was slow", which is
indistinguishable from a hung daemon. An explicit assertion reports the number and the case
list that produced it, which is what a maintainer needs in order to decide between
tightening applicability rules and splitting the tier. FR-077a says exceeding the budget is a
failure of acceptance, not a reason to widen it, so the number has to be visible to be
argued with.

---

## Deferred / explicitly not built

| Refused | Why |
|---|---|
| Automatic case generation from uncovered pairs | The tool would choose what to test; FR-016 reserves that judgement. It also produces cases with no reviewed intent. |
| A covering-array minimizer | See Decision 3 — solves a problem this feature does not have, and churns the obligation set on unrelated edits. |
| Extending the assertion language | 023's hard line, restated in this spec's Out of Scope. A derived observer field is the escape hatch. |
| Activating a second environment profile | Assumption 10. FR-004b keeps activation a data change, so deferring costs nothing structurally. |
| Nested-struct unknown-field preservation | Pre-existing tracked deferral, unrelated to coverage; touching it here would widen an already large change. |
