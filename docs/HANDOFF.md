# Handoff — waiver adjudication and the fixes it produced

**Written 2026-08-02.** Working state, not project documentation: delete it when the queue
is empty. Durable rules live in `conformance/RULES.md` and in
`crates/conformance/src/parity_page.rs`'s module doc.

## The standing rule

> "I want almost no waivers — a waiver should be when you and I have discussed the item
> and I've agreed that a waiver makes sense. And there will be very few cases where I will
> think a waiver makes sense."

A `wvr-` record is never authored on the harness's say-so, and never retired on it either.
Bring each one up individually; do not batch rulings.

## State

`main` at `192db7d`. **14 waivers. `certify`: 49 blocking / 14 waived.** The 49 breaks down
as 7 gaps — 6 `gap-pairwise-*` (as 024 left them) plus the new
`gap-features-duplicate-in-one-document`, which blocks until #430 lands — and 42
execution-evidence conditions (41 `unknown-runner-omission`, 1
`missing-required-execution`). The latter are the normal state anywhere but the release
lane: they mean the container-backed lane has not run on this revision, not that anything
is broken.

Every waiver whose behavior reads `spec: conformant` is settled — those are `follow-spec`
now, meaning deacon follows the spec and the CLI deviates. **The open queue is exactly the
seven `spec: unspecified` + `intentional-divergence` records**: three ruled *keep*, two
ruled *fix deacon* with the work not started, two not yet ruled.

Regenerate the table any time — it is derived, never hand-maintained:

```bash
jq -s '[.[].records[]]' conformance/registry/behaviors/*.json > /tmp/b.json
jq -s '[.[].records[]]' conformance/registry/cases/*.json     > /tmp/c.json
jq -s '.'               conformance/registry/waivers/*.json   > /tmp/w.json
jq -rn --slurpfile b /tmp/b.json --slurpfile c /tmp/c.json --slurpfile w /tmp/w.json '
 ($b[0]|INDEX(.id)) as $B
| ([$c[0][]|.allowedDifferences[]?|.waiverId]|map(select(.))|unique) as $cons
| $w[0][] | .id as $wid | (.behaviors[0]) as $bid | ($B[$bid]) as $bh
| ([$c[0][]|select((.behaviors//[])|index($bid))]|length) as $n
| [($wid|sub("^wvr-";"")), $bh.spec, $bh.decision,
   (if ($cons|index($wid)) then "live" else "INERT" end), ($n|tostring)]|@tsv' \
| sort -t$'\t' -k3,3 -k2,2 | column -t -s$'\t'
```

`jq` gotcha that has cost time twice: `$cons | index(.id)` pipes the **array** into `.id`.
Bind the field first (`.id as $wid | … index($wid)`).

## The method — apply it to every remaining waiver

**Spec-check the `spec` axis before forming a view.** That is where the error hides. Four
Group C waivers claimed `spec: unspecified`; one was simply wrong, and no amount of reading
its rationale would have shown it — only opening the pinned prose did. Then **measure both
CLIs**; do not reason from the recorded rationale.

```bash
S=<scratch>; npm install --no-save --prefix $S @devcontainers/cli@0.87.0
$S/node_modules/.bin/devcontainer read-configuration --workspace-folder <fixture>
$S/node_modules/.bin/devcontainer features resolve-dependencies --workspace-folder <fixture>
```

Docker and the pinned oracle both work in this dev container. `features
resolve-dependencies` is the highest-value probe for anything Feature-related — it prints
the reference's own resolved install order, which is what settled #430.

## The queue

### A. The seven `unspecified` + `intentional-divergence` waivers

R5 forbids `follow-spec` when the spec is silent and R6 forbids `align-with-reference` when
the reference differs, so removing any one leaves `unresolved-gap` as the only legal
disposition — **each removal is a decision to block releases, not a cleanup.**

