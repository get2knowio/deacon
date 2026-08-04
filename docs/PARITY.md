# Parity: How deacon Knows It's Correct

deacon is a reimplementation. Someone else defined what it should do, and someone else
already built a tool that does it. That is the whole reason this repo verifies itself the
way it does.

**Read this if** you have seen "parity", "divergence" or "waiver" in a PR and were not sure
whether they mean the same thing. They don't.

---

## The instrument, in one sentence

**Run deacon and the pinned reference CLI over the same scenarios, normalize both outputs,
and diff them.** A difference is either a bug (an issue) or a documented choice (one
allowlist entry). Every wild bug becomes a scenario before it is fixed.

That is the whole design. It used to be much larger — a conformance crate plus discovery,
drift, canary, coverage and certification layers, six CI lanes, thirty-six validation
classes: roughly 90k lines verifying a 115k-line product, generating enough housekeeping to
fill most of its own commit history. All of it is gone. If you find yourself about to add a
validation class, an obligation model or a lane registry, that is the thing that was
deleted.

## Two authorities, and why both

**The spec.** [containers.dev](https://containers.dev) defines what a devcontainer tool
must do. It is pinned at commit `113500f4`, so "the spec" means one exact document rather
than a moving target.

**The reference.** [`@devcontainers/cli`](https://github.com/devcontainers/cli), pinned at
`0.87.0`. It matters independently of the spec, because users compare deacon against *it*,
not against a document.

The two disagree sometimes, and the direction of disagreement is the most important
distinction here — see below.

## Vocabulary

**Divergence** — an observable difference between deacon and the reference that has been
*characterized*: measured, explained, recorded. A divergence is a claim about behavior,
never a placeholder for "we haven't looked yet".

**Allowed difference (tolerance)** — a divergence a scenario may see without failing.
Declared on the case, scoped to one `(behavior, observablePath)` pair; there are no global
ignore lists. Each names a backing identity in `parity/ALLOWLIST.json`, where the rationale
lives — argued from measured output.

The split is deliberate: the case says WHERE the difference is tolerated, the allowlist
says WHY. Restating the scope in the allowlist would be a second copy that drifts, so the
loader computes the relationship instead and fails **both** ways — a tolerance naming an id
no record defines (unbacked, excusing a difference on the authority of nothing) and a
record no case references (an orphan, reading as characterized coverage nothing exercises).

**Tolerances are self-invalidating.** One whose difference *stops reproducing* fails the
run as **stale**. Not politeness — a stale tolerance is strictly worse than none, because
it keeps excusing a path where the difference is already fixed, and will silently excuse a
*new* difference that appears there later.

**A tolerance is consumed by the MOST SPECIFIC covering path, not the first match.**
Diverging paths join object keys with `.`, and Docker label keys contain dots, so
`labels.com.docker.compose.project` and `labels.com.docker.compose.project.config_files`
are two separate flat keys a prefix rule cannot tell apart as parent and child. First-match
let the shorter swallow the longer, and the longer then reported stale.

Which records are backed and which are orphans is no longer a question anyone answers by
hand — `Registry::load` fails on both. What is still worth querying ad hoc is the SHAPE of
the tolerance surface: how many observable paths each record excuses, across how many
cases. A record excusing many paths on many cases is doing more work than its one rationale
may cover.

```bash
jq -s '[.[].records[]]' parity/cases/*.json  > /tmp/c.json
jq       '.records'     parity/ALLOWLIST.json > /tmp/w.json
jq -rn --slurpfile c /tmp/c.json --slurpfile w /tmp/w.json '
  ([$c[0][] | .id as $case | (.allowedDifferences // [])[]
    | {id: (.waiverId // .divergenceId), case: $case}]) as $tol
| $w[0][] | .id as $wid
| [$wid, ([$tol[] | select(.id == $wid)] | length | tostring),
   ([$tol[] | select(.id == $wid) | .case] | unique | length | tostring)] | @tsv' \
| column -t   # record → tolerances → distinct cases
```

Two things that have cost time here. A tolerance names its backing id in **either**
`waiverId` **or** `divergenceId`, so a query reading only one silently under-counts. And
`$tol[] | select(.id == $wid)` must bind the id to a variable first: `$tol | index(.id)`
pipes the whole **array** into `.id` and quietly yields nothing.

**deacon is sometimes the conformant side.** When deacon follows the spec and the reference
deviates, that is the reference's deviation, not work we owe. Filing it as a deacon
divergence-to-fix is the most common way this record goes wrong, which is why
`parity/SPEC_STATUS.md` gives it its own status.

**Out of scope is recorded nowhere.** deacon implements the consumer surface only. Feature
authoring (`features test|info|plan|package|publish`) is not a divergence and not tracked —
it is a decision about product scope. See the constitution.

## Where things live

| Path | What it is |
|---|---|
| `parity/cases/<area>.json` | the scenarios — **data**, not code |
| `parity/fixtures/fx-*/` | one directory per fixture id, 1:1 with case references |
| `parity/ALLOWLIST.json` | every tolerated difference's identity and its reasoning |
| `parity/oracle.json` | the oracle pin, `include_str!`-embedded at compile time |
| `parity/spec/113500f4/` | the vendored upstream spec revision the scenarios are pinned to |
| `parity/SPEC_STATUS.md` | the hand-maintained answer to "does deacon behave like the CLI?" |
| `crates/parity-harness/` | the runner. Dev-only; not a dependency of the shipped binary |

A scenario is **ordered `operations[]`** (a consumer subcommand plus argv, with
`${WORKSPACE}`, `${IMAGE_TAG}` and `${CONTAINER_ID}` tokens), an `oracleType`, and
per-channel `expected[]` assertions. Adding one is a pure data edit — no new Rust.

`case.cleanup` is declared on every case and consumed by **nothing**: reclamation is the
`DockerWorkspace` RAII guard, unconditionally. Do not reach for a fixed global resource
name (a named volume, a fixed host port) on the assumption that field will clean it up.

## Three oracle types

| Type | Compares against |
|---|---|
| `spec-expectation` | the declared assertion. No reference run, so no oracle needed. |
| `live-differential` | the verified pinned oracle, run over the same operations. |
| `invariant-metamorphic` | a declared *relationship* across ≥2 operations (idempotence, first-create-vs-restart) rather than a fixed output. |

A `live-differential` may ALSO declare an assertion, which is evaluated against deacon's
side. Do that whenever the case knows what the output should *be*: a differential alone
cannot fail when both sides are equally wrong.

## Eleven observable channels

`chan-exit-code`, `chan-stdout`, `chan-stderr`, `chan-structured-output`,
`chan-filesystem`, `chan-file-content`, `chan-image`, `chan-process-graph`,
`chan-injected-process`, `chan-temporal`, `chan-container-state`.

Evidence is captured RAW, then normalized by the single `normalize.rs` using named,
field-specific rules — `path_token`, `image_tag_token`, `label_semantic`,
`mount_source_canonical`, `path_env_segmented`, `feature_staging_dir_token`,
`null_preserving`. **Nothing is blanket removed.** A rule that dropped a field wholesale
would hide the very differences it was written to make comparable.

Two channels capture things that *cannot* agree and deliberately omit them: a built image's
identity (id, digests, tags) and a Compose service's image digest when Features force a
rebuild. Both sides build separately, so those differ by construction and comparing them
would report a divergence every run while saying nothing about behavior.

## An assertion that cannot fail is worse than no assertion

Four have been committed here, and every one was found by *breaking it and watching what
happened*, never by reading:

1. `jsonSubset: {}` matches any value.
2. `contains` cannot see APPENDED output.
3. An `assertion` on a `live-differential` was loaded and never evaluated.
4. A stale tolerance was computed and then discarded by the driver.

The first two are guarded by `model.rs::is_vacuous_assertion` and a loader test; the last
two are now failures. **After writing any assertion, perturb it once and confirm it fails.**

## The lanes

Three lane binaries, split by what a case NEEDS rather than by what it is about, which is
what lets most of the suite gate every pull request. `driver::lane_of` derives a case's lane
from its `oracleType` and `resourceGroup`, so a lane never *skips* a case it cannot run — a
case it cannot run is in another lane. A skip and a pass look identical in a report; a
non-selection does not. The table of binaries and how to run them is in
[`CLAUDE.md`](../CLAUDE.md) § Differential Parity Suite.

**Selection is profile-based. There is no env-var opt-in and no silent skip.** Live parity
runs ONLY under `cargo nextest run --profile parity`; every other profile excludes it, so
those lanes are truthful by non-selection — a green fast run never *implies* parity ran. A
missing oracle, absent Docker, or a normalization failure FAILS with a cause-specific error
rather than skipping to green.

`.github/workflows/parity.yml` runs the profile nightly and **gates nothing** — it is not in
the release path, which runs the two no-oracle lanes instead. It runs `--no-fail-fast`
deliberately: its job is to enumerate a work queue, and cancel-on-first-failure once
truncated a nightly to 7 of 26 tests, hiding every Docker group behind one config-only
failure.

**Where the current state lives.** Behavior-by-behavior, `parity/SPEC_STATUS.md`; the
current verdict, the latest nightly run (`gh run list --workflow=parity.yml --branch=main`).
Neither belongs in this file — status snapshots in prose go stale within the day.

**What a failing run leaves behind.** Every lane writes its raw capture and report fragments
under `target/parity/`, and every job that runs a parity binary uploads that tree as a
`parity-evidence-*` artifact — the nightly on any outcome, the gating lanes (`ci.yml`'s fast,
MVP-integration and Podman jobs; `release.yml`'s verify job) on failure only. Download it
before re-running: a re-run that goes green destroys the only copy of what happened (#474).
The failure text itself carries the diverging observable paths, and — for a `chan-exit-code`
divergence specifically — a tail-bounded excerpt of the stderr of whichever side exited
non-zero, because that channel's verdict otherwise names nothing to fix.

## What to do when you find a difference

1. **Measure it.** Run both CLIs over the same fixture and record both values verbatim. A
   rationale arguing from reasoning rather than from output is not evidence.
2. **Decide which side is wrong.** deacon → file an issue, write the scenario, leave it
   red; do not waive it. The reference → record it as deacon following the spec. Spec
   silent and the difference deliberate → allowlist it, scoped to one observable path, with
   the measurement in the rationale.
3. **Update `parity/SPEC_STATUS.md` in the same commit.** It is hand-maintained; a row
   describing yesterday's behavior is worse than no row.

**A `wvr-` record is the maintainer's to grant, and the maintainer's to retire.** The
standing rule, in the maintainer's words:

> "I want almost no waivers — a waiver should be when you and I have discussed the item and
> I've agreed that a waiver makes sense. And there will be very few cases where I will think
> a waiver makes sense."

So: never author one on the harness's say-so, and never remove one on it either. Bring each
up individually; do not batch rulings. A ruling is recorded by folding an `ADJUDICATED
<date> and KEPT` sentence — with the argument, not just the verdict — into the record's own
`rationale`, so the reasoning travels with the thing it excuses.

## Verifying locally

Docker works in the dev container and the pinned oracle installs cleanly, so verify parity
changes for real rather than reasoning about them:

```bash
npm install -g @devcontainers/cli@0.87.0
cargo nextest run --profile parity
```

Docker-backed cases run in isolated temp workspaces behind an RAII cleanup guard, and a
cancelled run leaves orphans that trip the next run's resource guard. The reclamation recipe
is in [`CLAUDE.md`](../CLAUDE.md) § Differential Parity Suite.

## Further reading

- [`CLAUDE.md`](../CLAUDE.md) § Differential Parity Suite — how the lanes run, how to verify
  a change locally, and the rules that have cost time
- [`parity/SPEC_STATUS.md`](../parity/SPEC_STATUS.md) — the behavior-by-behavior record
- [`.specify/memory/constitution.md`](../.specify/memory/constitution.md) — the principles
  the strictness choices appeal to
- [`examples/CANARY_STATUS.md`](../examples/CANARY_STATUS.md) — canary state and protocol
