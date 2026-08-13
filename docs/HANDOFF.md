# Session Handoff — Differential Parity Steady State

Last updated: 2026-08-13, `main` at `7d643b1` (required checks green; the
differential nightly is expected GREEN for the first time since #573, see below;
v0.3.0 shipped from `d642e8d`). This document is the cross-session
handoff for the ongoing parity/quality campaign: where the project stands, how the
work is being run, and what is queued next. When a queue item lands or a rule
changes, update this file in the same PR.

## Where things stand

- **Zero open nonconformances.** `parity/SPEC_STATUS.md` records **135 behaviors**:
  0 open nonconformance, 9 follows-spec, 11 documented choice, 20 extension, 95
  conformant-and-matching. The header census is CI-guarded
  (`check_spec_status_census` in `crates/parity-harness/src/registry.rs`) — rows and
  header must change together. **Zero is not the goal; an accurate ledger is**, and
  the way it was reached is the point: the column went 0 → 2 → 1 → 0 across two days
  as #480 batch 3 measured two real defects (#571, #572), landed them as
  red-on-purpose cases the day it found them, and both were then closed with their
  cases turning green **as authored** (#575, #578). No assertion was bent to reach
  zero. Expect the column to reopen — it has three times now, every time someone
  measured something the paperwork had only reasoned about, and the unfiled findings
  in the queue below are the next candidates.
- **Closing a defect can ADD rows, and that is not inflation.** #575 took the
  census 132 → 134 while closing an open nonconformance, because making deacon read
  the lockfile made two further behaviors observable that were previously vacuous:
  `--no-lockfile` suppresses the READ (not only the write), and an unparseable
  lockfile now fails the build instead of being silently replaced. A fix that
  reveals adjacent behavior should record it in the same commit.
- **A recurring shape worth naming: batch 2 CLASSIFIED two items it had not
  MEASURED, and both classifications were wrong in the same direction** — the work
  was ordinary and the answers were not what the paperwork said. `lockfile-oci-integrity`
  was carried as "deferred, most landable"; measured, it is a defect. The
  compose-naming trio was carried as "blocked on a maintainer ruling"; it was never
  blocked, and it held a second defect. Treat an inventory's *reasons* as claims
  with the same standing as any other unmeasured claim.
- **The differential nightly should be GREEN again — the first time since #573.**
  Both red-on-purpose records are now closed, and `live-certification` passed on
  #578 and again on the rebased #577, which is the same workflow run against a
  branch. That is strong evidence, not proof: confirm against the next real nightly
  (`gh run list --workflow=parity.yml --branch=main`) before treating it as
  established. **This matters more than the count does.** A job that is known-red
  teaches nobody anything — every run needs a human to decide whether the red is the
  expected red. Green restores it to a signal, so the next divergence stands out on
  its own. The standing rule still applies whenever a case is red on purpose again:
  the check is whether the diverging set is exactly those cases and nothing else,
  and the fix is to the product, never to the case. **`parity.yml` is not a required
  check** (see the check-watching rules); it surfaces on PRs as `live-certification`.
- **The pinned oracle** is `@devcontainers/cli@0.87.0`
  (`npm install -g @devcontainers/cli@0.87.0`). Docker and the oracle both work in
  this dev container — verify parity changes for real, never by reasoning alone.
- **⚠️ This dev container's Docker daemon was modified (2026-08-06, decision
  pending).** Docker 29's default containerd-snapshotter + BuildKit could not
  mount overlay here — *every* `RUN` failed (`operation not permitted`,
  `userxattr`) for BOTH CLIs, making feature-installing builds impossible. The
  #417 agent wrote `/etc/docker/daemon.json` =
  `{"features": {"containerd-snapshotter": false}}` and restarted dockerd (this
  persists across host restarts). Costs: `case-build-output-export-tar` and
  `case-build-output-metadata-label` fail locally (`--output type=docker` needs
  the containerd store; CI is unaffected), and some large images
  (`universal:2-linux`, `base:ubuntu`) fail layer extraction on this
  overlay2-on-btrfs setup. Untried alternative that might fix both:
  `{"features": {"containerd-snapshotter": true}, "storage-driver": "btrfs"}`.
  Maintainer decides: keep, revert, or try the btrfs snapshotter. Until then,
  treat those local failures as environment-pinned, not regressions (#538's
  neutrality diff confirmed identical failure sets at HEAD and `main`).
- **⚠️ Second local-only failure class, observed 2026-08-12:** uncached `apt`
  installs inside BuildKit die with
  `dpkg-deb: error: paste subprocess was killed by signal (Broken pipe)` →
  `E: Sub-process /usr/bin/dpkg returned an error code (1)`. That takes out ~6
  Docker-profile tests (`common-utils` / `ca-certificates` on `debian:bookworm-slim`).
  **Reproduced with the REFERENCE CLI on the same base + Feature**, which is what
  makes it the host and not deacon — run that control before spending a session on
  it. CI is unaffected. A warm BuildKit cache hides it, so a suite that passed an
  hour earlier proves nothing about it.

## Recent landings (this campaign, newest first)

