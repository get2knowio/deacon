# Quickstart: Deterministic Conformance Coverage

Practical workflows for the coverage machinery. All commands are **dev-only** — none is part
of the shipped `deacon` CLI.

## The five-second model

```text
scenario.json ─┐
applicability ─┼─► coverage generate ─► obligations.json ─┐
behaviors.json ┘        (machine)                          ├─► validate  (V26–V29, every PR)
                                                           ├─► certify   (release gate)
obligation-dispositions/ ─────────────────────────────────┘└─► coverage report (4 families,
        (hand-authored)                                                          8 artifacts)
```

**Machine-owned** files are regenerated and byte-compared; **hand-authored** files carry
judgement and are never written by a tool. Generation never touches judgement.

---

## Add a deterministic case

The common task. It is a **pure data edit** — no new Rust function (SC-013).

1. **Write the fixture** under `conformance/fixtures/fx-<name>/`. Pin every image to a digest
   or concrete tag (V18 rejects `latest`).
2. **Add the case** to `conformance/registry/cases/<area>.json`, assigning **every** scenario
   dimension:

   ```jsonc
   {
     "id": "case-up-compose-lockfile",
     "behaviors": ["bhv-up-compose-project-resources"],
     "context": [],
     "scenarioContext": {
       "sdim-operation": "up",
       "sdim-config-source": "compose",
       "sdim-container-state": "none",
       "sdim-features": "lockfile",
       "sdim-layering": "single",
       "sdim-output-mode": "structured"
     },
     "oracleType": "live-differential",
     "operations": [ { "id": "op-up", "subcommand": "up",
                       "argv": ["--workspace-folder", "${WORKSPACE}"],
                       "fixtures": ["fx-up-compose-lockfile"] } ],
     "expected": [ { "channel": "chan-process-graph", "operation": "op-up" } ],
     "resourceGroup": "docker-shared",
     "cleanup": { "tempdir": true }
   }
   ```

3. **Verify and see what it now covers**:

   ```bash
   cargo run -p deacon-conformance -- validate
   cargo run -p deacon-conformance -- coverage report
   $PAGER target/conformance/coverage-pairwise.md      # which pairs the case now satisfies
   $PAGER target/conformance/coverage-observables.md   # which channels it now covers
   ```

4. **Flip every `odp-cmb-*` the case now covers from `gap` to `case`**, in
   `registry/obligation-dispositions/<area>.json`, **in the same commit**. Add the
   `odp-bhv-*` entry for any new behavior too.

**Step 4 is the one that gets skipped, and skipping it is silent.** An explicit disposition
takes precedence over the evidence — deliberately, so a reviewer can rule that a mechanical
`scenarioContext` match is not real coverage — so a case that covers a pair whose record
still says `gap` changes nothing anyone can see: `validate` passes, `certify` blocks on a
gap that is genuinely gap-shaped, and the report under-counts. It happened during this
feature's own build-out: 22 records across three areas, found by hand. See
`conformance/RULES.md`, "Drift workflow (adding a case)".

A partial `scenarioContext` is rejected (V26). Assigning every dimension is what lets one case
cover `C(n,2)` pairs at once — which is why the pair space is fillable at all.

**Assertions must be able to fail.** Before committing, perturb each new assertion and
confirm the case DIVERGES. `jsonSubset: {}` matches any value and `contains` cannot see
appended output; both shipped in committed cases and were found only by the injected run.
And no channel may rest on fewer than three covering cases (SC-005,
`summary.channelsBelowFloor`) — a channel carried by one case is one authoring mistake from
being unobserved.

---

## Find what is missing

```bash
cargo run -p deacon-conformance -- coverage report
$PAGER target/conformance/coverage-pairwise.md
```

Read in this order:

| Look at | Question it answers |
|---|---|
| `summary.undispositioned` | What must be zero before release (SC-001) |
| `bucket: "gap"` rows | What is admitted missing |
| `excluded` | What is impossible — and by which rule |
| `deadValues` | Which declared values no longer occur anywhere |

If a combination is missing from **all** of these, the model is wrong, not the coverage — a
combination must be valid-and-bucketed or excluded-with-a-rule. There is no third state.

---

## Disposition the queue

```bash
cargo run -p deacon-conformance -- coverage scaffold > /tmp/skeletons.json
```

Skeletons print to **stdout** with `"UNREVIEWED"` sentinels the loader rejects, so a scaffold
committed unedited fails rather than silently passing. Move each into the appropriate
`obligation-dispositions/<area>.json` and replace the sentinel with a real disposition:

