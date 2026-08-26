# HANDOFF

What a fresh session needs that the standing docs do not already carry.

**Read first, in this order:** [`CLAUDE.md`](CLAUDE.md) (how to work in this repo —
non-negotiable), [`docs/PARITY.md`](docs/PARITY.md) (what a divergence, a tolerance and an
oracle are), [`parity/SPEC_STATUS.md`](parity/SPEC_STATUS.md) (the current claim-by-claim
answer to "does deacon behave like the reference?").

This file is *state and method*. It deliberately does not restate rules that live in
`CLAUDE.md` — when a rule earned here proved durable it was moved there, and the line here
points at it.

---

## Where things stand

_Last updated: 2026-08-26, at `d989b36` on `main`._

- **Ledger: 220 recorded behaviors — 0 open nonconformance**, 10 deacon-follows-spec,
  14 documented choice, 23 deacon extension, 173 conformant. Zero `UNADJUDICATED` records.
- **The verdict of record is the latest nightly**, never this file:
  `gh run list --workflow=parity.yml --branch=main`.
- Nothing in flight. No open work assigned.

> `0 open nonconformances` is a statement about the questions that have been asked, not a
> statement about deacon.

### Recently landed

| PR | What |
|---|---|
| #683 `d989b36` | Windows identity-label normalization (closes #682) — batch 18 |
| #681 `d41f3f9` | every parity case runs against a *copy* of its fixture, with a guard proving it (closes #680) |
| #679 `bcea065` | `env` channel for the parity operation model |
| #678 `ab8309b` | control manifests — mechanism implemented, source configurable, default off (closes #676) |
| #677 `5dd03ae` | the disallowed-Features gate stopped failing open (closes #675) |

---

## The arc: [#480](https://github.com/get2knowio/deacon/issues/480), parity mining

The long-running task is mining the reference CLI's own test suite and real-world configs for
behaviors deacon has never been compared against, one upstream file at a time. Eighteen
batches so far.

**The method has held for every batch and should not be shortcut:**

1. **Mine** one upstream file — read every `it`, list what it asserts.
2. **Measure** each claim against the pinned oracle (`@devcontainers/cli@0.87.0`) on a real
   fixture. Never file from a reading of the source.
3. **File** an issue with the measured evidence in it, *before* writing any fix.
4. **Fix**, then land tests and parity scenarios **watched to fail** — revert the fix, confirm
   exactly the intended cases go red, restore.
5. **Update `parity/SPEC_STATUS.md` in the same commit.** A status snapshot that lags the code
   is worse than none.

### Rules the arc earned that are not obvious from the code

- **Measure before sizing a fix.** Batch 18 looked like three normalization behaviors and was
  one: `absolutize` already handled separators and dot segments, and `Path`'s `Hash` already
  folded drive case. Only the label *string* diverged.
- **A queue item marked "deacon may simply not have this" deserves a grep before it deserves a
  skip.** Batch 17 existed only because that assumption was wrong — deacon had a stub nothing
  had exercised in years, which is *worse* than absence on a security-shaped surface, because
  the knob reads as protection while doing nothing.
