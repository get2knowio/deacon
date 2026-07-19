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
