# Session Handoff — Differential Parity Steady State

Last updated: 2026-08-06, `main` at `ba8368a`. This document is the cross-session
handoff for the ongoing parity/quality campaign: where the project stands, how the
work is being run, and what is queued next. When a queue item lands or a rule
changes, update this file in the same PR.

## Where things stand

- **Zero open nonconformances.** `parity/SPEC_STATUS.md` records **120 behaviors**:
  0 open nonconformance, 9 follows-spec, 11 documented choice, 19 extension,
  81 conformant-and-matching. The header census is CI-guarded
  (`check_spec_status_census` in `crates/parity-harness/src/registry.rs`) — rows and
  header must change together.
- **The differential nightly's diverging set is empty.** Current verdict:
  `gh run list --workflow=parity.yml --branch=main`. One transient false red was
  root-caused to a real product defect (a 2s fetch timeout vs FR-023's mandated
  30s) and fixed in #529 — a slow ghcr response is no longer a false divergence.
- **The pinned oracle** is `@devcontainers/cli@0.87.0`
  (`npm install -g @devcontainers/cli@0.87.0`). Docker and the oracle both work in
  this dev container — verify parity changes for real, never by reasoning alone.

## Recent landings (this campaign, newest first)

| PR | What | Issues |
|---|---|---|
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

1. **#532** — `set-up` mergedConfiguration deep-merges `customizations`; the
   reference reports **per-tool arrays**. Measured during #533's post-fix
   re-measurement (25 identical keys, one differing value). Small, well-scoped.
2. **#417** — Feature install ORDER is claimed but never verified with more than
   one Feature. Needs a two-Feature fixture whose install scripts record ordering.
3. **#476** — characterize `--skip-post-create` phase coverage: the reference
   defers everything, deacon defers postCreate onward. Measure, then either align
   or file an allowlist ruling request.
4. **#482** — `port_forward prefers_same_number_when_free` is a TOCTOU flake on
   busy runners. Test-infra fix.
5. **#454** — `case-merged-decl-extends-child` can pull an image inside the
   "needs nothing" hermetic lane. Lane-truthfulness fix.
6. **#441** — hermetic case data is Linux-pinned; lane gated to Linux pending
   portability.
7. **#371** — `up` leaves the previous container RUNNING when a changed config
   forces a new one.
8. **#402** — discovery: no-longer-reproducing is unreachable — nothing retires a
   finding.
9. **#480 batch-2** — mine the reference's remaining ~40 e2e fixtures into
   declarative parity cases (grouped by blocker in the issue).

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
  Docker-gated agent runs leaked ~117 containers + 14 networks; sweep at
  session end.
- **Gate for metadata/lifecycle-path changes**: the standard pre-push set
  (fmt --check, clippy `--workspace --all-targets --all-features`, build
  all-targets, `dev-fast`, live `--profile parity`) does NOT select
  `parity_docker` — run it explicitly
  (`cargo nextest run -E 'binary(=parity_docker)'`). Its absence let a
  hook-doubling regression reach CI once.

## Key seams touched recently (for orientation, not authority)

- `crates/deacon/src/commands/shared/container_metadata.rs` —
  `resolve_config_against_container` + `MetadataComposition`
  (complete-record vs layered, #527).
- `crates/deacon/src/commands/set_up.rs` — `METADATA_MERGE_PROPERTIES` /
  `restrict_to_metadata_properties` (#526), marker `config_hash` stamping (#372).
- `crates/deacon/src/commands/shared/identity.rs` —
  `canonical_reconnect_identity`; the hash-equality invariant test.
- `crates/core/src/oci/{fetcher,mod}.rs` — `FEATURE_FETCH_TIMEOUT` (FR-023).
- `crates/parity-harness/src/registry.rs` — `check_spec_status_census`.

The authoritative current state is always `parity/SPEC_STATUS.md` plus the latest
nightly — this file is orientation, and goes stale the moment it isn't updated
with the queue it describes.
