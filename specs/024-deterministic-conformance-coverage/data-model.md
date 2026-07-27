# Phase 1 Data Model: Deterministic Conformance Coverage

**Feature**: 024-deterministic-conformance-coverage
**Date**: 2026-07-26

All records are strict JSON (`deny_unknown_fields`), version-controlled, and loaded through
the existing `deacon-conformance` loader. Ordering is declaration order (`IndexMap`) or
id-sorted, never `HashMap` iteration order.

## Ownership boundary

The single rule that keeps generation from overwriting judgement, inherited from 020/021:

| File | Owner | Written by | Hand edits |
|---|---|---|---|
| `conformance/registry/scenario.json` | hand | never generated | expected |
| `conformance/registry/applicability.json` | hand | never generated | expected |
| `conformance/obligations/obligations.json` | machine | `coverage generate` only | **V27** |
| `conformance/registry/obligation-dispositions/<area>.json` | hand | never generated | expected |
| `conformance/registry/regressions.json` | hand | never generated | expected |
| `target/conformance/coverage-*.{json,md}` | machine | `coverage report` | git-ignored |

`coverage generate` **never** touches a disposition file. `coverage scaffold` emits skeleton
dispositions to **stdout** with an `"UNREVIEWED"` sentinel the loader rejects.

---

## 1. Scenario dimension (`sdim-`) — `registry/scenario.json`

```jsonc
{
  "schemaVersion": 1,
  "records": [
    {
      "id": "sdim-operation",
      "kind": "scenario",
      "description": "The consumer operation under test.",
      "values": ["read-configuration", "build", "up", "exec", "run-user-commands",
                 "down", "outdated", "upgrade", "templates-apply", "doctor"]
    }
  ]
}
```

| Field | Rule |
|---|---|
| `id` | `sdim-<slug>`; unique across **all** id namespaces (V2) |
| `kind` | always `"scenario"`. Environment dimensions keep the `dim-` prefix in `dimensions.json` and are **not** represented here (research Decision 1) |
| `values` | closed, non-empty, unique, declaration-ordered |

**Required dimensions** (FR-003): `sdim-operation`, `sdim-config-source`,
`sdim-container-state`, `sdim-features`, `sdim-layering`, `sdim-output-mode`.

Minimum value sets mandated by FR-005 – FR-009:

| Dimension | Values |
|---|---|
| `sdim-config-source` | `image`, `dockerfile`, `compose` |
| `sdim-container-state` | `none`, `stopped`, `running`, `running-stale-config` |
| `sdim-features` | `none`, `single`, `multiple-declared-order`, `multiple-dependency-order`, `lockfile` |
| `sdim-layering` | `single`, `extends-chain`, `cli-overlay`, `image-metadata` |
| `sdim-output-mode` | `structured`, `human` |

**Invariant (V26)**: every value must be permitted by at least one valid combination. A value
excluded everywhere is a **dead value** and is reported, never carried (FR-010).

---

## 2. Applicability rule (`rule-`) — `registry/applicability.json`

```jsonc
{
  "id": "rule-no-container-state-without-container",
  "excludes": [
    { "dimension": "sdim-operation", "values": ["read-configuration", "build", "doctor"] },
    { "dimension": "sdim-container-state", "values": ["stopped", "running", "running-stale-config"] }
  ],
  "ground": "These operations never inspect or create a container, so a container state is not a property they can exercise."
}
```

| Field | Rule |
|---|---|
| `excludes` | ≥2 `Condition`s (the existing `model::Condition`, reused verbatim), each naming a declared `sdim-` and a subset of its values |
| `ground` | required, non-filler; states *why* the combination cannot exist (V26) |

**Semantics**: a candidate combination is **invalid** iff, for some rule, every listed
condition is satisfied by the combination. A rule constrains only the dimensions it names;
unnamed dimensions are unconstrained.

**Invariant**: an invalid combination is excluded from the denominator entirely and the
excluding rule id is carried into the report (FR-012). This is the one place where "silently
absent" and "explicitly excluded" must not be confused — exclusion is attributable.

---

## 3. Scenario context on a case — `registry/cases/<area>.json`

`TestCase` gains one optional field. The existing `context: Vec<Condition>` continues to mean
**environment** context and is unchanged.

```jsonc
{
  "id": "case-up-compose-multi-service",
  "behaviors": ["bhv-up-compose-project-resources"],
  "context": [],
  "scenarioContext": {
    "sdim-operation": "up",
    "sdim-config-source": "compose",
    "sdim-container-state": "none",
    "sdim-features": "none",
    "sdim-layering": "single",
    "sdim-output-mode": "structured"
  }
}
```