| Situation | Disposition |
|---|---|
| You wrote a case | `case` |
| It cannot be observed, and you can name why | `non-testable` + `rationale` naming a **ground** |
| A real, characterized divergence you accept | `waived` + a scoped `wvr-` with `expires` |
| You have not done the work | `gap` — honest, and it blocks release |

`"out of scope"` alone is rejected (V29). Name the principle (*"Constitution II forbids feature
authoring"*) or the mechanism (*"no reference side, so the three-axis disposition has nothing
to record"*).

A **high-risk triple** accepts only `case` or `gap`. Rationale and waiver are rejected there
by design — triples are selected precisely because interaction defects hide in them, so an
argument cannot stand in for evidence.

---

## Add a scenario dimension or applicability rule

1. Edit `registry/scenario.json` or `registry/applicability.json` (every rule needs a
   `ground`).
2. Regenerate and inspect the delta:

   ```bash
   cargo run -p deacon-conformance -- coverage generate
   git diff --stat conformance/obligations/obligations.json
   ```

3. `validate` — V26 reports dead values, V28 enumerates the new undispositioned queue.
4. Disposition until `certify` unblocks.

**Disposition is never inherited by name.** A regenerated obligation resembling a removed one
is a *new* obligation needing its own decision. A name is not evidence.

---

## Run the live tiers

Needs Docker and the pinned oracle. Never runs in the fast or CI lanes — a green fast run does
not imply live coverage, by design.

```bash
cargo nextest run --profile parity                      # every live binary, not just these two
cargo nextest run --profile parity -E 'binary(=parity_conformance_docker)'   # Docker tier only
cargo nextest run --profile parity -E 'binary(=parity_conformance_runner)'   # config-only tier
```

`--profile parity` selects the whole live allow-list — the two declarative drivers **and**
the four surviving legacy carriers (`parity_build`, `parity_exec`, `parity_observable_state`,
`parity_state_diff`). Filter by binary when you only want the tier you are working on.

A missing oracle, a version mismatch, or an absent Docker daemon **fails loudly** with a
cause-specific error. There is no skip. If you see a pass, it ran.

Budget: the Docker tier asserts a 30-minute wall clock and a 5-minute per-case timeout.
Exceeding either is a failure of acceptance, not a reason to widen the budget — tighten the
applicability rules or split the tier instead.

---

## Prove the channels are live

```bash
cargo run -p parity-harness --bin coverage-regressions
$PAGER target/conformance/regressions.json
```

`inertCount` must be **zero**. An inert channel means no case's comparison actually depends on
that channel — the suite is green there because nothing is looking, which is the failure this
run exists to catch.

Regressions are applied to the **evidence source** (a process result, an inspect document,
file bytes) and reverted on success and on unwind. Perturbing an observer's *return* value is
forbidden: a dead observer would then look live.

---

## Before you push

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
make test-nextest
cargo run -p deacon-conformance -- validate
cargo run -p deacon-conformance -- certify
```

`-p deacon` alone misses fmt drift in new test files and lints in `deacon-core`, which then
fail CI's `Lint (fmt + clippy)` job.

---

## Adding a live test binary

Three places, or `parity_registry_check` fails:

1. `fixtures/parity-corpus/registry.json` — the binary, its `kind`, and its **true**
   `docker_required`.
2. `.config/nextest.toml` — the `[profile.parity]` `default-filter` **and** the exclusion in
   every other profile.
3. A `crates/*/tests/<name>.rs` that actually exists.

No `#[ignore]`, no env-var opt-in, no silent skip. The harness's whole value is truthful
non-selection.

---

## Troubleshooting

| Symptom | Cause |
|---|---|
| `V27: obligations drift` | Committed file ≠ regeneration. Run `coverage generate` and commit |
| `V28: undispositioned obligation` | New obligation from a model edit. `coverage scaffold` |
| `V29: filler rationale` | `"out of scope"` without a ground. Name the principle or mechanism |
| `V26: dead value` | A rule edit stranded a value. Remove it, or relax the rule |
| Case covers no pairs | Partial or missing `scenarioContext` |
| Pair still `gap` after adding a case | Its `odp-cmb-*` still says `gap`; explicit records outrank the evidence. Flip it |
| `channelsBelowFloor` > 0 | A channel rests on fewer than three covering cases (SC-005). Add cases; do not lower the floor |
| Channel reported `inert` | No case's comparison depends on it — add one; do not delete the regression |
| Case reports `observed nothing` | Not an assertion failure: the operation failed, or the container was never inspected. Read the raw output under `target/parity/raw/` |
| Docker tier over budget | Tighten applicability rules or split the tier. Do not widen the budget |
