# Conformance Registry — Disposition Rules

This document is the human-readable companion to the machine-enforced disposition
rules in `crates/conformance/src/validate.rs`. It exists so a contributor can predict
validation outcomes **before** running `conformance validate` (FR-014: "the full rule
set MUST be documented in the registry itself").

Every behavior in the registry carries **three independent axes** (FR-009 – FR-012).
The axes are stored and reported **separately**; the registry deliberately provides no
single combined "different but acceptable" state, and a record that omits any axis is
rejected at load as a `SCHEMA` failure.

| Axis        | Field       | Closed set of values |
|-------------|-------------|----------------------|
| Spec        | `spec`      | `conformant`, `nonconformant`, `unspecified`, `not-applicable` |
| Reference   | `reference` | `aligned`, `divergent`, `unknown`, `not-applicable` |
| Decision    | `decision`  | `follow-spec`, `align-with-reference`, `deacon-extension`, `intentional-divergence`, `unresolved-gap` |

- **Spec** — how the behavior relates to the written [devcontainers/spec](https://github.com/devcontainers/spec).
- **Reference** — how the behavior relates to the *observed* reference implementation
  (`@devcontainers/cli`) **for the active profile's oracle only** (FR-013). It is a claim
  about the pinned oracle, not a universal truth.
- **Decision** — what this project has decided to do about the behavior.

The three-axis model is what elevates the registry above a binary waiver system: it
keeps spec violations, reference bugs, and deliberate extensions from being conflated
into one ambiguous "waived" bucket.

## Core principle: statuses are evidence-backed claims, not aspirations

A `spec: conformant` / `reference: aligned` behavior with no test case behind it is
exactly the ambiguity the three-axis model exists to eliminate — a claim deacon
*believes* but has not *verified*. Honestly, that is a **gap**. The contradiction rules
below encode this principle: a status may only assert alignment or conformance when
there is structural evidence (a test case, or a waiver) standing behind it.

A **waiver** counts as evidence for a `divergent` status because the parity harness
*verifies* waivers keep reproducing: a waiver whose characterized difference stops
reproducing fails the run as *stale*. So waiver-only coverage legitimately backs
`reference: divergent` without forcing an `unresolved-gap` decision.

## Violation-class index

The complete set `validate` can emit, in one place. Each row links to the section that
states the rule; this table exists so the lockstep between `validate.rs` and this file is
checkable at a glance rather than by reading every section.

| Class | Statement | Where |
|---|---|---|
| `SCHEMA` | a registry file is unreadable, malformed, or violates its record schema | loader (`load.rs`) |
| **V1** | dangling reference: a record id, dimension value, orphan behavior, or a case naming a test binary with no source under `crates/*/tests/` | `validate.rs` |
| **V2** | duplicate stable id anywhere, an id-format violation, or a prefix↔type mismatch | `validate.rs` |
| **V3** | a test case linked to no behavior | `validate.rs` |
| **V4** | a source unit with empty `behaviors` and no `outOfScope` | `validate.rs` |
| **V5** | an in-profile behavior with no case, no waiver AND no gap | `validate.rs` |
| **V6** | a waiver whose `expires` is earlier than today | `validate.rs` |
| **V7** | a source revision whose `pin` disagrees with its `verifiedAgainst` file | `validate.rs` |
| **V8** | a disposition contradiction — rules R1 – R8 | [Contradiction rules](#contradiction-rules-r1--r8) |
| **V9** | an expected outcome referencing an undeclared observable channel | `validate.rs` |
| **V10** | a case whose context has an empty intersection with a linked behavior's applicability | `validate.rs` |
| **V11 – V15** | inventory join: stale / unclassified / malformed / provenance / clause↔source integrity | [Inventory join](#inventory-join-v11--v15--constraints-and-clauses) |
| **V16** | declarative-case well-formedness (shape, `oracleType`, consumer subcommand, assertions, `fsAllowlist`, observable channel, Docker `resourceGroup`, [error-path tier](#the-container-backed-error-path-tier-v16-024)) | `validate.rs` |
| **V17** | committed-snapshot integrity (orphan or malformed provenance) | `validate.rs` |
| **V18** | a Docker case referencing a fixture with an unpinned image | `validate.rs` |
| **V19** | an allowed-difference whose backing waiver/divergence id does not resolve | `validate.rs` |
| **V20** | invariant-metamorphic arity (≥2 operations + a relationship naming a sibling) | `validate.rs` |
| **V21** | migration mapping integrity (forward direction) incl. exception correspondence; the reverse orphan-case direction is **retired** | [Migration mapping](#migration-mapping-v21--v23--transitional) |
| **V22** | fixture correspondence and unreferenced migrated fixtures | [Migration mapping](#migration-mapping-v21--v23--transitional) |
| **V23** | malformed residual | [Migration mapping](#migration-mapping-v21--v23--transitional) |
| **V24** | unscoped or unjustified normalization rule | [Normalization rules](#normalization-rules-v24--transitional) |
| ~~**V25**~~ | baseline provenance — **RETIRED** (FR-053); the artifact is retained, the gate is gone | [Baseline provenance](#migration-baseline-provenance-v25--retired) |
| **V26** | scenario-model integrity: dead dimension value; rule naming an unknown dimension/value; rule with no ground; case `scenarioContext` that is partial, undeclared, or invalid | [Scenario model and obligation provenance](#scenario-model-and-obligation-provenance-v26--v27) |
| **V27** | obligation provenance: committed obligations ≠ regenerated; `revision` mismatch; obligation referencing a removed dimension value | [Scenario model and obligation provenance](#scenario-model-and-obligation-provenance-v26--v27) |
| **V28** | an applicable obligation with zero dispositions, or with more than one | [Obligation dispositions](#obligation-dispositions-v28--v29) |
| **V29** | malformed disposition: filler rationale; a high-risk triple dispositioned by rationale/waiver rather than a case; a disposition whose obligation no longer resolves (stale) | [Obligation dispositions](#obligation-dispositions-v28--v29) |
| **V30** | injected-regression integrity: a declared channel with no regression record; a regression targeting a channel with no observer | [Injected-regression harness](#injected-regression-harness-v30) |

**Three distinctions this file keeps apart**, because conflating any pair makes a status
unfalsifiable:

- **gap vs. waiver** — missing coverage versus a characterized divergence. See
  [Gap vs. waiver](#gap-vs-waiver).
- **residual vs. gap** — missing *representation* (the coverage exists, carried by a
  program not yet retired; never blocks certification, does block deleting its carrier)
  versus missing *coverage* (always blocks). See
  [Residual vs gap](#residual-vs-gap-do-not-conflate).
- **declared vs. undeclared deficiency** — an admitted, tracked normalization deficiency is
  reported debt; an unadmitted blanket rule is V24 and blocks. See
  [Normalization rules](#normalization-rules-v24--transitional).

## Contradiction rules (R1 – R8)

Validation reports any violated rule under class **V8**, naming the record and the
specific rule identifier (e.g. `R3`) in the message. R1 – R4 are the FR-014(a) – (d)
minimum; R5 – R8 close the remaining "declared, never verified" loopholes.

| Rule | Statement | Rationale |
|------|-----------|-----------|
| **R1** | decision `unresolved-gap` contradicts (spec `conformant` **and** reference `aligned`) | A behavior that both matches the spec and matches the reference is, by definition, resolved — it cannot simultaneously be an open gap. |
| **R2** | decision `deacon-extension` requires spec ∈ {`unspecified`, `not-applicable`} | An extension is by definition outside the spec's scope. Calling something both `conformant`/`nonconformant` *and* an extension is a category error. |
| **R3** | decision `intentional-divergence` contradicts reference `aligned` | You cannot intentionally diverge from a reference you are aligned with. If the reference is aligned, the divergence is not real. |
| **R4** | reference `unknown` on an **in-profile** behavior requires decision `unresolved-gap` | If we have not characterized what the reference does, the only honest decision is to admit the gap. Any other decision claims knowledge we do not have. |
| **R5** | decision `follow-spec` requires spec `conformant` | "We follow the spec" is only truthful when we are actually conformant to it. |
| **R6** | decision `align-with-reference` requires reference `aligned` | "We align with the reference" is only truthful when we are actually aligned with it. |
| **R7** | a behavior whose **only** structural coverage is a gap record requires decision `unresolved-gap` | Gap-only coverage means there is no test and no waiver. The evidence backs nothing but a gap, so the decision must say so. |
| **R8** | an **in-profile** behavior with **no test case and no waiver** requires reference `unknown` | With no case and no waiver there is no evidence for any reference claim — the only defensible reference status is `unknown`. Statuses are verified claims, not aspirations. |

### R8 exemption: `deacon-extension`

R8 exempts behaviors whose decision is `deacon-extension`. For an extension,
`reference: not-applicable` is the *correct* reference status — the reference CLI has no
concept of the behavior at all, so `not-applicable` is a classification, not an
unverified claim. Forcing `unknown` would be wrong. (This exemption is also
belt-and-suspenders: R2 already constrains an extension's spec, and R7 already blocks
gap-only extensions, so any *valid* in-profile extension is already case- or
waiver-backed — which makes R8's antecedent false regardless.)

### The R8 → R4 → R7 chain (why incremental population stays coherent)

These three rules interlock so that adding a behavior *before* it has been characterized
never produces a dishonest status, yet never blocks a contributor either:

```
no case and no waiver   ──R8──▶   reference must be `unknown`
reference `unknown`      ──R4──▶   decision must be `unresolved-gap`
decision `unresolved-gap` (gap-only) ──R7──▶   a gap record must exist
gap record exists                 ──▶   structural validation (V5) passes
                                  ──▶   strict certification still BLOCKS on the gap
```

So a freshly-recorded, uncharacterized behavior is forced into the honest shape
`reference: unknown` + `decision: unresolved-gap` + a `gap-` record. The registry
validates (nothing is silently broken), while strict certification correctly refuses to
certify until the gap is resolved. When a test case is later added, the statuses become
evidence-backed, the decision is re-recorded, and the gap record is deleted in the same
change (otherwise R1/R7 flag the now-stale contradiction).

## Gap vs. waiver

Both a **gap** (`gap-`) and a **waiver** (`wvr-`) satisfy structural coverage (they keep
a behavior from tripping V5), but they mean opposite things and are reported and gated
differently.

| | **Gap** (`gap-`) | **Waiver** (`wvr-`) |
|---|---|---|
| Meaning | "We know we do **not** yet have this covered / characterized." | "We have characterized a difference and **accepted** it." |
| Evidence value | None — it is an admission of *missing* evidence. | Positive — the parity harness verifies the difference keeps reproducing (a stale waiver fails). |
| Backs which reference status | `unknown` (via R4/R7). | `divergent`. |
| Expiry | **None.** Persists until the registry is edited to resolve it. | **Required** `expires` date. `expires < today` → violation V6. Forces periodic re-review; there is no auto-renewal. |
| Strict certification | **Always blocks** (FR-020, FR-025). | **Never blocks** — waivers are enumerated in the certification output but are non-blocking. |
| Coverage bucket in the report | `gap` | `waived` (never folded into `conformant`, FR-023). |

In short: a gap is a promise to do work; a waiver is a decision that no further work is
needed. A gap can never be certified around; a waiver can.

## Out of scope — non-behavioral differentiators

Some ways deacon differs from the reference are **not behaviors** and therefore are
**recorded nowhere** in the registry — they have no `spec`/`reference`/`decision` axis
because there is nothing externally observable to characterize (research Decision 6,
item 3). Examples:

- **Single static binary** — deacon ships as one native binary vs. a Node.js package.
  A packaging/distribution property, not an observable behavior of any command.
- **Environment-probe caching performance** — a latency optimization. It changes *how
  fast* a command runs, not *what* it observably does.

These are documented here as out-of-scope so contributors do not attempt to force them
into behavior records (which would then have no meaningful reference status and would
distort the coverage denominator). If a purported differentiator has no externally
observable effect on stdout, stderr, exit code, container state, or the filesystem, it is
out of scope for the registry.

## The declarative conformance runner is DEV-ONLY (Principle II)

The declarative conformance runner (022-conformance-runner) — the shared machinery that
executes a `cases.json` record against deacon / the pinned reference / a committed
snapshot, capturing and normalizing observable channels — is **contributor test tooling,
never a shipped consumer command**. It adds NO `deacon` subcommand. Concretely:

- `conformance snapshot check|diff` is a subcommand of the dev-only `deacon-conformance`
  bin (`cargo run -p deacon-conformance -- …`), not of the `deacon` CLI.
- `conformance-snapshot refresh` (the reviewed record path) is a `parity-harness` bin
  (`cargo run -p parity-harness --bin conformance-snapshot -- …`), not `deacon`.
- The live differential run is a **test binary** (`parity_conformance_runner`, `--profile
  parity` only), not a runtime command.

The consumer surface stays exactly `up`/`down`/`exec`/`build`/`read-configuration`/
`run-user-commands`/`templates apply`/`doctor` (Constitution II). A declarative
`operations[].subcommand` is validated to be in that surface (V16); the runner exercises
only consumer commands, never authoring ones. Do NOT add a `deacon conformance` /
`deacon snapshot` command — that would drag test tooling into the shipped binary and
violate the consumer-only scope.

A declarative case may also only declare an **observable** channel — one the runner has an
observer for (`model::OBSERVED_CHANNELS`, V16). This is stricter than V9, which only
requires the channel to be *declared* in `channels.json`: a channel can be declared and
still have no observer (`chan-container-state` is, until its declarative observer lands),
in which case the case would validate cleanly and then fail at RUN time while the registry
claimed the behavior was covered. `OBSERVED_CHANNELS` is held in lockstep with the
harness's `observe::observer_for` by a `parity-harness` test, the same discipline the
normalization-rule registry uses.

**A case invoking a container-creating subcommand must declare a Docker `resourceGroup`**
(`model::CONTAINER_SUBCOMMANDS` = `up`/`down`/`exec`/`build`/`run-user-commands`; V16).
The field reads like nextest scheduling metadata, but it is the *only* discriminator both
the runner and the validator use to decide a case is Docker-backed. Omit it on a case that
runs `up` and three protections switch off at once, silently: the runner builds no isolated
`DockerWorkspace` (so both sides run against the committed fixture tree and stamp an
identical `devcontainer.local_folder` label — deacon's container and the oracle's become
indistinguishable, as do two cases running in parallel), no RAII cleanup guard is created
(container, network and volume leak), and V18 skips the case's image inputs because it is
`is_docker_case`-gated too. The case still runs, and may well pass.

### The container-backed error-path tier (V16, 024)

Parity testing used to stop comparing the moment **both** implementations accepted a
configuration document. That is precisely where the reference is most lenient, so it is
where a difference is most likely to survive unobserved — and every later stage (build,
container creation, Feature installation, lifecycle execution, teardown) went uncompared
for any input that read cleanly.

A case joins the tier by declaring `errorPathTier: true`. Membership is **declared, not
derived**, because a predicate over `expectFailurePhase` gets it wrong in both directions:
`case-down-removes-container` declares `expectFailurePhase: exec` on a *verification* step
and is not an error-path case, while a genuine tier case whose phase was left off would
silently leave the tier with nothing to notice. Declaring it makes each of those a
validation failure instead.

| Rule | A tier case must … | Because |
|---|---|---|
| 1 | declare an `expectFailurePhase` later than `config-resolution` on ≥1 operation | FR-042 requires the record to say **where** the failure occurs |
| 2 | declare `config-resolution` on **no** operation | a verdict reachable at configuration read is a verdict about the stage the tier exists to look past (FR-041) |
| 3 | declare a Docker `resourceGroup` | every later stage needs the container runtime — and without the group there is no isolated workspace and no cleanup guard |
| 4 | *(all declarative cases)* declare only phases its subcommand can **reach** | `read-configuration` reaches exactly one phase; a case declaring `lifecycle:postCreate` on it describes a run that cannot happen and would otherwise validate, run, and report a green nothing |

Reachability is the closed, hand-reviewed `model::phases_reachable_by` mapping. **`down`
stops at `config-resolution`**: teardown owns no phase in the closed failure-phase set
(022-conformance-runner data-model §8, which this feature reuses rather than extends), so a
teardown error-path case declares its failure on the operation that *observes* the teardown
— an `exec` into the container that must no longer exist — which is the shape
`case-down-removes-container` already uses.

`errorPathTier` is **excluded from `caseHash`** (with `inputClass`, `notes` and
`allowedDifferences`): it classifies a case, it does not change a byte the runner feeds the
CLI, so joining the tier must never re-record a snapshot.

## Inventory join (V11 – V15) — constraints AND clauses

This section is the human-readable companion to the two inventory joins enforced in
`crates/conformance/src/validate.rs`: `check_inventory` (the schema-constraint inventory,
020-schema-constraint-inventory) and `check_clause_inventory` (the normative-clause
inventory, 021-normative-clause-inventory). It stands in the same validate.rs/RULES.md
lockstep as R1 – R8 / V1 – V10: the classes below are updated in the SAME change that
alters the enforcement.

An **inventory unit** is either a machine-extracted schema **constraint** (`cst-`, from the
vendored pinned JSON schemas) or a canonicalized prose **clause** (`clu-`, from the vendored
pinned `docs/specs/` Markdown). Each unit carries an **effective disposition** recorded by a
hand-authored **classification** (`cls-` for constraints, `clc-` for clauses) under deacon's
consumer-only scope. Validation joins each inventory against its classifications (and the
vendored sources) and reports these classes alongside V1 – V10 in one run; **all block
`certify`** (the release gate) — an unclassified, stale, malformed, provenance-broken, or
source-inconsistent inventory can no more be certified around than a `gap-` record can.

V11 – V14 are the **generalized** inventory-unit classes (they run for constraints AND
clauses). **V15 is new and prose-only** (schema constraints, whose substance IS the parsed
JSON, have no separate source-text-integrity dimension).

| Class | Statement (inventory unit = schema constraint OR prose clause) | Remedy |
|-------|-----------|--------|
| **V11** | a classification (`cls-`/`clc-`) names a unit id (`cst-`/`clu-`) absent from its committed inventory (**stale**) | Delete or re-point the record in the same change that moved the inventory. Waiver-style self-invalidation — a classification whose unit vanished never lingers. |
| **V12** | a unit has **no effective disposition** (**unclassified** — this IS the drift item; there is no separate drift record type) or **more than one** per-unit record (**duplicated**). For a clause: no per-clause `clc-` record AND no permitted document-scope default (see below); an unresolved `ambiguous` clause is V12 by construction. | Author exactly one classification (or the permitted document-scope default). Every unit of every kind requires one. |
| **V13** | a classification's shape/linkage is broken: the `id`-tail mirror, the `behaviors` arity/existence rule vs its `disposition`, a missing `rationale` on a `non-testable`/`not-applicable` record, a clause record with BOTH or NEITHER of `clause`/`document`, or a document-scope default on a **consumer** document | Fix the record to satisfy the arity table below and the document-scope rule. |
| **V14** | **provenance** breakage: a manifest fingerprint (schemas OR spec) mismatches a vendored file, the inventory's `revision` ≠ the registry's matching-kind revision pin (`schema`/`spec`), or the committed inventory no longer byte-matches a fresh regeneration (`inventory generate` / `clause generate`) | Re-vendor / re-generate; never hand-edit the machine-owned inventory. |
| **V15** (clauses only) | **clause↔source integrity**: a clause's `strength` label contradicts its excerpt's RFC-2119 keywords, a `descriptive` clause hides an unqualified mandatory keyword, or an excerpt is not present in the pinned document under its recorded heading/anchor | Fix the excerpt, anchor, or strength label; `clause generate` fails loud on the same conditions so the committed inventory can never carry them. |

### Document-scope disposition default (clauses only, research Decision 7)

A `clc-` classification MAY be **document-scoped** — one `clc-doc-<key>` record dispositioning
every non-`ambiguous` clause of an **authoring**-scope document as `not-applicable` (consumer-only
scope, constitution II). Resolution order for a clause: a per-clause record wins; else, if the
clause's document is `authoring`-scope AND its `testability` ≠ `ambiguous`, the document-scope
default applies; else the clause is unclassified (V12, blocking). Two guard rails, both V13:
a document-scope default is permitted **only** for `authoring` documents (a `consumer` document
is classified clause-by-clause), and an `ambiguous` clause is **never** covered by a blanket
default — it needs an explicit per-clause decision. The mixed authoring documents
(`features`/`templates`) carry the document-scope default for their authoring bulk PLUS per-clause
`behavior-mapped` overrides for the consumer install/apply clauses inside them.

### Disposition arity (V13)

Every classification carries exactly one `disposition`. The `behaviors` and `rationale`
fields are required or forbidden per disposition; the scaffold sentinel `"UNREVIEWED"` is
not a member of the closed set and is rejected at **load** as a `SCHEMA` failure (never a
V-class).

| Disposition | `behaviors` | `rationale` | Blocks `certify`? | Meaning |
|-------------|-------------|-------------|-------------------|---------|
| `behavior-mapped` | **required**, non-empty, every id an existing `bhv-` record | optional | only if V11–V14 (a well-formed one never blocks) | The constraint is consumer-runtime behavior, covered by real behavior(s) under the existing coverage rules (research Decision 11's evidence rule — no evidence-free behaviors). |
| `non-testable` | **forbidden** (must be empty) | **required**, non-empty | never | The constraint carries no testable behavior (titles/descriptions, `$schema`, JSONC directives). Kept visible in `report` (FR-014). |
| `not-applicable` | **forbidden** (must be empty) | **required**, non-empty | never | The constraint is outside deacon's consumer-only scope (e.g. feature-authoring surface, editor-only keywords). The honest consumer-scope boundary, kept visible in `report`. |

`not-applicable` / `non-testable` are the honest scope boundary: a well-formed one
produces **no** violation, so it is listed in `report` but never blocks certification.

### Drift review workflow (upstream pin bump)

Because a unit's stable id hashes its substance, a materially changed constraint gets a
NEW id — its old classification goes stale (V11) and the new unit is unclassified (V12).
No disposition is ever inherited by name. So a re-vendoring mechanically enumerates its
own review queue:

```
re-vendor at the new pin  →  inventory generate  →  inventory diff old new (review doc)
        →  validate:  V11 = stale classifications to delete/re-point
                      V12 = new/changed units to classify
        →  classify + delete stale records  →  validate clean  →  certify unblocks
```

`certify` stays blocked until the queue is empty; nothing is silently carried forward.

### Machine-owned vs hand-authored file boundary

| Path | Ownership | Edited by |
|------|-----------|-----------|
| `conformance/schemas/<pin>/` | vendored, byte-exact upstream copies + manifest | the human, only when re-vendoring at a new pin (never in place) |
| `conformance/inventory/constraints.json` | **machine-owned** — canonical output of `inventory generate` | `inventory generate` ONLY; hand edits are caught as V14 |
| `conformance/registry/classifications/<doc>.json` | **hand-authored** — one file per manifest document key | humans; `inventory generate` NEVER touches these |
| `conformance/migration/baseline.json` | **machine-owned** — canonical output of `baseline generate`, retained as frozen evidence | `baseline generate` ONLY. Its V25 gate is **retired**, so a hand edit is caught by REVIEW of the diff, not by `validate`; `baseline_archive.rs` still checks the artifact's internal integrity |
| `conformance/migration/mapping.json` | **hand-authored** — the unit → destination mapping | humans; no generator ever writes it |
| `conformance/registry/residuals.json` | **hand-authored** — representation debt, non-blocking | humans; no generator ever writes it |

Generation and classification are strictly separated: regenerating the inventory can add
or remove `cst-` units (surfacing V11/V12 for review) but can never rewrite a human's
disposition. Never delete a unit to go green — units are machine-owned; classify it, or
accept the honest blocking gap.

---

## Migration baseline provenance (V25) — **RETIRED**

This section documented `check_baseline` in `crates/conformance/src/validate.rs`
(023-migrate-parity-to-conformance, US1). That function is **gone**, so this section is
kept as the record of a retired class rather than as a live rule — the lockstep discipline
still holds, and retiring the enforcement in the same change that retired the prose is what
it required.

`conformance/migration/baseline.json` is the frozen, mechanically enumerated inventory of
pre-migration coverage — **151** records today: 118 executable units (91 `live-per-case`,
23 `hermetic-guard`, 4 `internal-consistency`) plus 33 recorded-only
`external-corpus-entry` manifest entries. At the original freeze it was 144 (111
executable); US4 deliberately added 7 fault-injection guard units, each re-frozen with
`--force`. It is the **subject** of the sentence "no coverage was lost". If it is wrong,
the conservation claim is unfalsifiable — which is why it is retained untouched and read,
never rewritten, by every command that measures against it.

> **V25 is retired (023 T099, FR-053).** It is documented here because the artifact it
> guarded is retained and the reasoning still governs how that artifact is read.
>
> The gate compared the committed `baseline.json` against a fresh enumeration of the
> repository tree. That comparison is only meaningful while the tree still contains the
> pre-migration machinery: the moment a superseded carrier is deleted, its units drop out
> of the enumeration and the two can never agree again. A permanent gate would therefore
> forbid ever retiring the machinery this migration exists to retire — it would have to be
> broken to make progress, which is the definition of a gate nobody can obey.
>
> **What is retained**: `conformance/migration/baseline.json` itself, untouched, and the
> final migration report. They are the evidence for the conservation claim. **What is
> gone**: only the live checking gate — `validate` no longer emits V25, and the
> regeneration-vs-committed tests retired with it. `baseline generate` / `baseline check`
> remain as tooling; `baseline check` now reports drift informationally, which after any
> carrier deletion is the expected state, not a failure.
>
> **What still guards the artifact**: it is version-controlled, so lowering it is a
> reviewed diff — and `crates/conformance/tests/baseline_archive.rs` still fails on a
> record that loses its assertion, its id uniqueness, its sort order, or a channel that no
> longer resolves.
>
> **Its PRESENCE is guarded by V21**, and deliberately so. Retiring V25 removed the only
> check that the file exists at all, and `Registry::load` reads a missing baseline as
> `None` rather than as an error — so deleting it made `check_mapping`, `check_residuals`
> and the harness's reported-granularity gate each scope themselves out, and `validate`
> went green while every conservation claim became vacuously true. Retiring a *drift* gate
> was correct; retiring the *existence* requirement along with it was not, and V21 now
> carries it: a registry holding mapping or residual records with no baseline to reference
> is incoherent and is reported.

Membership is derived, not authored, so it cannot be gamed: corpus units come from the
*production* discovery functions (`discover_tier1_cases` / `discover_error_cases` in
`deacon-conformance::parity_corpus`, shared with the live runners — re-walking directories
is exactly how 24 Tier-1 cases were once counted as 25), guard units from scanning each
program's real `#[test]`/`#[tokio::test]` functions, and the external entries from the
pinned manifest. Each unit's `assertion` is authored **once, at freeze**, in
`crates/conformance/src/baseline.rs` (so regeneration reproduces it byte-for-byte) and is
**immutable** thereafter: rewriting it post hoc would let the coverage proof be satisfied
by lowering the bar.

**V25 was deliberately transitional, and was retired earlier than planned.** FR-053
scheduled its removal for feature completion, "once the deletion predicate holds for every
non-residual carrier". In practice the gate and the first deletion are mutually exclusive:
deleting a proven-safe carrier necessarily breaks the gate, so waiting for *every* carrier
would have left verified-safe deletions permanently undone. It was retired at the point
the first carriers cleared the predicate. See the reordering note in
`specs/023-migrate-parity-to-conformance/tasks.md` (T099).

## The parity carriers this migration retired

023-migrate-parity-to-conformance moved parity coverage into this registry. Four
config-corpus carriers cleared the equivalence gate and were deleted in US7 —
deleted: `parity_corpus_tier1`; deleted: `parity_corpus_merged`; deleted:
`parity_corpus_errors`; deleted: `parity_read_configuration` — together with their shared
runner module and their in-repo fixture trees. Their coverage is now the `case-tier1-decl-*`,
`case-merged-decl-*`, `case-errors-decl-*` and `case-readconfig-decl-*` records here,
driven by `parity_conformance_runner`.

Five carriers survive, each for a recorded reason: `parity_build` and
`parity_observable_state` / `parity_state_diff` are fully residual (research D4 predicted
exactly this); `parity_exec` carries one residual; and `parity_up_exec` is
equivalence-clean but is the ONLY evidence for `bhv-exec-container-id-metadata`, so
deleting it would uncover that behavior. Every one of those reasons is a `res-` record or
a coverage fact this file's rules already govern — none of them is a note someone has to
remember.

## Migration mapping (V21 – V23) — transitional

The human-readable companion to `check_mapping` / `check_residuals` in
`crates/conformance/src/validate.rs` and the rules in `crates/conformance/src/mapping.rs`
(023-migrate-parity-to-conformance, US2). Same validate.rs/RULES.md lockstep as every
other class. Like the retired V25 these are **transitional**: they exist to make the
parity→conformance migration falsifiable and retire with it (FR-053). Unlike V25 they are
still live — the migration is not finished while any residual still blocks a carrier.

`conformance/migration/mapping.json` is the *proof* that no coverage was lost. Equal
counts prove nothing — two sets of the same size can still have lost an item and gained
another (research D7) — so every baseline unit carries an explicit destination, and every
destination is reachable from a unit.

| Class | Statement | Remedy |
|-------|-----------|--------|
| **V21** | **mapping integrity (forward)**: a baseline unit with no mapping entry (an orphan *test*); a mapping naming a unit or case that does not exist; a disposition whose arity is wrong (`migrated`/`deduplicated` without `caseIds`, `residual`/`retired` with them, `residual` without a resolvable `residualId`, `deduplicated`/`retired` without a `rationale`); a destination case that resolves to no behavior or declares no observable channel, or names a dangling behavior/channel id; and, for a characterized **exception**, a mapping to zero or to more than one mechanism, a missing mapping entry, or a mechanism whose current direction/scope is BROADER than the recorded pre-migration form. **Also: a registry that carries mapping or residual records with NO committed `baseline.json`** | Give the unit a destination, the case a reachable mapping, the exception exactly one mechanism. A tolerance may be narrowed, never widened. Restore the baseline (or, if the records are genuinely obsolete, delete them too). |
| **V22** | **fixture correspondence**: a `from` split across two `to`s; a `to` fed by two `from`s (a silent merge); a `from` that is not one of the unit's baseline fixtures; a baseline fixture of a migrated unit with no `fixtureMapping` entry (a silent drop); and a migrated fixture no case references (an unreferenced orphan) | Make the correspondence one-to-one and account for every fixture the unit consumed. Declaring the SAME `(from, to)` pair from two units is fine — two modes of one workspace legitimately share one fixture. |
| **V23** | **malformed residual**: a vague `missingCapability` (a filler phrase, or too short to name a mechanism); a `followUp` that is not a tracked reference; an `outOfScopeRationale` that names no ground for permanent exclusion (024 — see [Queued vs permanent](#queued-vs-permanent-residuals-024)); a `blockedCarrier` that is absent on a residual whose units are not ALL `external-corpus-entry`, that names no baseline program, or that is present on an `external-corpus-entry` residual; a `units`/`behaviors` entry that does not resolve; and a unit claimed by a residual while its mapping says it was migrated | Name the specific missing capability, and either a tracked follow-up (`queued`) or the principle that forbids expression (`permanent`); name the carrier the residual pins. A residual never blocks certification, which is exactly why its shape must be strict. |

### The orphan-*case* direction is retired (024 US3)

V21 originally also ran in reverse: every **declarative** case had to be reached by some
mapping entry, on the reasoning that a declarative case IS a migration destination. That
was true exactly while the declarative case set *was* the migration's output. The moment a
case is authored for coverage the migration never had — which is what 024 US3 does, 81
times — the rule reports a correct record as an orphan, and the only way to satisfy it
would be to invent a baseline unit for a case that migrated from nothing.

So it is retired, for the same reason V25 was: a permanent gate here would forbid exactly
the growth the migration exists to make room for. The **forward** direction is untouched,
and it is the one conservation rests on — every baseline unit still needs exactly one
destination, which is what proves nothing was lost.

Legacy pointer cases were exempt from the reverse direction while it existed, and remain
irrelevant to it: they are the pre-migration *carriers*, not destinations; they are retired
once the equivalence gate clears them, not mapped into.

**A characterized exception authored after the branch point is likewise out of scope.** A
`wvr-` or `ext-` whose behaviors are all in `POST_BRANCH_BEHAVIORS` describes something the
pre-migration system never observed, so it has no pre-migration form for `mapping.json` to
preserve. The exemption covers **both** mechanisms — scoping it to extensions alone would
make it depend on which one an author reached for rather than on when the fact was learned.

**Direction and scope breadth are structural orders, not string comparisons** (FR-027).
Direction: `none` < agreement (`both-reject`/`both-accept`) < one-directional
(`reference-stricter`/`deacon-stricter`) < `field-divergence`. Scope: a single case
(`corpus_case:` / `case:` / `record:`) < a corpus (`corpus:`) < a behavior (`behavior:`).
Anything unrecognized is treated as maximally broad — fail-closed, so an unreviewed
spelling can never pass as narrow.

## Normalization rules (V24) — transitional

The human-readable companion to `conservation::check_normalization_rules` and the
`NORMALIZATION_RULES` registry in `crates/conformance/src/conservation.rs`
(023-migrate-parity-to-conformance, US4). Same validate.rs/RULES.md lockstep as every
other class.

A normalization rule decides what a comparison is allowed to **ignore**. Left
unconstrained it is the most effective way to make a parity suite pass while proving
less — and, unlike a weakened assertion, it is invisible in the test data: the case still
declares the channel and still reports `agree`. So every rule the harness applies is
registered with its scope, action, removal set and justification, and the registry is
checked.

| Class | Statement | Remedy |
|-------|-----------|--------|
| **V24** | **unscoped or unjustified normalization rule**: no scope, an `all`-style scope, or a scope that is neither `channel:<chan-id>` nor `field:<json-pointer>`; a `drop` with no justification or an empty `removes`; a `removes` entry that is open-ended (a glob, a prefix, or a category predicate such as "every empty value") rather than a field name; a non-`drop` rule that declares `removes`; or a rule declared `known_non_compliant` without a reason naming a tracked follow-up | Scope the rule and enumerate what it removes, or declare the deficiency honestly with a tracked follow-up. |

**Only a `drop` loses information**, so only a `drop` needs an enumerated `removes` and a
justification. The removal set must be a finite list of **field names**: an open-ended set
removes a *category*, which means a field added tomorrow disappears without anyone
deciding it should — the exact regression FR-021 exists to prevent.

### Declared deficiency vs undeclared blanket rule

This mirrors residual-vs-gap. An **undeclared** blanket rule fires V24 and blocks: the
problem is unadmitted. A **declared** one (`known_non_compliant`, carrying a reason that
names a tracked follow-up) is reported by `certify` as non-blocking debt, exactly like a
residual — it is admitted, explained and queued. Declaring is a conspicuous source edit
with a mandatory tracked reason, so it is not a cheap escape from the guard.

There are currently **no** declared deficiencies (024 Phase 4). The last one,
`strip_intentional_labels`, subtracted labels by four namespace PREFIXES rather than an
enumerated list — a category, so a label a future release added under `devcontainer.` /
`com.docker.` / `desktop.` / `dev.containers.` would have vanished from the comparison
with nobody deciding it should. It was **retired, not narrowed** (tasks.md#T112 closed):
container-state capture now keeps every label, and the per-CLI identity labels are
characterized where a reader can see them — a scoped, backed `allowedDifference` on the
case that compares them, and, until those cases land, an explicit named allowance inside
the one legacy carrier that still diffs labels (`parity_state_diff`), which dies with it.

The same change registered **`workspace_basename_token`** (rewrite, scope
`channel:chan-container-state`, removes nothing): each side of a differential runs in its
own isolated temp workspace, so a config with no explicit `workspaceFolder` yields
`/workspaces/<tmpA>` versus `/workspaces/<tmpB>`, and the full-path token cannot reach a
container-side path that never contains the host path. Without it every container-state
comparison would report a divergence that is an artifact of the runner's own isolation.

### `chan-container-state` is not snapshot-oracle material

The `snapshot` oracle replays committed evidence, so it requires evidence that is
byte-stable across recordings. `chan-container-state` is **not**: container ids, compose
project names and image ids survive tokenization, and they change on every run. A case on
this channel therefore belongs on `live-differential` (compare two sides in the same run)
or `spec-expectation` (assert a declared shape), never on `snapshot` — recording one would
produce a snapshot that replays stale on the next run for reasons that carry no meaning.
This is a property of the evidence, not a gap to close.

### What US4 retired (research D3)

`prune` — which removed every null, empty object, empty array and empty string anywhere in
a configuration document, plus `configFilePath` unconditionally — and `replace_hex12`,
which rewrote any 12-character lowercase-hex run in any string. Both had unbounded
removal sets and are **V24 by construction**; neither can be re-registered.

They are replaced by two named rules with finite, enumerated scope:

- **`drop_absent_optional`** — removes one of 46 enumerated `devcontainer.json` property
  names, and only when its value carries no information. An unlisted property is always
  compared, so a newly added one cannot vanish.
- **`devcontainer_id_token`** — rewrites the literal `${devcontainerId}`, and a 12-hex run
  only inside six enumerated id-bearing fields, so two genuinely different digests
  elsewhere can no longer be collapsed into one token.

Both are the *single* definition: `normalize::config_document_rules` is shared by the
legacy `config`/`merged_config` entry points and the declarative `chan-structured-output`
channel. `NORMALIZER_VERSION` was bumped `2` → `3`, so every committed snapshot goes stale
and is re-reviewed rather than silently replayed under new semantics.

Retiring them surfaced four genuine divergences that were previously hidden; they are
characterized on `bhv-readconfig-tier1-corpus` / `bhv-readconfig-merged-configuration`
(`reference: divergent`) and left **reporting**. No tolerance was authored and no blanket
rule reinstated (FR-036).

### `deacon-only` is not noise (FR-020)

`DiffKind` used to rank `deacon-only` last, documented as "usually default noise". A field
deacon emits and the reference does not is either a real extension or a real
over-emission; neither is noise, and combined with `prune` it meant such a field was
hidden when empty and buried when populated. All three difference classes are now reported
with equal significance; the ordering is a deterministic display order only.

### Residual vs gap (do not conflate)

| | `gaps.json` | `residuals.json` |
|---|---|---|
| What it admits | missing **coverage** — no evidence exists | missing **representation** — the coverage exists, carried by a program not yet retired |
| Blocks `certify`? | **Yes**, always | **No**, never (FR-054) — it is listed as information |
| What it does block | nothing else | deleting its `blockedCarrier` program (FR-013) |
| Resolution | add real evidence and delete the record | express the unit as data, then delete the record |

A residual must name a **specific** missing capability (never a vague "not supported
yet"). `blockedCarrier` is optional only for `external-corpus-entry` residuals, which block
no program because no program runs them.

#### Queued vs permanent residuals (024)

Not every residual is *debt*. Some units can never be expressed as data:

| | `disposition: queued` | `disposition: permanent` |
|---|---|---|
| What it means | migratable once a named capability exists | never migratable — a principle or a category mismatch forbids it |
| Requires | `followUp` (a tracked reference) | `outOfScopeRationale` (the ground) |
| Forbids | `outOfScopeRationale` | `followUp` — there is nothing to track |
| Expected trajectory | count falls to **zero** | count is stable; it is not a queue |

Exactly-one-of is enforced at **deserialize** time (`residual.rs`), not deferred to a
validation pass: a permanent residual carrying a tracked follow-up promises work that cannot
happen, and a queued one carrying a rationale claims to be excluded while asking to be fixed.
`disposition` defaults to `queued`, so a record written before this field existed keeps its
meaning.

**Why the split exists.** Folding permanent exclusions into the queue makes the queue
asymptote at a nonzero floor forever, and a number that can never reach zero cannot be read
as progress. `certify` therefore reports `residualQueue` and `permanentResiduals` separately,
and the migration report renders them under separate headings.

**V23 requires the rationale to name a ground**, not restate the exclusion: cite the
principle (e.g. "Constitution II: feature authoring is out of scope") or the specific
unmodellable mechanism (e.g. "no reference side, so the three-axis disposition has nothing to
record"). A bare "out of scope" is rejected — it is indistinguishable from unqueued debt.

The permanent set at 024 P1 is 61 units across 6 records: the 33 network-fetched real-world
corpus entries (research D8), the 17 harness fault-injection and 6 registry-structural units
(they observe the comparison machinery and the repository, not consumer behavior, so they have
no oracle and no observable channel), the 4 intra-deacon consistency units (no reference side,
hence `behaviors: []`), and the lockfile interop unit (Constitution II).

## Scenario model and obligation provenance (V26 – V27)

024-deterministic-conformance-coverage adds a **constrained scenario model** and a
generated **obligation** denominator on top of the behavior denominator. Two classes keep
that denominator honest: **V26** guards the hand-authored model, **V27** guards the
machine-generated inventory built from it.

### Two namespaces, deliberately never merged

| Namespace | File | Means | Evaluated against |
|---|---|---|---|
| `dim-*` (environment) | `dimensions.json` + `profiles.json` | **where** evidence can be gathered | the single active profile's assignment |
| `sdim-*` (scenario) | `scenario.json` | **what** a case exercises | a case's `scenarioContext` |

Scenario dimensions MUST NOT be added to `dimensions.json`. `applies_in_profile` treats a
condition on a dimension the profile does not assign as **unsatisfied**, so a scenario
dimension living in the environment model would silently drop every behavior constraining
it *out of profile* — removing it from the coverage denominator entirely. A feature built
to stop the denominator hiding things must not begin by hiding things, and the failure
would be invisible: `certify` counts only in-profile behaviors, so the number would go
*down* and still be green. `Condition` is reused verbatim for scenario applicability; only
the evaluator is new.

### Applicability is pure exclusion, and exclusion is attributable

```text
invalid(combination) ⇔ ∃ rule : ∀ condition ∈ rule.excludes :
                         combination[condition.dimension] ∈ condition.values
```

There is no "include" form, no precedence, and no ordering dependence — the predicate is a
disjunction, so evaluation order cannot change the answer. A rule constrains **only** the
dimensions it names. A **partial** combination that does not assign a dimension the rule
names is *inconclusive*, never excluded: excluding speculatively would drop pairs no rule
actually forbids.

An invalid combination leaves the denominator entirely, and **the excluding rule id travels
with it** into `coverage-pairwise.json`'s `excluded` list. This is the one place where
"silently absent" and "explicitly excluded" must not be confused: collapse the two and any
missing combination can be explained away as impossible, which makes the denominator
unfalsifiable.

A dimension **every** value of which is excluded under an operation is *inapplicable* there
and contributes no pairs. Pruning happens before enumeration, which is what keeps the space
tractable without a covering-array minimizer.

> **Note on the illustrative rule in data-model.md §2.** It excludes only the three
> container-ful states for the operations that create no container, leaving `none`. That is
> deliberate and the committed rule follows it, even though contracts/scenario-model.md
> describes the dimension as *inapplicable* to those operations. Excluding `none` as well
> would leave `read-configuration`, `build`, and `doctor` with **no valid total assignment
> at all**, and a case must assign every scenario dimension (data-model.md §3) — the
> operation would become unrepresentable. The dimension therefore survives with one value
> rather than being pruned outright.

### Obligations are generated, never authored

| File | Owner | Written by | Hand edits |
|---|---|---|---|
| `registry/scenario.json` | hand | never generated | expected |
| `registry/applicability.json` | hand | never generated | expected |
| `obligations/obligations.json` | **machine** | `coverage generate` ONLY | **V27** |

`coverage generate` writes **exactly one file** and never a disposition, case, behavior,
waiver, gap, or report. This is the 020/021 boundary restated, because it is the invariant
most easily lost: a generator that could edit a disposition would convert human review into
a build artifact.

Two obligation kinds, **never multiplied together** (FR-019):

- `obl-bhv-<hash8>` — a behavior paired with the context its own applicability requires.
  **Exactly one per behavior.** An empty applicability is one universal context (zero
  obligations would erase the behavior from the denominator); a non-empty one *is* the
  context, because a condition pins a value **subset** meaning "any of these". Expanding a
  subset into one obligation per value would multiply the two kinds against the environment
  model and produce the unreviewable thousands research Decision 2 rejected.
- `obl-cmb-<hash8>` — a valid pair (`arity: 2`) or a hand-selected high-risk triple
  (`arity: 3`), partitioned by operation.

The **operation is a partition key, never a pair member**: a pair covered under `up` does
not cover that pair under `down`, because the same pair means different things per
operation and pooling would let one operation's coverage mask another's.

Ids are **substance-anchored**, following the `clu-` precedent: reordering records,
renaming a file, or writing the same pair's keys in a different order leaves the id alone,
so a cosmetic edit never orphans a hand-authored disposition. Changing what a combination
*is* does change the id — and that is a new obligation needing its own decision.

### The classes

| Class | Fires on | Remedy |
|---|---|---|
| **V26** | a **dead value** (declared but permitted in no valid combination — FR-010); an empty or duplicated value set; a missing required dimension (FR-003); a rule naming an unknown dimension or value; a rule with fewer than two conditions; a rule whose `ground` is filler rather than an argument; a high-risk triple that pins no operation, pins other than three dimensions, carries a filler `reason`, or selects an excluded combination; a case `scenarioContext` that is partial, names something undeclared, or is itself an excluded combination | Remove the value or narrow the rule that strands it; declare the dimension; name the mechanism in the ground; assign **every** scenario dimension on a case, or none |
| **V27** | the committed inventory does not byte-match a fresh regeneration; its `revision` names no `spec`-kind revision record; a unit references an operation, dimension, value, or behavior the registry no longer declares; **or** the registry declares a scenario model but ships no inventory at all | `cargo run -p deacon-conformance -- coverage generate` |

**A hand edit and a stale regeneration are indistinguishable to V27, and both fail.** That
is the point: there is no way to edit the generated file that the check would treat as
legitimate. A missing inventory is likewise a violation and not a skip — an absent file
would otherwise read as "nothing to check".

A registry that declares **no** scenario dimensions opts out of both classes: there is no
model to be broken, so the fixture registries predating this feature stay silent rather
than reporting six missing dimensions each.

### A rule's `ground` must argue, not assert

V26 rejects filler with the same vocabulary V23 applies to `missingCapability`, plus a
prose floor. It deliberately does **not** reuse `names_an_exclusion_ground`, whose markers
("constitution", "principle", "authoring") are tuned for permanent *out-of-scope* claims: an
applicability ground argues from a **mechanism** ("this operation never creates a container,
so a container state is not a property it can exercise"), so requiring those markers would
reject every correct ground.

### Drift workflow (pin bump, dimension edit, or rule edit)

1. `coverage generate` — regenerate the inventory.
2. `coverage check` — confirms the commit matches; a mismatch is V27 and names the first
   differing unit id and whether it was added, removed, or changed.
3. `validate` — V26 lists dead values; V28 (User Story 2) enumerates the new
   undispositioned queue.
4. `coverage scaffold` (User Story 2) — skeletons to stdout with `UNREVIEWED` sentinels the
   loader rejects.
5. Disposition until `certify` unblocks.

**Disposition is never inherited by name.** A regenerated obligation that happens to
resemble a removed one is a **new** obligation and needs its own decision — the same rule
020 states for classifications, for the same reason: a name is not evidence.

### Reporting never gates, and gating never reports

`coverage report` is read-only with respect to the record and its **exit code never
reflects what the report says**. A command that both measured coverage and decided the
build's fate would make widening the report the cheapest way to go green. The gates are
`validate` (V26, V27) and `certify` (V28, V29 — User Story 2).

---

## Obligation dispositions (V28 – V29)

V26/V27 make the obligation set *honest*. V28/V29 make sure someone **decided** about each
one. The split matters: an inventory that is provably complete and provably regenerated
still says nothing about whether the work is covered, waived, argued away, or simply
untouched. That question has exactly one home — a hand-authored `odp-` record under
`registry/obligation-dispositions/<area>.json`.

### Four words, one payload each

| `disposition` | Requires | Blocks `certify`? |
|---|---|---|
| `case` | `cases` — ≥1, each resolving to a declared case | no |
| `non-testable` | `rationale` naming a ground | no |
| `waived` | `waiver` — a resolvable `wvr-` | only once it has **expired** (V6) |
| `gap` | `gap` — a resolvable `gap-` | **always**, through that gap record |

The arity *within* a record — exactly one of the four payloads, and the one its word
requires — is refused at **load** time, not by V28/V29. A record whose payload disagrees
with its word is not a nuance for a validation pass to interpret; it is a half-stated
judgement, and the only honest reading is to refuse it at the door. The same loader rejects
the `UNREVIEWED` scaffold sentinel, so `coverage scaffold` output can never be committed
unedited.

### Explicit only: no inheritance, no default, no winner

An obligation with **no** record is undispositioned — **V28**, not "implicitly fine". There
is no document-scope fallback of the kind `clc-` clause classifications use for authoring
documents, because a scenario combination has no document to inherit from and an obligation
nobody looked at is not a decision anyone made.

An obligation with **more than one** record is also **V28**. Resolution never picks a
winner: two records are two judgements, and silently preferring the one that sorts first
would turn a disagreement between reviewers into a decision neither of them made. The
coverage report buckets such an obligation `undispositioned` for the same reason — it does
*not* fall through to the evidence, because the evidence answers "nobody has looked at this
yet" and someone plainly has, twice.

`inactive-environment` is a **reporting bucket, not a disposition.** It is derived from the
active profile and outranks even an explicit record. If an author could out-vote it,
"the environment is inactive" would become a way to retire an obligation rather than a
statement about the profile. An inactive obligation owes nobody a decision, stays in the
denominator, and never counts as covered.

### Gap vs. waiver vs. rationale, restated for obligations

[Gap vs. waiver](#gap-vs-waiver) draws the line for behaviors; obligations add a third
option, and the three are not interchangeable:

| | `gap` | `waived` | `non-testable` |
|---|---|---|---|
| The claim | "we owe coverage here and do not have it" | "we know exactly how this differs, and accept it" | "there is nothing here that could be tested" |
| Backed by | a `gap-` record | a `wvr-` record with an `expires` | an argument, and nothing else |
| Blocks a release | **always** | only once expired | never |
| Ends by | someone writing a case | someone re-confirming, or fixing | never — it is a permanent claim |

`non-testable` is the only one of the three that admits **no** follow-up, which is why V29
holds its `rationale` to the same ground-naming test V23 applies to `outOfScopeRationale`.
A bare "out of scope" is indistinguishable from unqueued debt — and unqueued debt is what a
gap is for. Name the principle ("Constitution II forbids feature authoring") or the specific
unobservable mechanism, or use `gap` and be honest that it is work.

The reverse mistake is just as bad: reaching for `gap` when the difference is already
characterized inflates the release blocker list with things nobody intends to change, and a
blocker list that is mostly noise stops being read.

### A high-risk triple accepts only `case` or `gap`

**V29** rejects a triple dispositioned `non-testable` or `waived` (FR-015). Triples are not
generated — they are hand-selected, precisely because interaction defects hide where
individually-covered dimensions meet. An argument that such an interaction needs no test is
the one argument the model does not accept: either exercise it, or admit the gap.

**And the case it names must be EXECUTABLE** (024 US3). FR-015 asks for an executable case,
not merely a declared one, so a triple dispositioned `case` against a legacy carrier whose
residual has closed names a program that no longer exists. On any other obligation that is
a coverage question; on a triple — the one place an argument may not stand in for evidence
— a dead pointer is the quietest possible way to lose the evidence entirely.

Note that `gap` remains available on a triple and remains blocking. It is honest, and it is
not sufficient: **SC-003 requires 100% of selected triples to be covered by executable
cases**, which is asserted per triple in `crates/conformance/tests/workflow_coverage.rs`.

### Stale dispositions, and why they are reported rather than dropped

An obligation's id is substance-anchored. Renaming a dimension value or editing a rule
re-hashes every obligation that pins it, and the records that judged the old ones are left
pointing at nothing. **V29** reports each as stale — the same self-invalidating pattern
`waiver.rs` uses for a waiver whose difference stopped reproducing.

Dropping them quietly would be worse than useless: the regenerated obligations come back
**undispositioned** (V28) at the same moment, so a silent drop would delete the record of
what was decided about the old shape exactly when a reviewer needs it to decide about the
new one. **Disposition is never inherited by name** — a regenerated obligation that
resembles a removed one is a new obligation.

### A waiver backing a disposition must be specific

**V29** rejects a `waived` disposition whose waiver carries a **blanket** scope — a
`state_field` scope whose `field` is `*` or empty, matching every observable field of the
fixture. This is the FR-023 analogue of the FR-032 rule V19 already enforces on an allowed
difference's `observablePath`, and it exists for the same reason: a tolerance that matches
everything can never self-invalidate when the difference stops reproducing, so it is a
global ignore wearing a waiver's clothes. A `corpus_case` scope names one case of one
corpus and has no wildcard form, so there is nothing there to reject.

### What `certify` does with all of this

| Condition | Result |
|---|---|
| Any undispositioned or multiply-dispositioned applicable obligation | **blocks**, `BlockingKind::Obligation`, code `V28` |
| Any malformed or stale disposition | **blocks**, code `V29` |
| Any `gap` disposition | **blocks** — through the `gap-` record it names, as `BlockingKind::Gap` |
| Any `waived` disposition whose waiver has expired | **blocks**, code `V6`, naming the **waiver** |
| `non-testable`, unexpired `waived` | listed in their own buckets, non-blocking |
| `inactive-environment` | listed, non-blocking, counted separately |

A `gap` disposition gets no blocker of its own because it already has one: V29 requires the
`gap-` it names to resolve, and every gap record blocks. Listing it twice would double-count
a single fact.

An expired waiver has never blocked certification by itself, and still does not — a waiver
is a decision that no further work is needed, and its expiry is a prompt to re-confirm. An
obligation dispositioned `waived` is different: that waiver is the only thing standing
between it and *undispositioned*, so when the waiver dies, nothing stands there.

The five FR-026 buckets (`covered` / `waived` / `non-testable` / `gap` /
`inactive-environment`) plus the undispositioned queue are reported **alongside** the
behavior-level coverage numbers and never folded into them. The two denominators answer
different questions — which behaviors are evidenced, versus which modelled combinations are
exercised — and collapsing them would let progress on one hide the absence of progress on
the other.
