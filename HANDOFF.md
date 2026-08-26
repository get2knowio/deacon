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

_Last updated: 2026-08-26, at `d540c7e` on `main`._

- **Ledger: 225 recorded behaviors — 0 open nonconformance**, 10 deacon-follows-spec,
  15 documented choice, 24 deacon extension, 176 conformant. Zero `UNADJUDICATED` records.
- **The verdict of record is the latest nightly**, never this file:
  `gh run list --workflow=parity.yml --branch=main`.
- Nothing in flight. No open work assigned.

> `0 open nonconformances` is a statement about the questions that have been asked, not a
> statement about deacon.

### Recently landed

| PR | What |
|---|---|
| #689 `d540c7e` | a concurrent container removal is waited out rather than guessed at (closes #688) — batch 20 |
| #687 `6e76822` | Feature installs run as root; real Dockerfile `FROM`/`USER` variable resolution (closes #685, #686) — batch 19 |
| #683 `d989b36` | Windows identity-label normalization (closes #682) — batch 18 |
| #681 `d41f3f9` | every parity case runs against a *copy* of its fixture, with a guard proving it (closes #680) |
| #679 `bcea065` | `env` channel for the parity operation model |
| #678 `ab8309b` | control manifests — mechanism implemented, source configurable, default off (closes #676) |
| #677 `5dd03ae` | the disallowed-Features gate stopped failing open (closes #675) |

---

## The arc: [#480](https://github.com/get2knowio/deacon/issues/480), parity mining

The long-running task is mining the reference CLI's own test suite and real-world configs for
behaviors deacon has never been compared against, one upstream file at a time. Twenty
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
- **A module that describes itself as a partial port is making a claim, not stating a policy.**
  `dockerfile_utils` called itself "the small subset ... we need today" and put variable
  substitution "out of scope for bead 14b". It was neither current nor authorized — deacon's ONLY
  out-of-scope area is Feature *authoring* and *publishing*. Batch 19 found two defects behind that
  sentence, one of which broke `build` outright for a very common Dockerfile shape. Grep for
  "subset", "for now", "out of scope" in module docs; each is a lead.
- **A partially-correct module reads as a correct one.** `ensureDockerfileHasFinalStageName` was
  byte-exact right on every one of its cases while everything around it was wrong, which is exactly
  why nobody looked. Coverage of *part* of a file is not evidence about the rest of it.
- **Run the control that removes your hypothesis.** Batch 19's first end-to-end run showed deacon
  failing where the reference succeeded, and the obvious story explained all of it. The same fixture
  with the variable removed still failed — which is what turned one bug into two, filed and fixed
  separately.
- **The oracle for a library-level port is the reference's own source, compiled.** The npm package
  ships one minified bundle, so internal modules cannot be required from it; check the upstream repo
  out at the pinned tag and `tsc` the single file. Self-check the compiled oracle against upstream's
  own test expectations *before* trusting it to judge deacon.
- **A watch-to-fail that PASSES is telling you the test does not cover the thing — not that the
  thing is fine.** Batch 20's end-to-end test was written to cover two failure shapes and covered
  one; reinstating the second bug left it green 4/4, because that race is timing-dependent. The
  fix was a deterministic unit test over the predicate, plus a doc comment on the E2E saying what
  it does *not* cover. Never let a green sabotage run stand as evidence.
- **Before claiming a command is broken, check what it is contracted to do.** Batch 20's issue was
  filed claiming plain `deacon down` reported success while the container survived. `down` *stops*
  without removing (default `shutdownAction: stopContainer`) — a surviving container there is
  correct, and a racing `docker rm -f` in the measurement had removed it. The issue was corrected
  in place rather than quietly narrowed.
- **A fix that narrows a shared predicate must be re-measured on every shape that used it.**
  Narrowing `is_already_gone` fixed the remove step and regressed `--all` to exit 2, because the
  stop step asks a different question of the same error string.
- **Don't invent a match pattern for an error string you have not observed.** The concurrent-removal
  retry matches docker's exact phrase; podman's wording is unmeasured, so it is recorded as a known
  gap in the code, the ledger row and the test's skip — not guessed at. A matcher for an unobserved
  string is a matcher that silently matches nothing.
- **Sabotage distinguishes coverage from green.** Disabling an input and re-running is the only
  way to tell a case that covers something from a case that merely passes. It has found a
  passing-for-the-wrong-reason case, a fixture write into the repo, and a broken port.
- **An assertion that cannot fail is worse than no assertion.** Its variants are written up in
  `docs/PARITY.md`.

---

## Queue — none of this has been requested

Do not start any of it without being asked.

1. **Upstream files still uncited by anything in `parity/`:** `dockerComposeUtils` (5),
   `cli.podman` (2), `getHomeFolder` (1), `getEntPasswd` (1). `cli.up` (21) and `cli.test` (11)
   are partly mined. Two files were mined with a part deliberately left in each:
   `dockerUtils` (batch 20) left **`inspectImageInRegistry`**, which has no deacon equivalent at
   all — deacon pulls and inspects locally, where the reference reads an image's metadata WITHOUT
   pulling it. A real behavioral difference that surfaced on no observable channel; the place it
   would show is a config whose image can be inspected but not pulled. (`qualifyImageName` is NOT
   a lead: it only builds a registry API path for that function, and deacon's same-sounding
   `qualify_short_remote` is an unrelated podman short-name helper. Do not "align" them.)
   `dockerfileUtils` (48) was mined in batch 19 — **with one part deliberately left**:
   `supportsBuildContexts` is NOT ported.
   Upstream uses it to decide whether to prepend `# syntax=docker/dockerfile:1.4` to the
   generated Dockerfile; deacon emits no syntax directive at all while always passing
   `--build-context`. Every batch-19 build passed because modern BuildKit needs no directive,
   but on an older Docker the reference would succeed where deacon fails. **UNMEASURED** —
   demonstrating it needs an old BuildKit this dev container does not have.
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
- **Adding a NEW docker-gated test binary means editing ~10 `.config/nextest.toml` filter lines**
  across profiles, and conflicts with any sibling PR touching the same lines. Prefer adding to a
  binary already present in every profile — `integration_build_output` is one.
- **`live-certification` is not a required check and fails on oracle-side network flakes.** Batch
  19 saw it go red on `read ECONNRESET` fetching a Feature from ghcr.io — the REFERENCE exited 1,
  deacon did not, so the exit codes diverged. Read the reference's own stderr in the failure before
  suspecting the change; re-running cleared it.
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
  and comparisons normalize **both** sides), **#685** (the Feature-install stage becomes `root`
  after its `FROM` and restores the IMAGE's user — not the config's `containerUser` — on BOTH
  generator entry points), **#686** (`dockerfile_utils` is a FULL port of the reference's
  `dockerfileUtils.ts`; keep `crates/core/tests/dockerfile_utils_parity.rs` at zero divergences,
  and **regenerate its fixture by re-measuring, never by hand-editing an expectation**), **#688**
  (a removal that races another removal is waited out, never reported as done — `is_already_gone`
  governs the REMOVE step and `stopping_is_moot` the STOP step, and they are deliberately not one
  function).
