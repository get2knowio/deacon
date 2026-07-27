# Contract: obligation generation and disposition

Fixes obligation **identity**, **generation order**, and **disposition resolution**. Record
shapes are in [data-model.md](../data-model.md) §4–§6.

## Identity is substance-anchored

Following the `clu-` clause precedent, an obligation's id is derived from what it *is*, not
from where it sits:

| Kind | Id | Hashed over |
|---|---|---|
| behavior | `obl-bhv-<hash8>` | `behavior ‖ canonical(context)` |
| combination | `obl-cmb-<hash8>` | `operation ‖ canonical(sorted assignment)` |

`hash8` is the existing helper. **Assignment keys are sorted before hashing**, so two authors
writing the same pair in different key order produce the same id — otherwise a disposition
would silently detach when someone reformatted a file.

**Consequence**: reordering records, renaming a file, or moving a dimension's declaration
position does **not** change an id, so it does not orphan a hand-authored disposition. Changing
what a combination *is* does change the id — and that is a new obligation needing its own
decision, which is correct.

## Generation is total and ordered

```text
1. for each operation o          (declaration order of sdim-operation)
2.   prune dimensions inapplicable under o
3.   for each unordered pair of remaining dimensions
4.     for each value pair not excluded by a rule
5.       emit obl-cmb (arity 2)
6. for each high-risk triple      (declaration order)
7.   emit obl-cmb (arity 3)
8. for each behavior              (id order)
9.   for each context its applicability requires
10.     emit obl-bhv
11. sort all units by id; write atomically
```

Step 11 makes declaration order irrelevant to the output, so the file is stable under
reformatting of its inputs. Nothing in the pipeline reads the clock, the filesystem layout, or
a hash map's iteration order.

## Generation never touches judgement

| Writes | Never writes |
|---|---|
| `conformance/obligations/obligations.json` | any `obligation-dispositions/*.json` |
| | any case, behavior, waiver, or gap |
| | any report |

This is the 020/021 boundary, restated because it is the invariant most easily lost: a
generator that could edit a disposition would convert human review into a build artifact.

## Disposition resolution

Exactly one disposition per applicable obligation. There is **no inheritance and no default** —
an obligation with no explicit disposition is undispositioned (V28), not implicitly fine.

| Disposition | Requires | Blocks `certify` |
|---|---|---|
| `case` | ≥1 resolvable executable case | no |
| `non-testable` | `rationale` naming a ground | no |
| `waived` | resolvable `wvr-` with `expires` | only when expired (V6) |
| `gap` | resolvable `gap-` | **always** |

### Rules that make the vocabulary honest

1. **A high-risk triple accepts only `case` or `gap`** (V29, FR-015). Rationale and waiver are
   rejected: the triple set is the one place the spec insists an argument cannot substitute for
   evidence, since triples are selected precisely because interaction defects hide there.
2. **A `non-testable` rationale must name a ground** — a principle (e.g. "Constitution II
   forbids feature authoring") or a specific unobservable mechanism. A bare "out of scope" is
   rejected by the same test V23 applies to `outOfScopeRationale`, because it is
   indistinguishable from unqueued debt.
3. **A disposition whose obligation no longer resolves is stale** (V29) and is reported, not
   quietly dropped — the self-invalidating pattern `waiver.rs` already uses.
4. **`inactive-environment` is a reporting bucket, not a disposition.** It is derived from the
   active profile, never hand-authored. It neither blocks nor counts as covered (spec
   Assumption 11).

## Certification integration

`certify` gains `BlockingKind::Obligation`, carrying the class in `code` — the same shape
`Constraint` (V11–V14) and `Clause` (V11–V15) already use, so the output format does not fork.

| Condition | Result |
|---|---|
| Any undispositioned applicable obligation | **blocks**, code `V28` |
| Any malformed or stale disposition | **blocks**, code `V29` |
| Any `gap` disposition | **blocks** (existing gap semantics) |
| Any expired waiver disposition | **blocks**, code `V6` |
| `non-testable` / unexpired `waived` | listed, non-blocking |
| `inactive-environment` | listed, non-blocking, counted separately |

## Drift workflow (pin bump or rule edit)

1. `coverage generate` — regenerate.
2. `coverage check` — confirms the commit matches; a mismatch is V27.
3. `validate` — V26 lists dead values; V28 enumerates the new undispositioned queue.
4. `coverage scaffold` — skeletons to stdout, with `UNREVIEWED` sentinels the loader rejects.
5. Disposition until `certify` unblocks.

**Disposition is never inherited by name.** A regenerated obligation that happens to resemble a
removed one is a new obligation and needs its own decision — the same rule 020 states for
classifications, for the same reason: a name is not evidence.