| Rule | Class |
|---|---|
| Every key is a declared `sdim-`, every value a declared value of it | V26 |
| A declarative case MUST assign **every** scenario dimension — partial assignment would make "which pairs does this cover?" ambiguous | V16 (extended) |
| The assignment MUST NOT be an invalid combination | V26 |
| `scenarioContext` participates in `caseHash` — changing what a case exercises re-records its snapshot | — |

A **legacy** case may omit `scenarioContext`; it then covers no combination obligation,
consistent with the spec's Edge Case ruling that legacy carriers satisfy obligations only
while an open residual names them.

---

## 4. Obligation (`obl-`) — `obligations/obligations.json` *(machine-owned)*

```jsonc
{
  "schemaVersion": 1,
  "revision": "rev-spec-113500f4",
  "units": [
    {
      "id": "obl-cmb-3f9a2c17",
      "kind": "combination",
      "operation": "up",
      "assignment": { "sdim-config-source": "compose", "sdim-features": "lockfile" },
      "arity": 2
    },
    {
      "id": "obl-bhv-8b1e04da",
      "kind": "behavior",
      "behavior": "bhv-up-compose-project-resources",
      "context": []
    }
  ]
}
```

**Identity** is substance-anchored, following the `clu-` precedent so a cosmetic move never
changes an id:

- `obl-bhv-<hash8>` over `behavior ‖ canonical(context)`
- `obl-cmb-<hash8>` over `operation ‖ canonical(sorted assignment)`

`hash8` is the existing `deacon-conformance` helper. Assignment keys are sorted before
hashing so authoring order cannot fork an id.

**Generation** (FR-013, FR-013a, FR-013b, deterministic by construction):

1. For each `sdim-operation` value *o*:
   1. Determine which remaining scenario dimensions are applicable under *o* (a dimension all
      of whose values are excluded with *o* is inapplicable and contributes nothing).
   2. For each unordered pair of distinct applicable dimensions, for each pair of values,
      emit a `combination` obligation **iff** the pair is not excluded by any rule.
2. For each selected high-risk triple, emit a `combination` obligation with `arity: 3`.
3. For each behavior, for each context its `applicability` requires, emit a `behavior`
   obligation.
4. Sort by id; write atomically.

Environment dimensions never enter step 1 (FR-013b). The **full Cartesian product is never
materialized** — pairs are enumerated directly.

**Invariants**:

| Rule | Class |
|---|---|
| Committed file byte-equals a fresh regeneration | **V27** |
| `revision` equals the registry's schema/spec pin | **V27** |
| No obligation references a removed dimension value | **V27** |
| A hand edit is indistinguishable from staleness and is caught by the same check | **V27** |

---

## 5. High-risk triple (`hrt-`) — `registry/applicability.json`

```jsonc
{
  "id": "hrt-compose-features-restart",
  "assignment": {
    "sdim-operation": "up",
    "sdim-config-source": "compose",
    "sdim-features": "multiple-dependency-order",
    "sdim-container-state": "running"
  },
  "reason": "Feature install order interacts with Compose service startup and with re-entry into an existing container; each pair is individually covered but the interaction is where ordering defects have historically appeared."
}
```

Hand-authored, never machine-derived (FR-016). `reason` is required. The assignment names an
operation plus exactly three other dimensions.

**Invariant (V29)**: a triple obligation may be dispositioned **only** by an executable case
or a gap. A rationale or waiver on a triple is rejected — FR-015 makes the triple set the one
place where an argument is not an acceptable substitute for evidence.

---

## 6. Obligation disposition (`odp-`) — `registry/obligation-dispositions/<area>.json`

```jsonc
{
  "id": "odp-up-compose-lockfile",
  "obligation": "obl-cmb-3f9a2c17",
  "disposition": "case",
  "cases": ["case-up-compose-lockfile"]
}
```

Exactly one of four dispositions, mirroring the classification arity rules of V13:

| `disposition` | Requires | Forbids | Blocks `certify`? |
|---|---|---|---|
| `case` | `cases` (≥1, all resolvable, all executable) | `rationale`, `waiver`, `gap` | no |
| `non-testable` | `rationale` naming a ground | `cases`, `waiver`, `gap` | no |
| `waived` | `waiver` (a resolvable `wvr-` with `expires`) | `cases`, `rationale`, `gap` | only if expired (**V6**) |
| `gap` | `gap` (a resolvable `gap-`) | `cases`, `rationale`, `waiver` | **always** |

