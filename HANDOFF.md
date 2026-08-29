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

_Last updated: 2026-08-29, at `2746c76` on `main`._

- **Ledger: 233 recorded behaviors — 0 open nonconformances**, 10 deacon-follows-spec,
  15 documented choice, 25 deacon extension, 183 conformant. Zero `UNADJUDICATED` records.
- **The verdict of record is the latest nightly**, never this file:
  `gh run list --workflow=parity.yml --branch=main`.
- **No parity work in flight**, and the two podman compose issues (706 and 710) stay resolved
  by #715. The backlog issues that remain (`#30`, `#480`, the auto-forward family, …) are
  standing scope, not open questions about deacon's behavior.
- **One thing WILL cost you time on any PR: [#723](https://github.com/get2knowio/deacon/issues/723).**
  The Podman lane fails intermittently in `test_compose_override_command_lifecycle_runs` with
  `container create failed (no logs from conmon): conmon bytes ""`. It is `chore(ci)` —
  non-behavioral, so it owes no ledger row — but `Test (Podman)` is a REQUIRED check and this
  fails roughly half of runs (observed across four runs on #725/#726: fail, pass, fail, pass),
  so expect to re-run. **Three hypotheses are refuted** — daemon contention (#718, reverted by
  #724), container-not-running, and podman/conmon version skew (#726) — and the issue carries
  what is left. The `smoke-lite` trap below is the worked example of how the first was retired.

> `0 open nonconformances` is a statement about the questions that have been asked, not a
> statement about deacon. Batches 25–26 are the demonstration: the count sat at 0 while
> `deacon build` ran every build on a hardcoded `docker`, and again while every compose call
> ran on a hardcoded `docker`, because nothing had asked. **Both were found by making a lane
> honest, not by reading code.**

> **Podman now runs for real in this dev container** (`.devcontainer/setup-podman.sh`,
> `start-podman-socket.sh`). `DEACON_CONTAINER_RUNTIME=podman cargo nextest run …` is a real
> local measurement, and it is what validated #715 and found two things CI needed before it
> could. Earlier handoffs said podman could not run containers here; that was never a sandbox
> limit — see the trap below.

### Recently landed

| PR | What |
|---|---|
| #725 `2746c76` | Feature content stages without BuildKit build contexts, so Features install on podman compose and under `--buildkit never` (closes issue 719) — ledger back to 0 open |
| #726 `402d787` | the Podman lane runs the podman the runner image ships (5.8.4), not an apt copy it never executed |
| #724 `9f81cb6` | reverts #718 — the throttle did not fix the conmon flake, so it was ~14% off every smoke lane for nothing |
| #722 `89ecc2d` | each side of a parity case gets its own `TMPDIR`, so the reference's `Date.now()`-keyed temp dir cannot collide |
| #720 `8db4717` | the podman lane's build-test exclusions are named and measured rather than inherited |
| #718 `a1ada8d` | `smoke-lite` 4 → 2 — **REVERTED by #724**, listed so the round trip is not repeated |
| #717 `fd74825` | HANDOFF refresh through batch 26 |
| #715 `d723c12` | Compose runs on the RESOLVED runtime, not a literal `docker` — resolves issues 706 and 710; podman lane gains the compose binary |
| #716 `a1bffc2` | rootless podman works in this dev container, so the Podman lane is reproducible locally |
| #714 `768965d` | `chacha20` off the yanked 0.10.0 — `Security (cargo deny)` was red repo-wide |
| #713 `1891f51` | the closing-keyword trap entry no longer carries a live keyword |
| #711 `75d34d9` | HANDOFF refresh through batch 25 |
| #709 `d9f7202` | `build` runs on the runtime it was told to use, not a literal `docker` (closes #708); build tests verify through that runtime |
| #707 `2832431` | podman's `image not known` reads as *absent*, so `ensure_image_available` pulls instead of erroring |
| #705 `4abfda8` | the Podman lane runs deacon's build tests (#30) |
| #704 `4f2a8fd` | guard: every subcommand deacon advertises has a ledger section — batch 25 (`cli.test`) |
| #702 `4f0e8d6` | the compose Feature build is deterministic, so a stopped container is reused (closes #700) — batch 24 |
| #699 `a5526f0` | HANDOFF refresh through batch 23 |
| #698 `dad1550` | records that podman `build` works, and bounds that claim |
| #697 `5e674f6` | `build` uses the runtime it was told to use — flavor and binary (closes #694) |
| #696 `7bee127` | `_REMOTE_USER_HOME` read from the image's passwd DB (closes #695) — batch 23 |
| #693 `1097d40` | `--docker-path` honored on every subcommand; flavor detected from the binary (closes #692) — batch 22 |
| #691 `9af1867` | Compose supplies the build defaults the reference hand-codes — pinned (no defect) — batch 21 |
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
behaviors deacon has never been compared against, one upstream file at a time. Twenty-six
batches so far, and the upstream files are now exhausted (see the queue).

**With the files exhausted, the productive source has shifted from mining upstream tests to
making deacon's own lanes honest.** Batches 25 and 26 found four defects between them — none
from reading upstream, all from noticing that a green lane was reporting coverage it did not
have. That is where the remaining yield is.

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
- **If your fix needs a SECOND fix to keep something else working, you are treating a symptom.**
  Batch 24's first attempt resumed the stopped compose container; reuse worked and the lifecycle
  broke, because the resumed project took the reconnect branch that deliberately runs no phases.
  Closing that PR and fixing image identity instead needed no control-flow change at all. Go back
  and find the layer where ONE change suffices.
- **Rule candidates out one at a time, and say which are eliminated.** That is what found batch
  24's cause: project name (stable), `--force-recreate` (absent), compose config-hash (identical),
  Feature staging mtimes (identical), layers, `Config`, `created` — all eliminated, leaving the
  BuildKit provenance attestation.
- **"Identical inputs produced a different image" means attestations.** BuildKit attaches a fresh
  provenance manifest to every build, so a fully CACHED rebuild still yields a new image ID
  (measured: `99483d1aab35` then `519839eb5424`; with `BUILDX_NO_DEFAULT_ATTESTATIONS=1` both
  `c0b6b3e6d519`). It is also what causes the concurrent-build deterministic-tag race already
  documented in `CLAUDE.md`.
- **When a fix would entrench a limitation, first measure whether the limitation is real.** #694
  was going to make `build` REFUSE podman. Measuring instead showed the reference does not branch
  its build command on podman, `podman buildx` is an alias for `podman build`, `podman build`
  supports `--build-context`, and deacon already had both podman behaviors — so the refusal would
  have permanently rejected a configuration that demonstrably works (proven in CI:
  `smoke_build_json_then_text` passes under `DEACON_CONTAINER_RUNTIME=podman`). The asymmetry is
  the point: entrenching a false limitation is permanent, testing costs one CI run.
- **A comment excusing a gap is a lead, not a boundary.** Three in one stretch, each hiding real
  work: `dockerfile_utils`' "small subset … out of scope" (#686), `shared/mod.rs`'s warning that
  `CliDocker::new()` ignores a podman selection — which `build` then did at four sites (#692) —
  and `// deacon build is docker-only today` (#694). Grep for "for now", "out of scope", "today".
- **A warning comment is not a guard.** If a rule matters, something must enforce it.
- **When only the container can know an answer, emit a lookup — do not compute a guess on the
  host.** `_REMOTE_USER_HOME` was `/home/<name>` by string formatting; the reference resolves it
  in-image from the passwd DB (#695). Worth grepping for other host-side guesses about container
  state.
- **Exit 0 means "did not fail", never "did the thing".** Cost three separate wrong conclusions:
  `build --runtime podman` exiting 0 with no podman installed; `doctor`/`down` "ignoring"
  `--runtime` when they had simply not failed; an image-reference `build` "ignoring"
  `--docker-path` when that shape never invokes a binary.
- **A watch-to-fail that PASSES means the test does not cover the thing.** Not that the thing is
  fine. Sabotage every assertion you rely on.
- **Sabotage distinguishes coverage from green.** Disabling an input and re-running is the only
  way to tell a case that covers something from a case that merely passes. It has found a
  passing-for-the-wrong-reason case, a fixture write into the repo, and a broken port.
- **A green LANE can report coverage it does not have. Verify which BINARY did the work.**
  #705 reported "34 of 36 build tests pass on Podman" and the number was fiction: `build`
  validated against the resolved runtime and then ran `Command::new("docker")`, so on a runner
  with docker installed the builds landed on docker inside a step labelled Podman (#708). The
  measurement that settles it is a **shim** — a script that appends `"$@"` to a log and `exec`s
  the real binary — passed as `--docker-path`. Five invocations reached it and the build was not
  among them. Two traps in doing this: use a **fresh config hash**, because a cache hit exits 0
  having invoked nothing and proves nothing; and the pre-existing "`--docker-path /nonexistent/…`
  must fail" test passes throughout, because the probes were always the honest part.
- **The same prefix can be right and wrong on the same day; what decides it is who PARSES it.**
  `localhost/` was applied to podman image refs three times in one stretch and was wrong twice.
  On the compose service-image **override** it broke two passing tests, because a compose
  `image:` is read by the compose CLIENT and docker-compose treats `localhost` as a registry
  HOST (`Get "http://localhost/v2/"`). On the `podman tag` **source** it only changed which
  spelling was reported missing. The reference's own use (`containerFeatures.ts:470`) is a
  `BASE_IMAGE` build-arg consumed by `podman build` — a third consumer entirely. Before
  transplanting an idiom, name the parser at the destination.
- **If your fix only changes the ERROR STRING, you are at the wrong layer.** `X: image not
  known` becoming `docker.io/library/X: image not known` was the tell that the compose image was
  in no podman store under any spelling — a store problem wearing a naming problem's clothes.
  Two CI cycles were spent before reading that as a signal rather than as progress.
- **A test that verifies by shelling out must use the runtime under test.** Three build test
  files checked deacon's output with a hardcoded `docker`, reading a different image store than
  the deacon under test wrote to — and never failed loudly, because every runner has docker
  installed. `integration_up_exec_identity` already had the `runtime_bin()` helper and the
  comment explaining exactly this hazard; grep for an existing helper before writing the fix.
- **Unmasking a false green means the number gets WORSE before it means anything.** The Podman
  lane went 34/34 (false) → 24/10 → 26/6 → 26/26. A rising failure count after a
  routing/visibility fix is the fix working. Budget for it: each honest run surfaces the next
  layer, and two of those layers were mine to fix while two were podman capability gaps.
- **An assertion that cannot fail is worse than no assertion.** Its variants are written up in
  `docs/PARITY.md`.
- **A hypothesis written into an issue is still a hypothesis, and filing it makes it look
  settled.** Issue 710 opened with a stated mechanism — podman delegates compose to an external
  `docker-compose` which then talks to the DOCKER daemon — and a checklist that began "first
  step is a measurement, not a fix". The measurement refuted the mechanism: podman points the
  provider at its OWN socket, exporting `DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock`
  (plus `DOCKER_BUILDKIT=0`, plus a blanked `DOCKER_CONFIG`). deacon never reached that
  delegation — it was running `docker compose` itself, because `ComposeManager::new()` defaults
  `docker_path` to `"docker"` and five other sites passed the raw `--docker-path` flag, which
  `binary_for` only maps INSIDE `resolve_runtime`. Three wrong diagnoses on one issue, each
  confidently recorded. **Write the measurement command into the issue, not the conclusion you
  expect it to reach.**
- **An environment limit remembered from an error message may be a setting nobody configured.**
  "Podman cannot run containers in this dev container" was recorded from two real errors —
  `newuidmap: write to uid_map failed` and `fuse: device not found` — and stood for two days.
  Neither was a capability the sandbox withheld: the first was `/etc/subuid` naming ids outside
  the ambient userns, the second a storage driver that needed a device node. Both fixed in a
  shell script. **Before recording an environment limit, ask whether the error names a
  capability that is absent or a setting that is wrong.**
- **When a fix would make a lane honest, expect the honest number to be WORSE, and budget for
  the layers underneath.** Routing compose onto the resolved runtime turned four green tests
  red — all of them tests that verified deacon's output by shelling out to a hardcoded
  `docker`, the same defect class #705 had already fixed elsewhere. They had passed only
  because compose ALSO ran on docker. The failure surfaced as `postCreateCommand should have
  created /tmp/deacon-lifecycle-marker`, which reads like a lifecycle bug and is not one.
- **A test fix that moves the failure to a different LINE is progress; read the line number.**
  Same test, first run failed at the marker assertion, second at `deacon up`. That is two
  different facts, and treating the second as "the fix didn't work" would have been wrong.
- **A plausible mechanism is not evidence — re-run before reclassifying a flake.** A podman
  `conmon bytes ""` failure had a good story behind it (four concurrent compose projects in
  `smoke-lite`, now on podman rather than docker, starving conmon on a shared runner) and a
  ready fix (halve the group). Re-running passed. The story may still be true at the margin,
  but acting on it would have slowed every docker lane on one data point. **Cost of a re-run:
  one CI cycle. Cost of a wrong global concurrency change: every lane, forever.**
- **A permission classifier refusing an edit can be correct.** Wiring podman into the
  devcontainer, the first attempt added `--device=/dev/fuse` plus `seccomp=unconfined` and
  `apparmor=unconfined` to `runArgs` — reached for from the standard podman-in-docker recipe,
  without checking whether this container needed them. It did not: everything was then verified
  on the `vfs` driver with no added privileges. **When a guard fires on a privilege escalation,
  check whether the privilege is load-bearing before arguing with it.**

---

## Queue — none of this has been requested

Do not start any of it without being asked.

1. **Upstream files still uncited by anything in `parity/`: none with material left.**
   Batches 19–25 closed `dockerfileUtils` (48), `dockerUtils` (5), `dockerComposeUtils` (5),
   `cli.podman` (2), `getHomeFolder` (1), `getEntPasswd` (1), `cli.up` (21) and `cli.test` (11).
   `cli.test` is the reference's Feature-*authoring* surface, permanently out of scope, so batch
   25 produced a guard rather than cases: `ledger_covers_every_subcommand` asserts every
   subcommand deacon advertises has a `SPEC_STATUS.md` section. Three parts were deliberately
   left, each with the place it would surface:
   - **`supportsBuildContexts`** (`dockerfileUtils`). Upstream uses it to decide whether to
     prepend `# syntax=docker/dockerfile:1.4`; deacon emits no syntax directive while always
     passing `--build-context`. Fine on modern BuildKit; on an OLDER Docker the reference would
     succeed where deacon fails. **UNMEASURED** — needs an old BuildKit this container lacks.
   - **`inspectImageInRegistry`** (`dockerUtils`). No deacon equivalent: deacon pulls and inspects
     locally where the reference reads image metadata WITHOUT pulling. Would surface on a config
     whose image can be inspected but not pulled. (`qualifyImageName` is NOT a lead — it only
     builds a registry API path for that function, and deacon's same-sounding
     `qualify_short_remote` is an unrelated podman short-name helper. Do not "align" them.)
   - **The Podman lane's `build` coverage** (#30) — DONE, and it cost three passes. #705
     widened the selection; #708 found `build` was executing on a hardcoded `docker` regardless,
     so the first numbers were fiction; #715 found the same of every compose call. The lane now
     selects **30 build tests, all passing on podman for real**, alongside 433 integration
     tests. Six exclusions, each by name with a measured reason: three buildx EXPORTER
     capability gaps (`--output type=docker`, `--output type=oci`, `--cache-to type=local` —
     `podman buildx` is an alias for `podman build` and supports neither grammar), and three
     Feature-extended compose builds, because podman sets `DOCKER_BUILDKIT=0` for the compose
     provider and the classic builder has no additional build contexts. Only `build:`-shape
     services hit the last one; the `image:` shape builds its Feature image with `podman build`
     directly. **The lane also needs podman's API socket started** — its setup step used to
     disclaim one, and without it every compose call fails with `Cannot connect to the Docker
     daemon`. Still open on #30: GPU passthrough is unwired for podman.
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

- **Closing keywords fire from COMMIT MESSAGES, and they cost this repo the same issue twice
  in one afternoon.** Both halves are worth knowing:
  1. *A squash merge concatenates commit messages, so a stale keyword survives a rewritten PR
     body.* PR #707 opened expecting to fix issue 706; the measurement said otherwise and the
     body was rewritten to "Refs …, stays open" — but the branch's first commit still carried
     the keyword, and merging shut an unfixed issue. Rewriting the body is NOT enough: amend or
     rebase the offending commit.
  2. *You cannot DESCRIBE a closing keyword without triggering it, and that applies to the PR
     body as well as the commit.* The commit documenting trap 1 quoted the offending sentence
     as evidence — verb adjacent to the issue number — and shut the same issue a second time.
     The PR documenting THAT quoted it once more in its description, and shut it a third time,
     because a squash merge consumes the PR body too. Same issue, three times in one afternoon.
     There is no escaping syntax and no prose context that disarms it: quoted, negated or
     merely described, the pattern fires.
  3. *It caught a fourth issue on 2026-08-29, in a sentence written to AVOID overclaiming.*
     #726's commit said `This does NOT fix <number>` — a deliberate disclaimer, because the
     change genuinely did not fix the flake. Merging shut the issue anyway. The author had
     read this very entry earlier the same session. **Negation is not an escape; there is no
     prose context that disarms the pattern.**
  **The remedy is to never put such a verb adjacent to a `#`-number in a commit message or PR
  body at all** — write `issue 706`, or split the verb and the number across clauses. Say
  `does not address issue 723`, never `does not fix #723`. This file now follows its own
  advice, so the examples above are described rather than quoted.
  And **check `gh issue view <n> --json state` after any merge whose scope changed
  mid-flight** — nothing warns you, and a ledger row can quietly end up pointing at a shut
  issue. That check is what caught all three.
- **Podman delegates `compose` to an EXTERNAL provider, and points it at PODMAN.** The lane
  log names the provider (`>>>> Executing external compose provider
  "/usr/libexec/docker/cli-plugins/docker-compose"`). What matters is the environment podman
  hands it, measured by pointing `compose_providers` at a script that dumps its own env:
  `DOCKER_HOST=unix://$XDG_RUNTIME_DIR/podman/podman.sock`, `DOCKER_BUILDKIT=0`, and an
  emptied `DOCKER_CONFIG`. Three consequences:
  1. **Compose needs a socket listening.** No systemd here or on the runner, so run
     `podman system service --time=0 unix://…` directly, or every compose call fails with
     `Cannot connect to the Docker daemon`.
  2. **`DOCKER_BUILDKIT=0` pins compose to the classic builder**, which has no additional
     build contexts — the mechanism deacon uses to stage Feature content.
  3. A compose `image:` value is parsed by **docker-compose**, not by podman's short-name
     logic, so `localhost/` does not belong there.
  **An earlier version of this entry said the provider talks to the DOCKER daemon. It does
  not.** That error survived two batches and produced two wrong fixes and one wrongly-premised
  issue. The image genuinely was in docker's store — because deacon was running `docker
  compose` itself, not because of anything podman did.
- **`smoke-lite` is `max-threads = 4`, and the attempt to make it 2 is a worked example of
  fixing a flake by theory.** The reasoning was good: #715 had just moved compose onto rootless
  podman, so four concurrent smoke-lite tests became four concurrent PODMAN compose projects,
  and `test_compose_override_command_lifecycle_runs` began failing with `container create
  failed (no logs from conmon): conmon bytes ""`. Contention was the obvious cause, and the
  test passes 5/5 locally when run serially. #718 dropped the throttle to 2 — **and the failure
  recurred unchanged.** Reverted in #724. Because `[test-groups]` has no per-profile form, the
  throttle cost ~14% on EVERY smoke lane (the Podman suite step went 195s → 222s) and bought
  nothing. The refutation is recorded in `.config/nextest.toml` beside the setting so the round
  trip is not repeated. **The transferable part: a flake that reproduces serially-never and
  concurrently-sometimes still is not proof of contention** — measure by removing the
  suspected cause, and revert when the failure survives. The live state of that flake is
  [#723](https://github.com/get2knowio/deacon/issues/723).
- **The Podman lane runs the podman the runner IMAGE ships, not the one apt installs.** The
  setup step used to `apt-get install -y podman`; `/usr/local/bin` precedes `/usr/bin` on
  PATH, so the apt copy was never executed. Measured: `podman version` answered **5.8.4**
  from `/usr/local/bin` with helpers out of `/usr/local/lib/podman`, while apt had just
  unpacked **4.9.3** and conmon **2.1.10** into `/usr/bin`. The unused binaries were the
  harmless half — the apt package also lays down `/etc/containers/*` written for 4.9.3, so a
  5.8.4 engine ran on a 4.9.3 configuration. Removed in #726, after confirming nothing
  depended on it: the image ships `/etc/containers` complete (`containers.conf`,
  `policy.json`, `registries.conf`, `seccomp.json`, `storage.conf`) and provides `pasta`,
  podman 5's default `rootlessNetworkCmd`. **Two reusable pieces.** *`command -v conmon` does
  not answer the question* — podman resolves helpers through `containers.conf` and never
  consults PATH, so ask `podman info --format '{{.Host.Conmon.Path}}'`. Asking PATH is what
  made one round of #723 diagnostics report 2.1.10 while podman was running 2.2.1, a
  self-contradiction that cost real time. And *a setup step that provisions a tool the job
  does not use is worse than one that provisions nothing*, because its log reads like
  evidence.
- **Copying a file between branches imports that branch's OTHER changes, silently.** Building
  #726 meant taking `ci.yml` from the #725 working branch (to avoid `git stash`, correctly)
  and applying it onto `main` — which brought #725's filterset change along without #725's
  code fix, un-excluding three tests that then failed. It survived review of the step being
  edited, because the damage was two hundred lines away. **`git diff <base>` and assert the
  diff contains ONLY what you meant**, by pattern and not by eye: here, that no `test(=`,
  `binary(=` or `nextest run` line appears at all. A file copy is not a patch; treat it as
  one and it carries whatever else the source had.
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
- **A fixture that does not declare a Feature cannot cover a Feature-build defect.**
  `fx-up-compose-restart-phases` declares none, which is why the compose-restart parity case never
  caught #700 — a Feature-less compose image is already stable. Two of the regression tests written
  for it made the same mistake and PASSED against the defect; sabotage caught both. Upstream's case
  is named "docker-compose with Dockerfile WITH features" and that word is load-bearing.
- **Assert container state on a path that a recreate would destroy.** A marker written into a mount
  survives being replaced and passes either way; write it to `/`.
- **`gh pr checks` lists check-runs that EXIST — a MISSING check reads exactly like a green one.**
  Re-running a workflow deletes its check-run and creates the replacement only when the job
  starts, so a `startup_failure` leaves the check absent and
  `gh pr checks | select(.bucket != "pass")` returns nothing. Verify by COUNT and PRESENCE:
  `gh api repos/<o>/<r>/commits/<sha>/check-runs`. It can also return the PREVIOUS run's results
  before a new run registers — key a watch on the RUN ID for a specific head SHA.
- **Before citing a lane as evidence, list the binaries it actually ran**
  (`gh run view <id> --log | grep -oE 'deacon::[a-z_0-9]+' | sort -u`). The Podman job does not
  run `integration_build`; a local `--profile default` does, which is how the same command gave
  different answers in the two places.
- **`live-certification` flakes on PR runs** (three in one day: oracle-side `ECONNRESET` twice, and
  the resource-reclamation guard reporting a false leaked Compose network). It does not gate, and
  `main`'s nightlies are green. **Read the failure before re-running** — "no case diverged, the
  guard fired" is a different fact from "a case diverged".
- **Podman runs containers here now — this entry used to say it could not, and that was
  wrong.** The two errors it cited (`newuidmap: write to uid_map failed`, `fuse: device not
  found`) were unconfigured prerequisites, not sandbox limits; `.devcontainer/setup-podman.sh`
  handles them and `start-podman-socket.sh` starts the socket compose needs. Three things it
  configures, each worth knowing if it ever breaks:
  - **Subuid ranges must fit the AMBIENT userns.** This container already runs in one
    (`/proc/self/uid_map` = `0 100000 65536`), so the image's `vscode:100000:65536` names
    unrepresentable ids. Podman also refuses a single range containing the user's own uid, so
    it straddles: `1:999` + `1001:64535`. **The size is load-bearing** — a range short of
    65534 cannot map `nobody`, and Feature installs then die inside apt with
    `setgroups 65534 failed`, which looks nothing like an id-mapping problem.
  - **`vfs` unless `/dev/fuse` is present.** Overlay needs the device node; `mknod` is refused
    inside. vfs needs no added privileges — prefer it over granting `--device` or
    `seccomp=unconfined`.
  - **`XDG_RUNTIME_DIR`**, which nothing creates without systemd.
  Useful inversion, still true: `deacon <cmd> --docker-path podman` failing with podman's OWN
  error is positive evidence that deacon selected and executed podman.
- **The upstream checkout does not persist across sessions.** Re-clone into the CURRENT scratchpad:
  `git clone --depth 1 --branch v0.87.0 https://github.com/devcontainers/cli.git upstream-cli`,
  and verify with `git describe --tags` before trusting it as the oracle.
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
  and comparisons normalize **both** sides), **#684**/**#690**/**#698** (this file and the
  ledger are refreshed in the same batch that changes them), **#685** (the Feature-install
  stage becomes `root` after its `FROM` and restores the IMAGE's user — not the config's
  `containerUser` — on BOTH generator entry points), **#686** (`dockerfile_utils` is a FULL
  port of the reference's `dockerfileUtils.ts`; keep
  `crates/core/tests/dockerfile_utils_parity.rs` at zero divergences, and **regenerate its
  fixture by re-measuring, never by hand-editing an expectation**), **#692**/**#694** (runtime
  selection goes through `shared::resolve_runtime` — flavor AND binary — never
  `CliDocker::new()`), **#695** (a user's home is READ from the image's passwd DB, never
  derived from the name), **#700** (a cached compose Feature build must produce the SAME image
  — `BUILDX_NO_DEFAULT_ATTESTATIONS` on the compose `build` invocation only, never on a
  publishing path), **#688** (a removal that races another removal is waited out, never
  reported as done — `is_already_gone` governs the REMOVE step and `stopping_is_moot` the STOP
  step, and they are deliberately not one function), **#715** (the COMPOSE client is the
  resolved runtime binary — `ComposeManager::with_docker_path(<resolved>)`, never
  `ComposeManager::new()` and never the raw `--docker-path` flag, which reads `"docker"` under
  `--runtime podman`; and any test that verifies deacon's output by shelling out uses
  `support::runtime_bin()`, never a literal `docker`).
