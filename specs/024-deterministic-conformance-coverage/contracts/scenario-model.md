# Contract: scenario model and applicability

Defines the record shapes and evaluation semantics for the constrained context model. Full
field tables live in [data-model.md](../data-model.md) §1–§3; this file fixes the **semantics**
that generation, validation, and reporting all depend on.

## Two namespaces, deliberately separate

| Namespace | File | Means | Evaluated against |
|---|---|---|---|
| `dim-*` (environment) | `registry/dimensions.json` + `profiles.json` | **Where** evidence can be gathered | The single active profile's assignment |
| `sdim-*` (scenario) | `registry/scenario.json` | **What** a case exercises | A case's `scenarioContext` |

These MUST NOT be merged. `CertificationProfile.context` assigns each declared dimension
exactly one value, and `applies_in_profile` treats a condition on an unassigned dimension as
**unsatisfied** — so a scenario dimension placed in `dimensions.json` would silently drop every
behavior constraining it out of profile, shrinking the coverage denominator (research
Decision 1). A feature built to stop the denominator hiding things must not begin by hiding
things.

## Applicability evaluation

A **candidate combination** is a total assignment of scenario dimensions.

```text
invalid(combination) ⇔ ∃ rule ∈ rules :
                         ∀ condition ∈ rule.excludes :
                           combination[condition.dimension] ∈ condition.values
```

- A rule constrains **only** the dimensions it names; unnamed dimensions are unconstrained.
- Rules are pure exclusions. There is no "include" form, no precedence, and no ordering
  dependence — the predicate is a disjunction over rules, so evaluation order cannot change
  the answer.
- An invalid combination is removed from the denominator entirely, and the **excluding rule
  id travels with it** into the report (FR-012). Exclusion is attributable; silence is not.

## Applicability of a *dimension* to an operation

A dimension *d* is **inapplicable** under operation *o* when every value of *d* is excluded in
combination with *o*. An inapplicable dimension contributes no pairs under that operation.

This is what keeps the obligation set tractable: `sdim-container-state` is inapplicable to
`read-configuration`, `build`, and `doctor`; `sdim-features` is inapplicable to `down` and
`doctor`; `sdim-output-mode` is inapplicable to operations emitting no structured document.
Pruning happens before enumeration, not after.

## Pair enumeration

For operation *o*, for each unordered pair of distinct **applicable** dimensions {*d₁*, *d₂*},
for each (*v₁*, *v₂*) ∈ values(*d₁*) × values(*d₂*):

> emit a combination obligation **iff** no rule excludes {*o*, *v₁*, *v₂*}.

The operation is a **partition key**, never a pair member: a pair covered under `up` does not
cover that pair under `down` (FR-013a). Environment dimensions never participate (FR-013b).

The full Cartesian product over all dimensions is **never materialized** — pairs are
enumerated directly from the two-dimension cross product, which is what makes the space
tractable without a covering-array minimizer (research Decision 3).

## Coverage matching

A pair obligation `(o, d₁=v₁, d₂=v₂)` is **covered** iff some declarative case satisfies all
three:

1. `scenarioContext[sdim-operation] == o`
2. `scenarioContext[d₁] == v₁` and `scenarioContext[d₂] == v₂`
3. the case is executable — not a legacy carrier, except while an open residual names it

Because a case assigns **every** dimension, one case covers `C(n,2)` pairs at once under its
operation. This is why the pair space is fillable at all: coverage is dense, and the report's
job is to name the sparse remainder.

## Dead values

A declared value appearing in no valid combination is a **dead value** (V26). It is reported,
never silently carried. Dead values arise from rule edits — adding a rule can strand a value —
which is exactly when a silently-carried value would misrepresent the model's size.

## What is NOT in this contract

| Not here | Where |
|---|---|
| Obligation identity and hashing | [obligation.md](./obligation.md) |
| Disposition resolution and arity | [obligation.md](./obligation.md) |
| Report shapes | [coverage-report.md](./coverage-report.md) |
| Automatic case generation | Refused — FR-016 reserves selection for humans |
