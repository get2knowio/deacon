# Phase 2 — draining the deferrals (in-repo record of the "024" work)

**Status**: in progress. Steps 1–4 complete, step 5 partial, steps 6–8 not started.

This document exists because the work below was driven by a plan that lived outside
the repository. Four substantial commits landed with their rationale recorded only in
commit messages and an untracked file on one machine. That is the drift `research.md`
exists to prevent, so the plan and its decisions are recorded here.

**Scope**: this is not a new spec. Every step drains a deferral 023 itself recorded
(T103, T104, T107, T108, T110), so by this repository's own rule — *"a spec is NOT
complete while deferred tasks remain unresolved"* — finishing them is finishing 023.
Work that is genuinely new (the defect fixes below) is called out as such.

## Why this went beyond migration

Planning the five deferrals surfaced three defects in the surviving test tooling and one
stale residual. Each was verified directly, not inferred:

| # | Defect | Evidence |
|---|---|---|
| D-1 | `ReportFragment::write_under` wrote one file per BINARY. `parity_state_diff` has 8 test fns, each its own nextest process → last writer wins, 1 case recorded of 8 | The exact under-count 023 existed to fix, alive in the carriers 023 did not retire |
| D-2 | `verdict_differential` returned `agree` when NEITHER side observed a channel | A Docker hiccup was a silent green pass |
| D-3 | `live_differential` kept deacon's container alive while the oracle ran | Any fixed-host-port case deadlocked — why `appport-published-ports` was called unmigratable |
| stale | `res-exec-per-side-argv` claimed deacon needed per-side argv | `cli.rs` already accepts `--remote-env` as the primary flag; the residual was obsolete, not pending |

## Design commitments

**Do not extend the assertion language.** `cases.json` gains no new predicate and no new
operation field. Where an assertion appears to need a search, add a *derived field to the
observer*, not a query engine to the assertion language (`workspaceBindTargets` is the
worked example). Spec 022's SC-001 — adding a case is a pure data edit — holds throughout.

**Reuse, don't reimplement.** `normalize::container_state` was already pure over a
`docker inspect` object, so the new observer is delegation, and `diff_states` /
`Divergence` / `field_matches` / `Scope::StateField` become deletable — retiring a genuine
second implementation of comparison.

**Measure before classifying.** Every divergence below was settled with Docker and the
pinned oracle, never by reasoning about what a difference "must" mean. This produced the
opposite answer twice (see step 5).

## Steps

| # | Commit | What |
|---|---|---|
| 1 | `d837637` | Residuals split `queued` vs `permanent`, so "the queue reaches zero" is a meaningful claim rather than an asymptote. 21 queued / 61 permanent |
| 2 | `7d28362` | D-1: one report file per CASE, plus gate 7 — every baseline unit a carrier still carries must actually be reported |
| 3 | `b7ebd1b` | D-2 + D-3: an unobserved differential fails loud; deacon's side is reclaimed before the oracle's runs |
| 4 | `8b81f52` | `chan-container-state` becomes an observed channel; `strip_intentional_labels` retired (`nonCompliantRules` → 0). Plus 15 review findings |
| 5 | `1a502e5`, `e243921`, `2547340`, `3489f6d` | Container-state units migrate (69 → 74). Three deacon defects found; two fixed |
| 6 | — | `${CONTAINER_ID}` argv token; delete `parity_up_exec`, `parity_exec` |
| 7 | — | `require_buildkit()`; image-by-name observation; delete `parity_build` |
| 8 | — | Close out migration classes; retire V21/V22 or justify keeping them |

## Decisions worth keeping

**Residual `disposition`.** A residual admits missing *representation*; a gap admits
missing *coverage*. But 61 of 82 residual units can never be expressed — they observe the
harness itself, or need feature-authoring commands Constitution II forbids. Leaving them in
a queue implied pending work that will never happen, so `permanent` requires an
`outOfScopeRationale` naming the ground, and `queued` requires a `followUp`.

**`POST_BRANCH_BEHAVIORS`.** Three rules — the frozen behavior denominator, V21's exception
mapping, the migration report's condition 2 — assumed every behavior and exception traces to
the pre-migration world. A newly *observed* fact cannot satisfy that: it has no pre-migration
form, so the only ways through are to fabricate one or not record the divergence. Rather than
raise the frozen count (which re-arms the guard one notch higher forever), each such behavior
is enumerated with the reason it is not a variant, and the allowance is self-invalidating.
Exception exemption is DERIVED from that list, in one shared
`conservation::post_branch_exceptions` — four sites had encoded the rule separately.

**Normalization stays bounded.** `drop_absent_optional` was elided an enumerated key *name*
at any depth, including inside `customizations` — arbitrary user data. The key list was
measured; the location was not. Anchored to its declared `field:/configuration` scope
(`NORMALIZER_VERSION` 5).

## What measurement changed

Three divergences surfaced. Reasoning would have mis-classified two of them:

1. **Container labels** — deacon sets five the reference never does. A deacon *extension*
   (`ext-`), not a disagreement (`wvr-`). Also shrank the tolerance from four namespace
   wildcards to five named keys; three of those wildcards matched nothing.
2. **Keep-alive `cmd`** — recorded in the codebase as having "no observable behavioral
   difference … intentional, characterized divergence (#290)". **False.** deacon's foreground
   `sleep` could not service SIGTERM, so `docker stop` took **10,258 ms** and SIGKILLed
   against the reference's 215 ms. Fixed on both paths (245 ms / 138 ms, exit 0). It is
   *now* an intentional divergence — recorded as one only because the equivalence was
   measured, and the record says so, so the next reader sees what changed: not the field,
   the evidence.
3. **`devcontainer.metadata`** — two separate defects. deacon omitted the label entirely
   where the reference stamps `[]` (fixed); and deacon substitutes `${localWorkspaceFolder}`
   before stamping where the reference stores the template (T115, open).

The general lesson, worth stating plainly: *"captured but not compared, because it cannot
matter"* is itself a claim about behavior, and an uncompared field is exactly where such a
claim never gets tested. The legacy `diff_states` captured `cmd` and skipped it for years.

## Explicitly not built

| Refused | Why |
|---|---|
| Outcome-conditional / `anyOf` assertions | A boolean language with an implicit discriminator. Dissolved by `require_buildkit()` |
| Quantified map predicates (`∃`/`∄`) | Quantifiers over maps are a query language; once `∃` lands, `∀`, negation and composition follow. Replaced by a derived observer field |
| Relational assertions between two fields of one document | Cross-field reference is where assertions become expressions. Hard line |
| Cross-side inequality | Inverts the oracle's meaning; needs a shared workspace, the opposite of the isolation invariant |
| A third "honest but hand-written" case shape | Re-legitimises hand-written coverage the moment SC-001 depends on data being the only first-class shape |

## Blocking work

**T115 blocks deleting `parity_state_diff`**, and correctly so. The declarative replacements
compare more than the legacy carrier did, so they are *stricter* — and `equivalence-report`
refuses a `stricter` verdict carrying no `characterizedAs`, because unproven-stricter is
indistinguishable from a newly introduced bug. Fixing or characterizing T115 is the
precondition for finishing step 5.