- **Before invoking "the reference is the authority on a spec-silent surface", grep the spec's
  ISSUE tracker, not just its text.** The disallowed-features list was *declined* by the spec
  (devcontainers/spec#226, closed by its own author) and shipped in the CLI anyway. Silence and
  refusal are different facts and point opposite ways.
- **Port a normalizer differentially, never by reading it.** Batch 18's first port passed all
  18 hand-picked vectors and still had 274 mismatches against node. Node's own source is
  extractable in this container: `node --no-warnings -e 'process.binding("natives").path'`.
- **When a fix is platform-conditional, grep the existing assertions for the platform-blind
  spelling.** Three tests spelled an expectation as `absolutize(...).display()` and were
  quietly pinning the pre-fix behavior on the Windows lane — that was the watch-to-fail there
  was otherwise no way to run.
- **A comparison normalized on one side only is invisible where the normalization is the
  identity.** Batch 18's first push normalized the candidate's label and not the local one;
  five tests went red on Windows and none could have gone red on Linux. The fix was to make the
  comparison a named helper taking the **platform as a parameter**, so its regression test runs
  on every host. Generalize: when behavior is platform-conditional, parameterize the platform
  rather than reading `cfg!` — that is also how upstream tests its own version.
- **Sabotage distinguishes coverage from green.** Disabling an input and re-running is the only
  way to tell a case that covers something from a case that merely passes. It has found a
  passing-for-the-wrong-reason case, a fixture write into the repo, and a broken port.
- **An assertion that cannot fail is worse than no assertion.** Its variants are written up in
  `docs/PARITY.md`.

---

## Queue — none of this has been requested

Do not start any of it without being asked.

1. **Upstream files still uncited by anything in `parity/`:** `dockerfileUtils` (48 `it`s),
   `dockerUtils` (5), `dockerComposeUtils` (5), `cli.podman` (2), `getHomeFolder` (1),
   `getEntPasswd` (1). `cli.up` (21) and `cli.test` (11) are partly mined.
   **`dockerfileUtils` is now the largest by a wide margin** and touches `resolve_base_image`
   and the merged-Dockerfile splice, which #595 and #628/#629 already made load-bearing.
2. The four consciously-dropped coverage areas named in `parity/SPEC_STATUS.md` under
   "Coverage this document does *not* claim".
3. `up_dotfiles.rs`'s 12 ignored, network-dependent tests.
4. Teach the harness to discover a case's container by its OWN declared id-labels. This would
   unblock a differential twin for `${devcontainerId}`: `--id-label` replaces
   `devcontainer.local_folder`, which is the label `require_observed_container` discovers by.
   Until then the pinned digest has no automatic upstream-drift detector — re-measure it at
   each oracle bump.
5. Smaller leads: a per-case time budget or a lighter digest-pinned Feature; run-twice
   vocabulary for the operation model (the gap #620 left); the GID half of the uid-remap guard.
6. An upstream issue for the Compose newline flattening — deliberately **not** filed. The
   maintainer has previously said no upstream filing.

---

## Verifying locally

Docker works in this dev container and the pinned oracle installs cleanly, so verify parity
changes for real rather than reasoning about them.

```bash
npm install -g @devcontainers/cli@0.87.0
scripts/parity/prepull-fixture-images.sh     # before trusting any local parity run
cargo nextest run --profile parity           # selects the differential binary only
```

- **Never run two parity runs against one daemon.** They delete each other's `deacon-conf-`
  containers. Stagger them.
- Docker etiquette, resource reclamation, and the case-*insensitive* name-sweep rule are in
  `CLAUDE.md` under the Differential Parity Suite section. All of it was learned from a leak;
  read it before debugging a resource-pool error.

The full pre-push gate, every time:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
make test-nextest-fast
```

Skipping clippy before a push has cost a CI round trip. `-p deacon` alone misses `deacon-core`
lints and fmt drift in new test files.

---

## Traps that have cost time

- **`git stash` is repo-global.** The stack is shared across every worktree and session. Never
  stash in a worktree agent — copy files instead.
- **`ci.yml` triggers on `pull_request: branches: [main]` only.** A PR opened against another
  feature branch gets the metadata workflows and `parity` and nothing else — five green checks
  that read like a verdict and are not. Retargeting the base does **not** re-trigger; close and
  reopen does. Clean sequence: merge the base, rebase the child onto `main`, force-push with
  `--force-with-lease`.
- **Auto-merge is not enabled on this repository.** `gh pr merge --auto` fails with
  `enablePullRequestAutoMerge`; poll and merge.
- **`Test (MVP fast) (windows)` is the lane that catches platform-conditional work**, and it
  only starts after `Lint` passes — roughly ten minutes into a run. Do not read an early green
  as a verdict.
- **The ledger-coverage check reads issue TITLES as conventional commits.** A `fix(<scope>)`
  outside `NON_BEHAVIORAL_SCOPES` owes a `SPEC_STATUS.md` row. If an issue genuinely owes none,
  retitle it `chore(<scope>)` and say why on the issue — that is the remedy the check's own
  message names.
- **Compile-check a `#[cfg(windows)]` test** by temporarily rewriting the gate to
  `#[cfg(all())]`. nextest compiles every binary before filtering, so a syntax error there
  fails the Windows lane and nothing local catches it.
- Standing rulings that still constrain work: **#660** (`--config` selects which *document*,
  never where the workspace is), **#665** (a symlinked workspace keeps the caller's spelling —
  `workspace::absolutize`, never `canonicalize`, at every site that reports, mounts or hashes;
  `trust.rs::canonicalize_workspace` is the deliberate exception and must not be unified with
  it), **#682** (identity-label values go through `label_path::for_path`, never `absolutize`,
  and comparisons normalize **both** sides).
