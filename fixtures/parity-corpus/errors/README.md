# Error corpus — Tier 1c differential

Invalid / edge-case `devcontainer.json` inputs, diffed for **error-decision
parity**: do deacon and the reference CLI (`@devcontainers/cli` v0.87.0) *agree
on whether the input is an error?* The valid-config tiers diff successful
output; this tier diffs the accept/reject decision (and, when both accept, the
resolved value after pruning).

Run it:

```bash
python3 fixtures/parity-corpus/run_tier1_errors.py [deacon_bin] [corpus_dir]
```

Exit 0 when every fixture matches its encoded `expect`, else 1 (CI-gateable).
Each fixture is `errors/<name>/expect.json` + (usually) a `.devcontainer/`.

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
i.e. it never followed the `extends` field at all. That is a deeper divergence
worth its own investigation (is deacon's `extends` an extension beyond what the
reference implements?) and is tracked separately, not by this tier.

## Why these are encoded as PASS, not bugs

deacon's strictness follows its constitution (*fail fast, no silent fallbacks,
filter invalid inputs at ingress*). Rejecting malformed JSON and detecting
`extends` cycles up front is defensible and arguably better than the reference's
leniency. So the divergences are **characterized** with `expect:
"deacon-stricter"`: the corpus stays green while that exact pattern holds and
goes red only if EITHER CLI's behavior *changes* (e.g. a deacon refactor makes
read-config lenient, or a reference upgrade makes it strict). True agreement
cases (`both-reject`, `both-accept`) guard the other direction.

## `expect` vocabulary

- `both-reject` — both CLIs must reject (exit != 0). True error-parity agreement.
- `both-accept` — both accept **and** emit the same resolved config after pruning.
- `deacon-stricter` — deacon rejects, reference leniently accepts (characterized).

## Adding a fixture

1. `errors/<name>/.devcontainer/devcontainer.json` (or supporting files; omit
   it entirely for a "no config" case).
2. `errors/<name>/expect.json` with `description` + `expect` (+ optional
   `config` for an explicit `--config`, `signal` for stderr substrings).
3. Run the driver; if it flags a DIVERGENCE, triage whether it's a deacon bug or
   a defensible characterized divergence, and set `expect` accordingly.

## Natural next step

This tier compares at `read-configuration` (Docker-free). A Tier-2c
**up/build error tier** would compare where the reference *does* finally
validate — surfacing whether deacon and the reference agree on runtime-stage
rejections (missing image, unresolvable feature, conflicting mounts). Not yet
built.