**Invariants**:

| Rule | Class |
|---|---|
| Every applicable obligation has exactly one disposition — zero or >1 both fail | **V28** |
| `rationale` names a ground, not a filler phrase (same test as V23's `outOfScopeRationale`) | **V29** |
| A triple dispositioned `non-testable` or `waived` | **V29** |
| A disposition whose `obligation` resolves to nothing (stale) | **V29** |
| An obligation in an inactive environment is `inactive-environment` — reported, never counted as covered, never blocking | — |

**Resolution order** when an obligation could be satisfied several ways: an explicit
disposition always wins; there is no inheritance and no default. Disposition is never
inherited by name — the same rule 020 states for classifications on a pin bump.

---

## 7. Regression record (`reg-`) — `registry/regressions.json`

```jsonc
{
  "id": "reg-chan-image-label",
  "channel": "chan-image",
  "target": "image-inspect-document",
  "perturbation": { "kind": "set-json-pointer", "pointer": "/Config/Labels/devcontainer.metadata", "value": "injected" },
  "expectedDetectingCases": ["case-build-image-metadata-labels"]
}
```

| Field | Rule |
|---|---|
| `channel` | a declared `chan-`; every declared channel needs ≥1 record (**V30**) |
| `target` | the **evidence source** the perturbation is applied to — a process result, an inspect document, or file bytes. Never an observer's return value (research Decision 5, FR-065b) |
| `perturbation` | a closed, declarative shape; applied and reverted by the harness (FR-066) |
| `expectedDetectingCases` | informational; the run reports what *actually* detected it |

**Verdict**: `detected` when ≥1 case fails with the failure attributed to `channel`;
`inert` otherwise. Any `inert` channel fails the acceptance run (FR-067).

**Invariant (V30)**: a record naming a channel with no registered observer is rejected — it
could never be detected, so declaring it would manufacture a false `inert`.

---

## 8. Derived evaluations (not stored)

| Evaluation | Definition |
|---|---|
| **Valid combination space** | All assignments not excluded by any rule, per operation |
| **Covered pair** | A pair for which ≥1 declarative case's `scenarioContext` matches both values under the same operation |
| **Coverage bucket** | `covered` / `waived` / `non-testable` / `gap` / `inactive-environment` — five buckets, never folded (FR-026) |
| **Dead value** | A declared value appearing in no valid combination (V26) |
| **Inert channel** | A channel whose every regression record went undetected (FR-067) |
| **Stale disposition** | A disposition whose obligation no longer exists (V29) |

---

## 9. Violation classes added

Stated in `RULES.md` in the same row format as V1–V24, preserving the `validate.rs` ↔
`RULES.md` lockstep the violation-class index exists to make checkable. V25 is retired, so
numbering resumes at V26.

| Class | Statement | Gates |
|---|---|---|
| **V26** | scenario-model integrity: dead dimension value; rule naming an unknown dimension/value; rule with no ground; case `scenarioContext` that is partial, undeclared, or invalid | `validate` (every PR) |
| **V27** | obligation provenance: committed ≠ regenerated; `revision` mismatch; obligation referencing a removed value | `validate` (every PR) |
| **V28** | an applicable obligation with zero dispositions or more than one | `validate` + `certify` |
| **V29** | malformed disposition: filler rationale; triple dispositioned by rationale/waiver; stale disposition | `validate` + `certify` |
| **V30** | injected-regression integrity: declared channel with no record; record targeting a channel with no observer | regression acceptance run |

`certify` gains `BlockingKind::Obligation`, carrying the specific class in its `code` field —
the same shape `Constraint` and `Clause` already use.

---

## 10. Relationship to existing entities

| Existing | Relationship |
|---|---|
| `Condition` | **Reused verbatim** for scenario applicability. Same shape, new evaluator (research Decision 1) |
| `dim-*` / `profiles.json` | **Untouched.** Continue to mean environment. `profiles.json` gains no scenario keys |
| `BehaviorUnit.applicability` | Unchanged; feeds behavior obligations |
| `TestCase.context` | Unchanged (environment). New sibling `scenarioContext` |
| `wvr-` waivers | Reused as a disposition target; expiry semantics (V6) unchanged |
| `gap-` gaps | Reused as a disposition target; always-blocking semantics unchanged |
| `res-` residuals | Unchanged. A residual keeps a legacy carrier alive, which keeps its obligations satisfied — the Edge Case ruling |
| `NORMALIZER_VERSION` | Bumped when US5 de-suppresses fields; participates in snapshot staleness as before |