| PR | What | Issues |
|---|---|---|
| #577 `7d643b1` | **`up --frozen-lockfile` refuses BEFORE any daemon work when the Features arrive via `--additional-features`** — deacon resolved and BUILT the Feature-extended image, then refused; the reference issues zero docker commands. The fix is an ORDERING change, not a new gate: the `--additional-features` merge now runs before `ensure_lockfile_usable`, so the gate is handed the same UNION the reference's early return keys off (`userFeaturesToArray`). **The issue's own cause analysis was half wrong** and measurement caught it — it asserted `build` shared the shape; `build` already merged before the gate and was already correct, so only `up` changed. **No parity case, said plainly rather than papered over:** exit code, result-document substance and workspace bytes ALL agreed before the fix, and the one thing that differed — an intermediate image — is observable on no declared channel (`chan-image` reads the image an operation PRODUCED, and this one produces none). Pinned instead by a hermetic test that runs `up` with an EMPTIED `PATH`, so any daemon access fails loudly; reaching `Lockfile does not exist.` proves nothing touched Docker. Watched-to-fail. Also carries the correction below: the #575 "gap" was measured and is NOT real | #569 fixed |
| #578 `bb06189` | **a compose file's top-level `name:` is interpolated at last** — `name: ${CUSTOM_NAME}` reached Compose verbatim and `up` died on `invalid project name`. **The issue's design question dissolved under measurement rather than being decided:** it framed the faithful fix as "async and daemon-dependent", but `docker compose config` returns the interpolated name with `DOCKER_HOST` pointed at a nonexistent socket (exit 0) — it is a CLIENT-SIDE call, so the added dependency is the `docker` binary + compose plugin every compose flow already needs. "What if Compose is unavailable" likewise had a measured reference answer instead of a design space: the reference aborts `up` outright, so deacon does too, with **no fallback**. An authored name is now read off `docker compose config`, where the reference's `Rp` resolver has always taken it. The line-wise reader survives only as an **authorship detector** — Compose reports a `name` whether or not one was authored (its directory default), so its answer alone cannot distinguish the two, and adopting the default would overrule deacon's namespaced derivation (#265/#564); the reference hits the identical wall and solves it identically. Configs authoring no `name:` spawn no subprocess. Second commit threads `build`'s previously-inert `--env-file` — the OPPOSITE of scope creep: without it `build` and `up` would derive DIFFERENT project names from the same config, an inconsistency this fix would itself have introduced. Red-on-purpose case green with its assertion byte-identical, plus a `spec-expectation` twin on the gating lane | #572 fixed |
| #575 `8665036` | **the lockfile's `integrity` is a content pin at last** — the most serious defect the campaign has found, closed. deacon never read the field: a lockfile whose checksum did not match the Feature it named installed anyway, and deacon then OVERWROTE the bad field with the digest it had just fetched, so a tampered lockfile was silently repaired. The fix is the reference's shape and NOT the obvious one — "compare the checksum after downloading" is both wrong and weaker. `integrity` is the manifest LOOKUP KEY (`ji`: `let r = e.version; t && (r = t)`; `aQ` re-checks `docker-content-digest`), so a Feature whose content no longer matches cannot be RESOLVED at all and there is no window in which it is downloaded, installed or written back. `FeatureFetcher::fetch_feature_pinned` is that request; declared Features and auto-installed `dependsOn` targets are both pinned, keyed by the id AS WRITTEN. Two carve-outs measured rather than assumed: `--no-lockfile` means "do not READ" (`A.noLockfile ? {lockfile: undefined} : await fI(t)`), `--frozen-lockfile` does NOT. **The consequence beyond tampering, measured on both CLIs: a lockfile pins a FLOATING tag** — `git:1` recorded at 1.3.2's digest installs 1.3.2 on both CLIs though `:1` resolves to 1.3.8 today; that IS "trust on first use", and `upgrade` is how the pin moves. Reading the lockfile also made an unreadable one matter (fails pre-build now, matching the reference), but the pin read is deliberately NOT the strict `read_lockfile`: it parses strictly and validates only the field it uses, because a lockfile the REFERENCE writes for a `direct-tarball` Feature has a `resolved` that deacon's `validate_oci_reference` rejects — refusing it would be a deacon-only surprise for a migrating user. A non-digest `integrity` IS rejected (it lands in a URL path; ignoring it would make corrupting the field a way to DISABLE the check). Red-on-purpose case turned green as authored, plus a `spec-expectation` twin so the behavior GATES (the differential lane does not) | #571 fixed |
| #573 `e6f69fd` | #480 batch 3 — 4 fixtures, 4 cases, **two defects found by measuring what batch 2 had only classified**. `lockfile-oci-integrity`: deacon **never reads a lockfile's `integrity`** — a lockfile whose checksum does not match the Feature it names installs anyway, and deacon then REWRITES the field to the digest it just fetched, so a tampered lockfile is not ignored but silently repaired. The reference exits 1; `devcontainer-lockfile.md` names "trust on first use" as a goal of the file, so spec and reference agree deacon is the wrong side (rare). Transcribed, not guessed: `integrity` is the reference's manifest LOOKUP KEY (`ji` substitutes it for the tag, `aQ` re-checks `docker-content-digest`), not a checksum compared afterwards. Compose naming: `compose-with-name` and `-custom-yaml` MATCH (worth having — the existing Compose differential TOLERATES the project path, so nothing said what happens when a name IS authored), while `name: ${CUSTOM_NAME}` reaches Compose verbatim and `up` dies on `invalid project name`. Not fixed in-batch on purpose: the faithful fix reads `name` off `docker compose config`, making `derive_project_name` async and daemon-dependent, and hand-expanding `${VAR}` reimplements part of Compose's interpolation grammar — neither is "small and clearly correct". The seven codspace Feature-graph configs are now **definitively** deferred, not "worth re-running". `.gitignore`'s blanket `.env*` was silently swallowing a fixture whose `.env` IS the input under test | #571, #572 filed |
| #570 `6b9f840` | the featureless `up --frozen-lockfile` carve-out finally has a row and a case. #565 changed it from exit 1 to exit 0 incidentally; both CLIs exit 0 and neither writes a lockfile, and the reference's reason is transcribed — `--frozen-lockfile` is read only inside `writeLockfile`, whose one caller returns early when `userFeaturesToArray` finds no Features, so the flag is never REACHED rather than satisfied. Deliberately `spec-expectation`, not `live-differential`: the differential lane is nightly-only and **gates nothing**, and a case whose whole point is that nothing gated this behavior belongs where it gates. Watched-to-fail by breaking the PRODUCT (dropping the `declares_features` gate), not the case — `chan-filesystem` correctly stayed GREEN through that break, which is why the case leans on exit code + result document instead | item 1 closed; #569 filed |
| #568 `6d2b7a0` | CLAUDE.md's cross-cutting-audit bullet said `dockerComposeFile` resolves against the workspace folder; it resolves against the **config dir** (spec: "relative to the `devcontainer.json` file"; `ComposeManager::create_project` already implements and cites it). No product change — the guidance file was what was wrong, and it had already steered one fixture wrong. Contrast preserved by naming what genuinely IS workspace-relative there: the compose project name and working dir | #555 fixed |
| #566 `b49da6d` | the derived Compose project name leads with the sanitized workspace-folder stem — `deacon_site_6fb1205c_532a7bdd`, not `deacon_6fb1205c_532a7bdd`. Ruling refined #265 on the ground that deacon's audience is terminal-first: there is NO VS Code integration path (no extension; the Dev Containers extension bundles and drives the reference CLI), and the one workflow that composes — Attach to Running Container — does not provision, so it cannot collide. Isolation still holds by construction (still outside `<folder>_devcontainer`); both hashes stay, each load-bearing (`workspaceHash` separates same-named checkouts, `configHash` drives the new-generation behavior #371/#551 rest on); empty stem falls back to hash-only. A one-time `up` diagnostic names superseded projects because Compose prefixes named VOLUMES with the project name — containers are swept, volumes never are. **Net find**: the parity harness's basename-marker sweeps could not see a pre-#564 `deacon_<hash>_<hash>_default` network at all; the stem makes deacon's own compose resources reclaimable, now asserted | #564 fixed |
| #565 `334970b` | `build` consumes `--no-lockfile` / `--frozen-lockfile` at last (parsed since forever, consulted never — the source comment admitted it). Both now route with `up` through one `commands/shared/lockfile` (`LockfilePolicy::{Skip,Frozen,Write}`), so the two subcommands cannot drift; a duplicated EROFS/EACCES detector was deleted rather than a fourth copy added. Frozen refuses PRE-build, so nothing is built and nothing written | #556 fixed |
| #563 `40d6fb6` | `up --frozen-lockfile` compares SEMANTICALLY and deacon emits the spec key order (`version, resolved, integrity`). deacon rejected every lockfile the reference writes — same 327 bytes, alphabetised keys. Both halves shipped together on purpose: the order change rewrites existing lockfiles once, and the tolerant comparison has to already exist when it does. Three duplicated `sort_json_object` copies collapsed into one `canonical_lockfile_value`; feature ids stay sorted (transcribed from `generateLockfile`, confirmed black-box), only entry keys stopped being | #557 fixed |
| #561 `6d82428` | OCI feature cache entries published ATOMICALLY: extract to a `.staging-<pid>-<n>-<nanos>` sibling under `cache_dir`, then `rename` into place, so a destination is only ever absent or complete. A cache hit is now the MARKER FILE (`devcontainer-feature.json` / `devcontainer-template.json`), not a bare directory, which self-heals partial entries; a stale incomplete tree is vacated by a SECOND rename, never `remove_dir_all` in place (deleting live re-opens the very window). Two symptoms, both reproduced: a reader mid-unpack (`Feature metadata file not found`, the CI error) and two writers colliding (`failed to unpack …`). Found because #558's new registry lane CONCENTRATED five same-feature fetches at concurrency 4 — lane concurrency deliberately left at 4, since lowering it would hide a user-facing defect behind a test-only setting. Cold-cache lane: 14/14 green vs **2/9 red on `main`**; a warm cache hides it entirely, which is why it read as unreproducible | #560 fixed |
| #559 `e9662ee` | #480 batch 2 — the reference's lockfile fixtures mined: 13 configs triaged, **3 landed as 5 cases, 10 deferred**. The deciding test is not "codspace = defer" but whether the third-party Feature's IDENTITY is incidental to the claim (repinning is faithful for lockfile mechanics, fatal for cases asserting generated bytes / a publisher's `dependsOn` graph / a tarball hash). Found TWO real defects → #556, #557. Also caught in its own data: a `resourceGroup: none` case ran against the committed fixture dir and left a stray `.devcontainer-lock.json` in `parity/fixtures/` (the #423 untracked-copy failure) → moved to `fs-heavy`. Fixed `prepull-fixture-images.sh`'s `find -name 'devcontainer.json'` never matching the root-dot form | #556, #557 filed |
| #558 `fd4dd63` | registry axis: `needsRegistry` on the record + `Lane::Registry` + `parity_registry`, resolving in scarcity order (oracle → daemon → registry) so a Docker case that also fetches stays in `parity_docker`. NOT a `ResourceGroup` variant — a group says what a case CONTENDS for and cannot say "reaches ghcr.io". Case-data diff is exactly six `"needsRegistry": true` lines; #411's `git:1.3.2` → `1.3.8` pin byte-identical (re-laning is not a coverage change). The lane's no-network promise is now ENFORCED by a namespace guard that re-execs the binary under `unshare --user --net` — deliberately WITHOUT `--map-root-user`, which Ubuntu 24.04 denies (`apparmor_restrict_unprivileged_userns`), measured on the very runner where the lane gates | #544 fixed |
| #554 `b745a47` | cross-shape supersede: the compose-project expansion was gated on the CURRENT `up` being compose, so a document that changed SHAPE (compose → plain `image`) stopped the superseded project's labelled primary and stranded its `depends_on` sidecars, holding the network referenced. Candidates are now inspected for a project on every path — the expansion is driven by what the CANDIDATES are, never by what this `up` is | #551 fixed |
| #550 `8dbeb2c` | `up` stops (never removes, per the maintainer ruling) every superseded container for the workspace when a changed config forces a new one — daemon-side label query (`devcontainer.source` ∧ `devcontainer.local_folder`), current excluded by id+project, superseded compose PROJECTS expanded so label-less sidecars stop too; not gated on `!reused` (edit-and-edit-back measured). Reattach-on-return verified — the recovery the stop ruling buys. New `deacon extension` row; `case-up-stale-config-reentry` asserts `{total: 2, running: 1}` (both numbers load-bearing). Review follow-up: cross-shape compose→single sidecar stranding → #551 | #371 fixed, #551 filed |
| #548 `d063073` | hermetic lane portable + ungated from Linux: doctor `host_os` pinned-runner-OS fix (shape assertion + a deacon-decided replacement claim), `parity/fixtures/** -text` (renormalize: zero diff), `path_spellings` registers as-given/canonical/verbatim-stripped → one token (also fixes latent macOS `/var`), mode predicate kept exercised by unit test. Hermetic binary PROVEN executed on macOS+Windows (per-group timings). Guardrail held: no per-platform expectation machinery | #441 fixed |
| #549 `75da7b7` | `build`'s feature install order pinned via the `devcontainer.metadata` label: whole-string label pin (positional by construction, sidesteps `jsonSubset` array order-insensitivity); spec-expectation because the serialization tolerance is path-scoped and would blank the order claim. Measured both CLIs: same four entries, same order | #536 fixed |
| #545 `05d9282` + #546 `4582692` | `up --skip-post-create` defers ALL five lifecycle phases + dotfiles, matching the reference (spec-silent flag → reference is authority; transcribed `postCreateEnabled: !skipPostCreate` gating the whole runner, measured full matrix at 0.87.0). One exhaustive-match classifier; two new parity cases (one-op `absent` + a run-user-commands-resume differential; filesystem channels capture once, after ALL ops — recorded in the SPEC_STATUS row). #546 is the compensating-review catch: `Initialize` was misclassified as deferred; the reference runs initializeCommand under the flag (its executor is outside the gate) — inert but corrected, with a pinned negative | #476 fixed |
| #543 `e4e50f4` | the hermetic lane's one merged-config case is actually hermetic: local feature replaces the ghcr fetch, `--docker-path /nonexistent/…` turns the base-image label pull into a no-runtime degradation assertion. Watched-to-fail via `unshare -rn` reproducing the exact CI flake; 165 ms, byte-identical with and without network. The no-network run exposed six MORE registry-reaching hermetic cases → #544 (lane-contract ruling needed) | #454 fixed, #544 filed |
| #542 `5be7282` | 37 remaining test `NamedTempFile::new()` sites moved onto test-owned TempDirs (Windows delete-pending flake class); production fallible sites untouched; per-shape faithfulness breaks; zero dropped-TempDir patterns | #540 fixed |
| #538 `dabf102` | `set-up` reports `mergedConfiguration.customizations` as per-tool ARRAYS (one slot per contributing metadata entry, `[…label fragments, --config]`, upstream `Tt`), via the new `metadata_customizations_layers` carrier (the #477 pattern) routed through the existing `apply_customizations_shape`; layers are variable-substituted because the reference maps `substitute` over label entries (`Tr`→`IG`, confirmed live) | #532 fixed |
| #537 `aa8258f` | `up` Feature install ORDER is observable at last: three multi-Feature cases read the sequence three local `install.sh` scripts append inside the created container — one order-by-declaration (`overrideFeatureInstallOrder`), one order-by-dependency (`installsAfter` + `dependsOn`), one differential. Measured at 0.87.0: no divergence, the hole was in the coverage and not in the behavior | #417 fixed, #536 filed |
| #535 `bf338f7` | `port_forward` registry tests deterministic: the flake was the test helper's drop-then-rebind of an ephemeral port (measured 1/300 under pressure), NOT a product TOCTOU (`TcpListener::bind` is the atomic take and the allocator holds its listener). Bind probe injected via private `allocate_with` seam; assertions strengthened, `free_port()` deleted from all five tests | #482 fixed |
| #531 `ba8368a` | exec/run-user-commands read `devcontainer.metadata` from the CONTAINER inspect; the identity labels pick between two compositions (complete-record vs layered), transcribed from the reference bundle | #527 fixed |
| #530 `56494de` | `run-user-commands` and `set-up` stamp `config_hash` in lifecycle markers via the shared `canonical_reconnect_identity` contract (hash the config AS LOADED, before mutation) | #372 fixed |
| #533 `32be9ae` | `set-up` folds only the reference's enumerated metadata property list (upstream `pickConfigProperties` ∪ `entrypoint`, 25 names); label-authored `workspaceFolder` no longer becomes the hook CWD | #526 fixed, #475 refuted/closed |
| #529 `2376a54` | feature-fetch timeout 2s → FR-023's 30s + one retry (`FEATURE_FETCH_TIMEOUT`, `feature_fetch_retry_config()` in `crates/core/src/oci/`) | #525 fixed |
| #528, #522, #521, #518… | ledger corrections, census guard, doctor bounded probes, perf-test calibration | — |

