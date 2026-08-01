# Error-decision contract (formerly the Tier 1c corpus)

Invalid / edge-case `devcontainer.json` inputs, diffed for **error-decision
parity**: do deacon and the reference CLI (`@devcontainers/cli` v0.87.0) *agree
on whether the input is an error?* The valid-config tiers diff successful
output; this tier diffs the accept/reject decision (and, when both accept, the
resolved value after pruning).

**This directory no longer holds cases or a runner.** The Python
`run_tier1_errors.py` driver was ported to Rust in 018-harden-parity-harness; that Rust
runner and the nine `errors/<name>/` case directories were in turn **deleted** in
023-migrate-parity-to-conformance (US7), once the equivalence ledger proved the
replacements lose nothing. The file you are reading survives because the *contract* below
outlived both implementations of it — the findings, the two deliberate refinements, and
the `extends` reasoning are the durable part.

What replaced them:

| Was | Is now |
|---|---|
| `crates/deacon/tests/parity_corpus_errors.rs` (deleted) | the shared `parity_conformance_runner` |
| nine `errors/<name>/` case directories (deleted) | eleven `case-errors-decl-*` records in `conformance/registry/cases.json` |
| the corpus's fixture trees (deleted) | `conformance/fixtures/fx-errors-*/` |

Nine units became eleven cases: two rejections needed a second `spec-expectation` twin to
pin the DIRECTION of the difference, which a differential where both sides reject cannot
express on its own.

```bash
make test-parity            # cargo nextest run --profile parity, then the aggregator
```

Each case's accept/reject expectation lives in the **conformance registry** as a
`bhv-readconfig-<name>` behavior with its three-axis disposition — as of
019-conformance-registry, not a per-case `expect.json`. Where the two CLIs genuinely
differ, a `corpus_case`-scoped `wvr-<name>` waiver characterizes the difference and
fails as *stale* once it stops reproducing (FR-011). Where they **agree**, there is no
waiver: six of the nine migrated records were retired on 2026-08-01 because a
`both-accept` / `both-reject` expectation records agreement, and agreement is asserted
by the case, not tolerated by a waiver.

## Headline finding

deacon's `read-configuration` validates **eagerly and strictly**; the
reference's is a **lenient parse-and-echo**. Concretely, at `read-configuration`:

| input                       | deacon            | reference                              |
|-----------------------------|-------------------|----------------------------------------|
| malformed JSONC             | **reject** (parse error) | accept — recovering parser drops the broken key |
| `extends` → missing file    | **reject** (resolves eagerly) | accept — `extends` echoed literally, not resolved |
| `extends` → cycle           | **reject** (loop detected) | accept — not resolved                  |
| `forwardPorts: "3000"`      | **reject** (typed deser) | accept — raw JSON kept                  |
| `features: "<string>"`      | **reject** (type-strict, see note) | accept — raw JSON kept     |
| duplicate key (last-wins)   | accept            | accept (same value)                    |
| unknown / future top-level field | accept — **preserved** (see note) | accept — preserved          |
| no config / bad `--config`  | **reject**        | **reject**                              |

### Two deliberate refinements (not just characterization)

deacon's strictness is meant to be a *consistent* policy, applied per our
design discussion:

- **Type-strict on modeled object fields.** `features` and `customizations` are
  spec-shaped as `map<string, …>`. deacon now rejects a non-object value for
  them, matching the typed strictness `forwardPorts` already had. Previously
  `features` was accepted untyped — an inconsistency (forwardPorts strict,
  features lenient). Fixed so deacon fails fast and *predictably* on a clear
  authoring mistake. (`wrong-type-features` → `deacon-stricter`.)
