# Error corpus — Tier 1c differential

Invalid / edge-case `devcontainer.json` inputs, diffed for **error-decision
parity**: do deacon and the reference CLI (`@devcontainers/cli` v0.87.0) *agree
on whether the input is an error?* The valid-config tiers diff successful
output; this tier diffs the accept/reject decision (and, when both accept, the
resolved value after pruning).

Run it (the Python `run_tier1_errors.py` driver was ported to Rust and deleted —
018-harden-parity-harness):

```bash
make test-parity            # cargo nextest run --profile parity, then the aggregator
```

The runner is `crates/deacon/tests/parity_corpus_errors.rs`. It FAILS the run
naming the case when a fixture's decision no longer matches its recorded
expectation, and names any stale waiver record (FR-011). Each fixture is
`errors/<name>/expect.json` (a schema-validated waiver record) + (usually) a
`.devcontainer/`.

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
  eagerly and rejects; reference never resolves). Encoded in `extends-missing`
  and `extends-cycle`.
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

## Waiver schema

Each `errors/<name>/expect.json` is a full waiver record validated by the single
loader `parity_harness::waiver` (schema in
`specs/018-harden-parity-harness/contracts/registry-waiver-schema.md`,
`data-model.md` §4). Unknown fields are rejected; `id` is globally unique;
`rationale` is non-empty.

```json
{
  "id": "errors/<name>",
  "scope": { "kind": "corpus_case", "corpus": "errors", "case": "<name>" },
  "expect": { "kind": "both-reject" | "both-accept" | "deacon-stricter" },
  "config": "<rel/path>",
  "rationale": "why this divergence is characterized / why parity is expected",
  "added": "YYYY-MM-DD"
}
```

`expect.kind` vocabulary:

- `both-reject` — both CLIs must reject (exit != 0). True error-parity agreement.
- `both-accept` — both accept **and** emit the same resolved config after pruning.
- `deacon-stricter` — deacon rejects, reference leniently accepts (characterized).
  Carries an optional `"signal": ["substr", …]` of informational stderr
  substrings (not part of the pass/fail decision).

`config` (optional, string) is a schema-known field carrying an explicit
`--config` argument for the case; it plays no part in waiver semantics.

## Adding a fixture

1. `errors/<name>/.devcontainer/devcontainer.json` (or supporting files; omit
   it entirely for a "no config" case).
2. `errors/<name>/expect.json` as the full waiver record above.
3. Run `make test-parity`; if it flags a DIVERGENCE, triage whether it's a deacon
   bug or a defensible characterized divergence, and set `expect.kind`
   accordingly (`errors/` cases require `min_cases` ≥ 9 in `registry.json`).

## Natural next step

This tier compares at `read-configuration` (Docker-free). A Tier-2c
**up/build error tier** would compare where the reference *does* finally
validate — surfacing whether deacon and the reference agree on runtime-stage
rejections (missing image, unresolvable feature, conflicting mounts). Not yet
built.