Adjudication #524 is the model case for the stop-condition machinery: its premise
("deacon reads the container label, the reference reads the image's") was refuted
by measurement — both read the same merged `Config.Labels` — and the two real
defects it concealed were filed as #526 and #527, both now fixed.

## Work queue (characterized, none delegated)

Sharpest first. Every item below already has a measurement or a precise claim in
its issue — read the issue before briefing an agent.

**The queue is empty of filed work.** The 2026-08-11 queue was consumed entirely
(#555, the featureless-lockfile row, #480 batch 3); the batch that replaced it —
#571, #572, #569 — is now closed too (#575, #578, #577). Every filed issue the
campaign has characterized is fixed. What follows is UNFILED: three measured
findings, one measured non-finding, and two maintainer decisions.

**[#580](https://github.com/get2knowio/deacon/issues/580) — with no authored `name:`,
deacon resolves `COMPOSE_PROJECT_NAME` from only one of the sources Compose uses.**
Filed 2026-08-13. On the derivation path deacon looks in exactly one place, a `.env`
in the workspace folder, so two sources the reference honors are invisible: the
PROCESS ENVIRONMENT (`COMPOSE_PROJECT_NAME=env-wins deacon up` derives
`deacon_m3_…` where the reference lands `env-wins`) and a `.env` BESIDE THE COMPOSE
FILE (`.devcontainer/.env` → reference `from-configdir`, deacon derives). Both have
one cause — `derive_project_name` never consults Compose when nothing is authored —
so it is one issue. The authored-name path already AGREES on both, because #578
routes it through `docker compose config` and Compose applies the variable itself.

**This entry replaces three findings the #572 agent reported, and the correction is
the point.** Verified before filing, per the standing rule; of the three, **one
survived and two were refuted**:

- *"deacon ignores the `COMPOSE_PROJECT_NAME` process env var"* — TRUE only on the
  derivation path. With a name authored, deacon honors it exactly like the reference.
  The agent's own fix closed the broader claim and it did not re-measure afterwards.
- *"a LITERAL authored name is not normalized — `name: My_Project` fails on deacon"*
  — REFUTED. Both CLIs exit 0 with `my_project`. #578 routes every authored name
  through `docker compose config`, so literals get normalization too; the predicted
  asymmetry never existed.
- *"the reference reads `.env` from the CLI's cwd"* — REFUTED, and this one also
  corrects #572's transcription of the `Rp` resolver. With `.env` in the cwd and
  nowhere else, the reference fell through to its step-4 default. The locations it
  was observed honoring are the workspace folder and the compose file's directory.

**The generalizable lesson: an agent that fixes a defect is the worst-placed
observer of what its fix left behind.** Two of the three claims were true of the
code the agent started from and false of the code it shipped. Re-measure adjacent
findings against the MERGED result, not against the branch point.

**A measured NON-finding, recorded so it is not rediscovered.** The gap #575 left
open — that `read-configuration` / `run-user-commands` resolve Features unpinned by
the lockfile — was measured on 2026-08-13 and **is not a defect**. The premise was
the unverified part and it was false: the reference does NOT thread its lockfile
through `generateFeaturesConfig` for every subcommand. `read-configuration` goes
through a dedicated helper that hard-codes `noLockfile: !0`, and
`run-user-commands` builds no features config at all (it reads the container's
`devcontainer.metadata` label, which deacon already mirrors via #527). Measured with
`git:1` pinned at 1.3.2's digest against a live `:1` of 1.3.8: `build` stays pinned
on both CLIs, `read-configuration` reports 1.3.8 on BOTH. Sharpest form — on the
deliberately-corrupt-`integrity` fixture where `build` exits 1 on both sides,
`read-configuration` exits 0 on both. **Pinning that reader would CREATE a
divergence.** `read-configuration`'s own richer variant was checked separately, also
unpinned, also matching. The stale doc comment at `feature_resolver.rs` is corrected.

**One more difference, pre-existing and NOT a pin problem** (found while measuring
the above, unfiled): when a container lacks the `devcontainer.local_folder` label,
`run-user-commands` resolves declared Features from the registry while the reference
resolves none. That is a SUPERSET rather than a mismatch, it is #527-characterized,
and pinning it by the lockfile would move deacon AWAY from the reference. Wants a
decision, not a fix.

**#480's first vein is close to exhausted.** All 34 remaining configs are deferred
for content or environment reasons, and the seven codspace Feature-graph configs are
now definitively out rather than "worth re-running". Only two unlocks remain and
**both are maintainer decisions**, not work: (a) a "materialise this file empty"
fixture step, wanted or not; (b) an in-place-config-rewrite fixture pattern for
`upgrade --feature`, the one surface in the suite where the CLI edits the user's
config. If neither is wanted, the issue's SECOND vein — real-world
`devcontainer.json` files from public repos — is where the next batch's value is.

Open question, not blocking: #566 implements the superseded-project diagnostic as
**one message per `up`** listing every superseded project, rather than a persisted
suppression marker — the condition is self-clearing, and persisting risks the one
emission landing in a run nobody read. A small follow-up if persisted suppression
was intended.

Hygiene item CLOSED 2026-08-13: `.claude/worktrees/` is now in `.gitignore`. It had
been noticed on 2026-08-12 and carried unlanded; two agents ran in worktrees the next
day, which made the "an agent doing a broad `git add` commits an entire second copy of
the tree" risk live rather than theoretical.

Retired without work: **#402** closed obsolete 2026-08-07 — its subject (the
discovery findings queue) was deleted with the conformance crate, and the parity
nightly's fresh-enumeration model cannot develop the stale-queue failure mode.

Unmeasured probe candidate (not yet filed): `overrideFeatureInstallOrder`'s
metadata-id alias surface, noted during #505.

## How the campaign is run (the working model)

The maintainer names issues; the session spawns Opus subagents in isolated git
worktrees with guardrail briefs; agents work end-to-end (branch → measure → fix →
parity case → PR → watch required checks → squash-merge → API-verified); the main
session verifies every landing with a compensating diff review. Standing rules,
each learned from a real failure:

- **Agents verify their worktree base** against latest `origin/main` before any
  work (worktrees can branch from a stale main).
- **Never `git stash`** — the stash stack is repo-global across worktrees; a
  stash/pop in one agent corrupts a sibling's checkout (this happened; recovery
  cost a session).
- **Measure before code.** Reproduce both CLIs at the pinned oracle; when
  black-box is ambiguous, grep the oracle's bundled JS
  (`dist/spec-node/devContainersSpecCLI.js`). Transcribe reference behavior, don't
  invent it — #531's discriminator and #533's property list were both read
  directly out of the bundle.
- **STOP conditions in every brief**: if measurement refutes the issue's premise,
  or the fix needs a maintainer ruling (allowlist entry, design choice between
  invariants), the agent posts its measurement packet to the issue and stops.
  Rulings are the maintainer's, made individually. #524 (premise refuted) and
  #527 (composition ruling: id-label discriminator) both exercised this.
- **Watched-to-fail**: every new test and parity case is broken once and observed
  failing before it counts.
- **Re-measure an agent's ADJACENT findings against the merged result before filing
  them.** CLAUDE.md already says to verify agent claims empirically; 2026-08-13 gave
  the reason a second time and named the mechanism. Three side-findings from the
  #572 work were checked before filing and only ONE survived — the other two were
  true of the code the agent branched from and false of the code it shipped, because
  its own fix had closed them. An agent reports what it noticed while working; it
  does not re-run those observations after its patch lands. One of the refuted
  claims also falsified a line in #572's transcription of the reference resolver,
  so a stale side-finding can poison a *different* issue's cause section too.
- **Ledger honesty**: every measured deacon-vs-reference difference gets a GitHub
  issue and a `SPEC_STATUS.md` row the day it is found; rows and header census
  move in the same commit.
- **Merges are API-verified** (`gh pr view --json state,mergeCommit`), never
  trusted from an agent's report. If `--delete-branch` trips on a worktree-held
  ref, delete via `gh api -X DELETE repos/get2knowio/deacon/git/refs/heads/<branch>`.
- **Check-watching**: required checks register LATE (the test matrix, Podman and
  MVP-integration appear minutes in — 5 → 8 → 10 in one observed run). A watcher
  must require TWO consecutive all-green polls of `gh pr checks --required`
  before concluding green.
- **A green PR is NOT a green `main`** — watch the post-merge run too. #558 passed
  `Test (Podman)` on its own PR and FAILED the identical job on `main` minutes
  later, because the bug it exposed (#560) was nondeterministic. Against a
  nondeterministic failure a single pass is weak evidence; the strong evidence is
  a repetition count (14/14 vs 2/9). Note `Test (Podman)` is simply the job that
  runs `mvp-integration` on a 2-core runner with a COLD cache — when it alone
  fails, suspect concurrency or cache state, not the runtime. Confirm the runtime
  is implicated before believing it is.
- **The required set is KNOWABLE, not guessable.** Ask the repository rather than
  counting rows: `gh api repos/get2knowio/deacon/branches/main/protection --jq
  '.required_status_checks.contexts'` returns the ten names verbatim. This matters
  because `gh pr checks` lists MORE than ten — `CodeQL`, `type-label`, `label`,
  `action-semantic-pull-request` and `live-certification` also appear, and
  `live-certification` (the parity workflow) is currently RED by design. A watcher
  that counts "all checks" therefore never goes green, and one that counts to ten
  without naming them can count the wrong ten. #575 was merged on the named set with
  `mergeStateStatus: UNSTABLE`, which is exactly what MERGEABLE-with-a-failing-
  non-required-check looks like.
- **A green streak is only green against a COMPLETE check set.** The required set
  is **TEN**. Two consecutive all-green polls is NOT sufficient on its own: an
  agent polled `gh pr checks --required`, got eight because `Test (MVP integration)`
  and `Test (Podman)` had not registered yet, and counted a streak toward merging —
  8-of-10 read as all-of-8. Its own words: *"the `n > 0` guard I wrote is not a
  completeness check."* The predicate is `total >= 10 AND all pass`, twice
  consecutively. It nearly merged without the two jobs that matter most.
- **One watcher per PR, and it belongs to the main session.** An agent that arms
  its own check monitor wakes every few minutes to report partial counts,
  duplicating the session's watcher and burning its context for nothing — one did
  this five times on #570 while holding (correctly) at 8-of-10. Brief agents to
  stop at "PR open, gate run locally, here is my report", and let the session watch
  and merge. Their holding discipline is still the thing being tested; what is not
  wanted is the narration. **Observed again 2026-08-13**, five more times, on #578:
  the agent held correctly at every partial count and re-emitted its entire final
  report on each stop. The brief said "report back"; it did not say "report once".
  Say so explicitly.
- **Do not trust a squash message's arithmetic after a rebase.** #573's commit body
  says "128 → 131" because it was written before rebasing onto #570; the FILE says
  132, and the file is what `check_spec_status_census` validates. Recount census
  rows after any rebase rather than carrying header arithmetic forward.
- **Git will merge the census block CLEANLY and WRONGLY.** Rebasing #577 onto #578,
  both of which had incremented "conformant and matching" by one for UNRELATED
  reasons (#578 reclassified a row, #577 added one), git took the total from one
  side and the matching count from the other and produced 135/0/9/11/20/**94** — a
  set that sums to 134. No conflict marker, because the two edits touched different
  LINES of the same list. The census test catches it, which is the argument for
  running `cargo nextest run -p parity-harness` after any rebase that touches
  `SPEC_STATUS.md` rather than eyeballing the block. Counts are arithmetic, not
  additive text: reconcile them by re-deriving from the rows, never by accepting
  either side.
- **A stale claim OUTSIDE the conflict region merges silently, and that is worse
  than a conflict.** The same rebase left the featureless-lockfile row still saying
  #569 was "measured, classification pending" and still describing deacon's gate as
  reading the configuration alone — both falsified by the very commit being rebased,
  neither flagged, because the text sat far from the changed rows. After a rebase,
  grep the ledger for the issue numbers the change closes and read every hit. A
  status is a claim; a merge that preserves a false one has done real damage.
- **Merge ORDER is a decision when one PR fixes the check that makes another look
  broken.** #577 sat at `mergeStateStatus: UNSTABLE` purely because
  `live-certification` was red on it — and #578 was the fix for that lane. Merging
  #578 first meant #577's rebase inherited a green lane and neither merge needed a
  protection bypass. Check for this relationship before reaching for `--admin`.
- **An issue's DESIGN QUESTION is a claim with the same standing as any other.**
  #572 was filed rather than fixed because the faithful path was judged "async and
  daemon-dependent"; one command dissolved it —
  `DOCKER_HOST=unix:///nonexistent/docker.sock docker compose config` exits 0,
  because it is a client-side call. The adjacent "what if the tool is unavailable"
  was not a design space either: the reference has a measured answer (it aborts).
  Reasoning about the COST of a fix goes stale exactly like reasoning about
  behavior, and gets the same treatment — measure it before accepting the trade-off
  that keeps an issue open.
- **A red check is a claim too — find out WHOSE before re-running or reworking.**
  #575's `Security (cargo deny)` failed with
  `fatal: unable to access 'https://github.com/RustSec/advisory-db/': server
  certificate verification failed` — a runner TLS failure fetching the advisory DB,
  not a finding. Two cheap discriminators settled it before any re-run:
  `git diff main -- '*Cargo*'` was empty, and `cargo deny check` locally reported
  `advisories ok, bans ok, licenses ok, sources ok`. Read the failing STEP's log
  (`gh api repos/.../actions/jobs/<id>/logs --allow-escape-sequences`) rather than
  the summary; a job cannot be re-run while its parent run is still in progress, so
  wait for `status=completed` then `gh run rerun <run> --failed`.
- **Rebuild before you measure.** `./target/debug/deacon` is not evidence of what
  is on `main`. Twice in one session a measurement against a stale binary produced
  a confidently wrong conclusion — once saying a landed fix was absent, once
  showing the old Compose project name. `cargo build` after every pull, and note
  the commit you built at when you report a measurement.
- **Self-merge on green**: agents may squash-merge once required checks are
  confirmed green. Every such merge is disclosed to the maintainer with a
  compensating pre- or post-merge diff review by the main session. The offer to
  switch to stop-at-green-PR (human merges) stands.
- **PR titles**: allowed Conventional-Commit types only — never `test`/`style`
  (use `chore` for test-only PRs). The squash merge uses the PR title.
- **Docker etiquette**: never global-prune. Reclaim by the documented rule only:
  containers whose `devcontainer.local_folder` label names a directory that no
  longer exists, plus `deacon-*`/`deacon_*` compose orphans AND their networks
  (leaked networks accumulate toward address-pool exhaustion). A day of
  Docker-gated agent runs leaked ~117 containers + 14 networks (a later one
  161 + 24); sweep at session end — last sweep 2026-08-07 left 0/0.
- **Gate for metadata/lifecycle-path changes**: the standard pre-push set
  (fmt --check, clippy `--workspace --all-targets --all-features`, build
  all-targets, `dev-fast`, live `--profile parity`) does NOT select
  `parity_docker` — run it explicitly
  (`cargo nextest run -E 'binary(=parity_docker)'`). Its absence let a
  hook-doubling regression reach CI once.

## Key seams touched recently (for orientation, not authority)

- `crates/core/src/config.rs` — `metadata_customizations_layers` carrier
  (`#[serde(skip)]`, concatenated in `merge_two_configs`, substituted in both
  passes, #532); its lifecycle sibling `metadata_lifecycle_layers` (#477).
- `crates/core/src/port_forward/registry.rs` — `allocate_with` injected bind
  probe (test seam, production byte-identical, #482).
- `crates/deacon/src/commands/shared/container_metadata.rs` —
  `resolve_config_against_container` + `MetadataComposition`
  (complete-record vs layered, #527); per-fragment customizations capture (#532).
- `crates/deacon/src/commands/set_up.rs` — `METADATA_MERGE_PROPERTIES` /
  `restrict_to_metadata_properties` (#526), marker `config_hash` stamping (#372).
- `crates/deacon/src/commands/shared/identity.rs` —
  `canonical_reconnect_identity`; the hash-equality invariant test.
- `crates/core/src/oci/fetcher.rs` — `fetch_feature_pinned` / `manifest_digest`'s
  `pin` argument: the lockfile's `integrity` REPLACES the tag in the manifest URL
  and is re-checked against `docker-content-digest` (#571). `FEATURE_FETCH_TIMEOUT`
  (FR-023) lives here too.
- `crates/core/src/lockfile.rs` — `LockfilePins` / `read_lockfile_pins`: the LOOSE
  read, deliberately distinct from `read_lockfile`'s strict, fully-modeled one.
  Adding validation here fails builds the reference completes.
- `crates/deacon/src/commands/shared/lockfile.rs` — `resolve_lockfile_pins` and
  `ensure_lockfile_usable` (renamed from `ensure_frozen_lockfile_usable` when the
  read stopped being frozen-only); `LockfilePolicy` is threaded to
  `up::features_build::resolve_and_stage_features` through all four feature-build
  call sites.
- `crates/core/src/compose.rs` — `ComposeCommand::extract_project_name` (shells
  `docker compose config`) + the pure `parse_project_name_from_config`;
  `derive_project_name` and `create_project` are now **async** and take the env
  files, threaded as a parameter rather than assigned afterwards so `up`/`exec`/
  `down`/`build` cannot land on different names (#572). The line-wise reader is
  retained ONLY as an authorship detector — deleting it would let Compose's
  directory default overrule the namespaced derivation (#265/#564).
- `crates/deacon/src/commands/up/mod.rs` — the `--additional-features` merge runs
  BEFORE `ensure_lockfile_usable` (#569). The ordering is load-bearing and carries
  a comment saying so; it also sits after `check_for_disallowed_features` and after
  the `identity_config` snapshot deliberately, so neither that check's scope nor
  container identity moves.
- `crates/parity-harness/src/registry.rs` — `check_spec_status_census`.

The authoritative current state is always `parity/SPEC_STATUS.md` plus the latest
nightly — this file is orientation, and goes stale the moment it isn't updated
with the queue it describes.