| Waiver | Status | Enforced? | Cases |
|---|---|---|---|
| `wvr-malformed-json` | ruled **KEEP** 2026-08-02 | live | 7 |
| `wvr-outdated-malformed-lockfile-rejected` | ruled **KEEP** 2026-08-02 | **INERT** | 1 |
| `wvr-up-changed-config-recreates` | ruled **KEEP** 2026-08-02 | **INERT** | 1 |
| `wvr-readconfig-authored-empty-omitted` | ruled **FIX DEACON** — not started (#398) | live | 2 |
| `wvr-container-metadata-label-serialization` | ruled **FIX DEACON** — not started (#394) | live | 2 |
| `wvr-compose-project-file-set` | **unruled** | live | 1 |
| `wvr-readconfig-merged-computed-empties` | **unruled** | **INERT** | **0** |

**The two ruled "fix deacon, drop the waiver"** admit in their own rationales that deacon is
wrong, not different — that is why they were ruled that way:

- `wvr-readconfig-authored-empty-omitted` — *"a fidelity DEFECT, not a design choice,
  tracked at #398"*. What remains after #398's first pass is one schema-invalid shape: an
  authored `"name": null`, which deacon holds in the same `None` as an omission.
- `wvr-container-metadata-label-serialization` — the key-order half: *"a deacon defect, not
  an accepted difference… filed as `parity-drift` #394"*. Its whitespace half is a genuine
  accept (the reference emits `[ {…}, {…} ]` only when the value arrives via an image
  build; matching that means mimicking its build routing), so this waiver **narrows** when
  #394 lands — it does not disappear.

**The two unruled ones**, with what I found before running out of session:

- `wvr-compose-project-file-set` — deacon streams the generated override to Compose's stdin
  (recorded as `-` in `config_files`); the reference writes a temp file and records its
  path. `config-hash` differs as a consequence. Neither mechanism is spec-defined and
  nothing else in the container differs — service, network, volume, mounts and environment
  all compare equal. Reads like a legitimate accept, but has not been spec-checked.
- `wvr-readconfig-merged-computed-empties` — **zero cases**, so it is the sole coverage for
  `bhv-readconfig-merged-computed-empties-omitted`. Authoring its case does double duty:
  first coverage for that behavior AND makes the waiver live. Its content argument is
  strong — the reference emits `hostRequirements: {memory: "-Infinity"}`, which is
  `Math.max()` over an empty set serialized, and reproducing that is adopting a quirk
  rather than achieving parity.

### B. #430 — duplicate Features (gap is open, blocks certification)

deacon rejects a `features` map whose keys resolve to one canonical Feature id.
`feature-dependencies.md` defines both entries as distinct Features (§Feature Equality),
supplies an oldest-tag-first tie-break for exactly this collision (§Round Stable Sort), and
says a single Feature may be installed more than once (§Feature authorship). Oracle 0.87.0
returns both digests in `installOrder`.

**The one-line parse fix must NOT land alone.** Deleting the rejection in
`deserialize_features_value` (`crates/core/src/config.rs`) makes `read-configuration` match
byte-for-byte and then **silently drops** the second Feature — verified with a real `up`:
only `git:1.3.1` fetched, one staged directory, one feature RUN stage, exit 0. Feature
identity is version-independent (`feature_resolver.rs:74` sets `id` from the Feature
metadata id; `features_build.rs:518-523` keys three maps on it).

Suggested approach: key staging and dependency-graph nodes by the **user-provided form**,
which is unique by construction — the lockfile already does this, with a comment saying it
must match upstream `generateLockfile`. Keep `FeatureIdResolver` translating aliases so
`dependsOn` / `installsAfter` / `overrideFeatureInstallOrder` still match.

Not yet measured: whether `bhv-extends-feature-version-override` (#411, child's version
wins across `extends`) is also non-spec. `extends` is a deacon extension with no reference
equivalent, so it needs its own argument.

### C. #394 is roughly 20 sites, not 366 across 52 files

The waiver's *"~366 references across 52 files"* is its stated reason for deferring, and it
is wrong by an order of magnitude. Measured by changing the two field types and letting the
compiler find the breaks:

- `indexmap` is **already** a `deacon-core` dependency
- `deacon-core` broke in **9 places, all in `config.rs`** — the two `LazyLock` empty-map
  statics, two substitution builders, `merge_string_maps`, `merge_optional_string_maps`,
  three call sites
- the rest of the workspace broke in **11 more**, in `plugins.rs`, `exec.rs`,
  `run_user_commands.rs`, `set_up.rs`, `up/container.rs`

Most of the 415 raw `container_env`/`remote_env` references are reads that behave
identically on `IndexMap`.

**Compiling is not fixing.** For the label bytes to come out in authored order, `IndexMap`
must *propagate* to the write site rather than be converted back at a boundary — and the
consumers are shared structs every subcommand uses: `shared/env_user.rs`,
`container_lifecycle.rs`, `container_env_probe.rs`, `host_ca/env.rs`. `features.rs:332`
also has a `container_env`, but that is Feature-contributed env and is a **different**
thing. Verify with a real `up` and `docker inspect` of `devcontainer.metadata`, not a green
build.

### D. Older items, unstarted

- **#423** — the inert waivers are enforced by nothing. Each needs a case tolerating it via
  `allowedDifferences`, which is also what makes it self-invalidating again. Currently
  inert: `outdated-extends-chain-features`, `outdated-malformed-lockfile-rejected`,
  `readconfig-merged-computed-empties`, `up-changed-config-recreates`.
- **#370** — `bhv-exec-restored-path-ordering`: exec drops the image ENV `PATH`.
  Root-caused; needs a precedence decision.
- **#371** — `up` leaves the superseded container RUNNING when a changed config forces a
  new one, so one workspace ends up with two live containers. Deliberately *not* waived:
  the recreate is the decision, the leak is a bug.

## Two fixture findings, neither filed

1. **A stray untracked file makes `fixtureHash` differ between this workspace and CI.**
   `conformance/fixtures/fx-upgrade-overlay-lockfile/.devcontainer/devcontainer-lock.json`
   was never committed — the only `*-lockfile*` fixture missing one, though `.gitignore:65`
   explicitly un-ignores that filename. `fixture_hash_dir` walks the working tree, so a
   recording made against this fixture reads fresh locally and stale in CI. No live
   impact today (the `snapshot` oracle and its one committed case were removed; nothing uses
   it). Left in place rather than deleted — deciding it needs the second finding resolved.

2. **`case-readconfig-lockfile-present` advertises evidence it cannot produce.** Its note
   opens *"A workspace whose Features are already pinned by a committed
   `devcontainer-lock.json`"*, but it is not committed; `read-configuration` never reads a
   lockfile (only `lockfile.rs`, `upgrade.rs`, `up/helpers.rs` do); and even present, the
   stray pins the *same* Feature at the *same* version, so it cannot discriminate. The
   sibling `case-upgrade-regenerates-lockfile` note says the opposite — *"the fixture
   contains no lockfile at all"* — and is the accurate one. A discriminating fixture would
   pin a *different* version and assert the read reports the declared one.

## Worth not re-deriving

- **The recurring defect class is rendering a claim where a reader reads evidence.** Six
  instances, each its own PR: the headline counted dispositions while the grid counted
  evidence (#422); a waiver id proved a record was written, not that anything re-checks it
  (#424); `❌` blamed deacon for the reference's deviation (#426); four records called the
  CLI's schema deviation our deliberate choice (#429); one waiver asserted a conclusion with
  no measurement and another cited the silence that helped it but not the passage that did
  not (#432). Look for this shape first in anything summarising the registry.
- **A waiver's premise can be false even when its measurement is right.**
  `wvr-features-duplicate-in-one-document` correctly measured that the reference accepts the
  document, then reasoned the collision "has no coherent meaning" and that "nothing breaks
  the tie". The spec breaks it in three passages. The measurement was never the weak part.
- **Removing a validation can be worse than leaving it.** Check what the rest of the
  pipeline does with input the validation used to stop — a loud rejection replaced by a
  silent drop is a regression, not a parity fix.
- **Nothing verifies that a waiver is reachable.** `validate` checks structure, `certify`
  checks expiry, but no gate asks whether any runner reaches it (#423). #431 made one record
  worse this way: `reference: divergent` on the duplicate-Features behavior is now backed by
  a `spec-expectation` case that never runs the reference. Adding a live-differential case
  belongs with the #430 fix, where it becomes an agreement case rather than a permanent
  unwaived divergence in the nightly.
- **A test asserting over the whole rendered page is vacuous when the legend contains the
  token being tested.** Hit twice. Scope assertions to `rows_of(&render(&reg))`.
- **Verify every new guard fails without its fix, and every narrowed guard still fails on
  everything else.** `gap_certification`'s gap allow-list was narrowed by exactly one id and
  probed with a bogus `gap-bogus-unjustified` to confirm it still trips.
- **The `live-certification` lane is always red and never gates.** Its diverging set is 11
  `case-merged-decl-*`. Before assuming a change is safe, diff the run's diverging case ids
  against a recent `main` nightly (`gh run list --workflow=parity.yml --branch=main`) —
  identical sets mean your change is neutral on it.
