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

**Waiver / allowed difference** — a divergence a scenario may see without failing. Scoped
to one `(behavior, observablePath)` pair; there are no global ignore lists. Each carries a
rationale arguing from measured output.

**Waivers are self-invalidating.** A tolerance whose difference *stops reproducing* fails
the run as **stale**. Not politeness — a stale tolerance is strictly worse than none,
because it keeps excusing a path where the difference is already fixed, and will silently
excuse a *new* difference that appears there later.

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
| `conformance/registry/cases/<area>.json` | the scenarios — **data**, not code |
| `conformance/registry/waivers/*.json` | tolerated differences, each with a rationale and an expiry |
| `conformance/registry/behaviors/*.json` | the behavior record `SPEC_STATUS.md` is harvested from |
| `conformance/fixtures/fx-*/` | one directory per fixture id, 1:1 with case references |
| `fixtures/parity-corpus/oracle.json` | the oracle pin, `include_str!`-embedded at compile time |
| `parity/SPEC_STATUS.md` | the hand-maintained answer to "does deacon behave like the CLI?" |
| `crates/parity-harness/` | the runner. Dev-only; not a dependency of the shipped binary |

A scenario is **ordered `operations[]`** (a consumer subcommand plus argv, with
`${WORKSPACE}`, `${IMAGE_TAG}` and `${CONTAINER_ID}` tokens), an `oracleType`, and
per-channel `expected[]` assertions. Adding one is a pure data edit — no new Rust.

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
`mount_source_canonical`, `path_env_segmented`, `null_preserving`. **Nothing is blanket
removed.** A rule that dropped a field wholesale would hide the very differences it was
written to make comparable.

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

| Lane | Runs | Gates |
|---|---|---|
| `.github/workflows/parity.yml` | nightly: installs the pinned oracle, prepulls fixture images, runs `--profile parity` | nothing |
| `parity_harness_faults` | `default`, `dev-fast` — hermetic | yes, as an ordinary test |
| `release.yml` | fmt / clippy / tests | yes |

**Selection is profile-based. There is no env-var opt-in and no silent skip.** Live parity
runs ONLY under `cargo nextest run --profile parity`; every other profile excludes it, so
those lanes are truthful by non-selection — a green fast run never *implies* parity ran. A
missing oracle, absent Docker, or a normalization failure FAILS with a cause-specific error
rather than skipping to green.

**The nightly is currently RED and that is known** — see #376. Roughly 127 of the
divergences concentrate in four path-valued fields and are very likely ONE `path_token`
normalization defect rather than many bugs. Before assuming a change made things worse,
diff your run's diverging case ids against a recent `main` nightly.

## What to do when you find a difference

1. **Measure it.** Run both CLIs over the same fixture and record both values verbatim. A
   rationale arguing from reasoning rather than from output is not evidence.
2. **Decide which side is wrong.** deacon → file an issue, write the scenario, leave it
   red; do not waive it. The reference → record it as deacon following the spec. Spec
   silent and the difference deliberate → allowlist it, scoped to one observable path, with
   the measurement in the rationale.
3. **Update `parity/SPEC_STATUS.md` in the same commit.** It is hand-maintained; a row
   describing yesterday's behavior is worse than no row.

## Verifying locally

Docker works in the dev container and the pinned oracle installs cleanly:

```bash
npm install -g @devcontainers/cli@0.87.0
cargo nextest run --profile parity
```

Verify parity changes for real rather than reasoning about them. Docker-backed cases run in
isolated temp workspaces behind an RAII cleanup guard; a run cancelled partway leaves
orphaned containers and Compose networks that trip the next run's resource guard (`all
predefined address pools have been fully subnetted`). Reclaim them by removing containers
whose `devcontainer.local_folder` label names a directory that no longer exists, and
networks whose names carry a `deacon-conf-` workspace basename.

## Further reading

- [`CLAUDE.md`](../CLAUDE.md) § Differential Parity Suite — the working reference
- [`parity/SPEC_STATUS.md`](../parity/SPEC_STATUS.md) — the behavior-by-behavior record
- [`.specify/memory/constitution.md`](../.specify/memory/constitution.md) — the principles
  the strictness choices appeal to
- [`examples/CANARY_STATUS.md`](../examples/CANARY_STATUS.md) — canary state and protocol