- **Preserve, never drop, unmodeled fields.** Unknown / future top-level fields
  are passed through verbatim (the spec's extensibility model assumes tools
  tolerate fields they don't understand). Previously deacon silently *dropped*
  them — a fidelity loss versus the reference. Now both accept and both
  preserve. (`unknown-field-preserved` → `both-accept`, value compared.)

The guiding principle: **fail fast and precisely where the developer made a
mistake; preserve silently where deacon simply does not model the field.**

The reference does **not resolve `extends` even at `build` time** — it errors
with "No image information specified" rather than on the missing/cyclic target,
i.e. it never followed the `extends` field at all.

**Resolved (issue #297):** yes — deacon's `extends` is a deliberate capability
*ahead of* the reference, not accidental drift. `extends` is the in-flight spec
proposal [devcontainers/spec#22], which the reference CLI (v0.87.0) does not
implement: `read-configuration` echoes the field literally, and `up`/`build`
fail to find an image because they never follow it. deacon resolves the full
chain eagerly across `up`, `build`, `read-configuration`, `outdated`, `set-up`,
and `upgrade` (see the field docs on `DevContainerConfig::extends` and
`docs/DIFFERENTIATORS.md`). The consequences are therefore **intentional,
characterized divergences**, not parity bugs:

- `extends` → missing / cyclic target: `deacon-stricter` (deacon resolves
  eagerly and rejects; reference never resolves). Recorded as behaviors
  `bhv-readconfig-extends-missing-rejected` / `bhv-readconfig-extends-cycle-rejected`
  linked from the `ext-extends-resolution` extension in the conformance registry, which
  is their single characterization (the duplicate `wvr-extends-*` waivers were retired
  2026-08-01).
- `extends` → valid target (conformance case 44): both succeed, but deacon
  merges the base config (e.g. the base `containerEnv` appears in the resolved
  config / created container) while the reference drops it. This is a
  deacon-only superset, expected by design.

deacon does **not** claim reference parity for configs that use `extends`; it
claims to do strictly *more*.

[devcontainers/spec#22]: https://github.com/devcontainers/spec/issues/22

## Why these are encoded as PASS, not bugs

deacon's strictness follows its constitution (*fail fast, no silent fallbacks,
filter invalid inputs at ingress*). Rejecting malformed JSON and detecting
`extends` cycles up front is defensible and arguably better than the reference's
leniency. So the divergences are **characterized** with `expect:
"deacon-stricter"`: the corpus stays green while that exact pattern holds and
goes red only if EITHER CLI's behavior *changes* (e.g. a deacon refactor makes
read-config lenient, or a reference upgrade makes it strict). True agreement
cases (`both-reject`, `both-accept`) guard the other direction.

## Waiver records

A case whose two sides genuinely differ carries a `wvr-<name>` record in the
**conformance registry** (`conformance/registry/waivers/wvr-<name>.json`), loaded
by `parity_harness::waiver` through `deacon-conformance` — the single waiver schema
now lives there (`crates/conformance/src/model.rs`; contract
`specs/019-conformance-registry/contracts/registry-schema.md`). Unknown fields are
rejected; `id` is globally unique; `rationale` is non-empty; `expires` is
mandatory. Each record links a `bhv-readconfig-<name>` behavior
(`conformance/registry/behaviors/read-configuration.json`) that carries the
three-axis disposition (spec / reference / decision).

`expect.kind` vocabulary (unchanged from the migrated schema):

- `both-reject` — both CLIs must reject (exit != 0). True error-parity agreement.
- `both-accept` — both accept **and** emit the same resolved config after pruning.
- `deacon-stricter` — deacon rejects, reference leniently accepts (characterized).
  Carries an optional `"signal": ["substr", …]` of informational stderr
  substrings (not part of the pass/fail decision).

Three records survive — `wvr-malformed-json`, `wvr-wrong-type-features`, and
`wvr-wrong-type-forwardports` — all `deacon-stricter`. The six that carried a
`both-accept` / `both-reject` expectation (`wvr-bad-config-path`, `wvr-duplicate-keys`,
`wvr-extends-cycle`, `wvr-extends-missing`, `wvr-missing-config`,
`wvr-unknown-field-preserved`) were retired on 2026-08-01: the first four recorded
agreement rather than divergence, and the two `extends` records duplicated
`ext-extends-resolution`. Each retirement is dispositioned in
`conformance/migration/mapping.json`, so the pre-migration inventory is still auditable.
`config` (optional, string) carries an explicit `--config` argument for a case and plays
no part in waiver semantics.

## Adding a case

Since 023 this is a **pure data edit — no new Rust function** (SC-001). Nothing is added
under this directory any more.

1. `conformance/fixtures/fx-errors-<name>/.devcontainer/devcontainer.json` (or supporting
   files; a deliberately "no config" case just omits the `.devcontainer/`).
2. A `bhv-readconfig-<name>` behavior in the conformance registry — plus a `wvr-<name>`
   waiver ONLY if the two CLIs genuinely differ and the difference is one we accept
   (see `conformance/RULES.md` and
   `specs/019-conformance-registry/quickstart.md` for the record-a-divergence recipe).
3. A `case-errors-decl-<name>` record in `conformance/registry/cases.json` with
   `operations` + `oracleType` + `expected`. Use `live-differential` when the interesting
   fact is *whether the two CLIs agree*; add a `-decision` twin with
   `oracleType: "spec-expectation"` when both sides reject and the interesting fact is
   *which* rejection deacon must produce — a both-reject differential agrees no matter
   what deacon says, so the direction has to be pinned separately.
4. `cargo run -p deacon-conformance -- validate` (hermetic; V16 checks the case shape),
   then `make test-parity` for the live comparison. If it flags a DIVERGENCE, triage
   whether it is a deacon bug or a defensible characterized divergence, and set the
   behavior's dispositions + the waiver's `expect.kind` accordingly.

## Natural next step

This tier compares at `read-configuration` (Docker-free). A Tier-2c
**up/build error tier** would compare where the reference *does* finally
validate — surfacing whether deacon and the reference agree on runtime-stage
rejections (missing image, unresolvable feature, conflicting mounts). Not yet
built.
