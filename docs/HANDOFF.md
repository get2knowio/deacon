# Session Handoff — Differential Parity Steady State

Last updated: 2026-08-07, `main` at `8dbeb2c` (v0.3.0 release cut from the commit
after it). This document is the cross-session
handoff for the ongoing parity/quality campaign: where the project stands, how the
work is being run, and what is queued next. When a queue item lands or a rule
changes, update this file in the same PR.

## Where things stand

- **Zero open nonconformances.** `parity/SPEC_STATUS.md` records **123 behaviors**:
  0 open nonconformance, 9 follows-spec, 11 documented choice, 20 extension,
  83 conformant-and-matching. The header census is CI-guarded
  (`check_spec_status_census` in `crates/parity-harness/src/registry.rs`) — rows and
  header must change together.
- **The differential nightly's diverging set is empty.** Current verdict:
  `gh run list --workflow=parity.yml --branch=main`. One transient false red was
  root-caused to a real product defect (a 2s fetch timeout vs FR-023's mandated
  30s) and fixed in #529 — a slow ghcr response is no longer a false divergence.
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

## Recent landings (this campaign, newest first)

| PR | What | Issues |
|---|---|---|
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

1. **#551** — cross-shape supersede (compose → single-container) strands compose
   sidecars: `stop_superseded_containers`' project expansion is gated on the
   CURRENT config being compose. Found in #550's compensating review; fix shape
   (drop the gate, always inspect candidates for a project) and evidence shape
   are in the issue. Strictly narrower than what #550 fixed — not a regression.
2. **#544** — six more hermetic-lane cases reach a registry (found by #454's
   no-network run); shape (b) does not extend to them (version-pinned OCI refs,
   `upgrade --dry-run` digest resolution). **Needs a maintainer ruling on the
   lane contract**: widened `parity_hermetic` promise vs a registry axis on
   `lane_of` vs cache pre-seeding — plus the proposed `unshare -rn` guard that
   would make the "no network" promise true by construction.
3. **#480 batch-2** — mine the reference's remaining ~40 e2e fixtures into
   declarative parity cases (grouped by blocker in the issue).

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
- `crates/core/src/oci/{fetcher,mod}.rs` — `FEATURE_FETCH_TIMEOUT` (FR-023).
- `crates/parity-harness/src/registry.rs` — `check_spec_status_census`.

The authoritative current state is always `parity/SPEC_STATUS.md` plus the latest
nightly — this file is orientation, and goes stale the moment it isn't updated
with the queue it describes.
