# Phase 0 Research: Continuous Conformance Operation & Release Certification

**Feature**: `026-continuous-conformance-certification`
**Date**: 2026-07-28

All Technical Context unknowns are resolved below. No `NEEDS CLARIFICATION` markers remain — the five
scope-level ambiguities were settled in the spec's Clarifications session; these ten decisions settle the
design-level ones.

---

## D1. Lane records live in a new root, `conformance/lanes/`

**Decision**: Lane definitions are hand-authored strict-JSON in a new root `conformance/lanes/lanes.json`,
a sibling of `conformance/registry/` and `conformance/discovery/`. No registry loader path reaches it.

**Rationale**: A lane is operational configuration — *which checks run where* — not a claim about deacon's
conformance. Placing it inside `registry/` would put it on a path reachable by `certify`, meaning an edit to
CI configuration could change a release verdict. That is the same failure mode 025 designed the discovery
root to avoid, and the reasoning transfers unchanged. The sibling-root pattern is established and its
isolation is testable by source scan.

**Alternatives considered**:
- *Extend `fixtures/parity-corpus/registry.json`.* Rejected: that file registers live parity binaries and is
  a legacy location the 023 migration is retiring. Growing it would move in the wrong direction, and it has
  no notion of hermetic units at all.
- *Encode lanes only in workflow YAML.* Rejected: FR-002 requires a machine-readable inclusion rule that
  validation can reconcile against the unit denominator. YAML spread across six workflow files cannot be
  checked for exhaustive assignment.
- *Put lanes in `registry/` but exclude them from `certify` by convention.* Rejected: convention is exactly
  what FR-017a and FR-020 forbid relying on.

---

## D2. The unit denominator is derived, never authored

**Decision**: `lane.rs` derives the set of execution units mechanically from four existing sources:
validation classes from the `Violation` enumeration; declarative cases from `Registry::cases`; test programs
by scanning `crates/deacon/tests/*.rs` for `#[test]` functions and binary names; and snapshot replay targets
from the `conformance/snapshots/<os-arch>/<case-id>/` tree. A lane record then *references* unit ids; it
cannot introduce one.

**Rationale**: This is the anti-gaming property 023's baseline was built around, and the reasoning is
identical. If the denominator were hand-authored, a new unit could be omitted from the list and would then
satisfy "every unit is assigned to a lane" while being covered by nothing — inverting the check into a
rubber stamp. Deriving membership means the only way to make a unit disappear is to delete the unit.

**Alternatives considered**:
- *Hand-authored unit inventory with a drift check.* Rejected: a drift check compares the list to itself at
  a prior revision, not to reality, so a unit that was never listed stays invisible forever.
- *Derive only cases, hand-author the rest.* Rejected: test programs are precisely where a new binary gets
  forgotten in a profile filter — the failure the parity profile has documented twice.

**Reuse**: `baseline.rs` already scans test programs for `#[test]` functions; that scanner is the model to
follow, and where possible the implementation, rather than a second one.

---

## D3. The execution manifest is the hermetic/live seam

**Decision**: The container-backed lane emits `target/conformance/execution-manifest.json` recording the
revision under test and, per case, the case id, case hash, fixture hash, outcome, and environment identity.
`certify` reads it as ordinary committed-shape data. In CI it moves between jobs as a build artifact.

**Rationale**: Clarification Q2 established that certification is hermetic while FR-041(h) requires missing
container execution to block a release. A hermetic process cannot observe whether Docker ran; it can verify
a receipt. Carrying the revision and per-case hashes is what makes the receipt non-forgeable in the ways
that matter: a manifest from another revision, or one whose recorded hashes no longer match the current case
definitions, is rejected rather than accepted as evidence.

**Alternatives considered**:
- *Run Docker inside `certify`.* Rejected: puts a container engine in the release path, destroys byte
  reproducibility (FR-036), and contradicts SC-013's zero-external-dependency goal.
- *Trust CI job ordering (`needs:`).* Rejected: the gate would be unenforceable locally and would silently
  pass anywhere the workflow was edited. It also gives the maintainer no artifact to inspect.
- *Fold execution results into the snapshot tree.* Rejected: snapshots are reviewed, committed evidence; an
  execution receipt is a per-run artifact. Merging them would mean every CI run wants to write the reviewed
  tree — the exact pressure FR-055 exists to remove.

---

## D4. Certification and its report are produced by one command

**Decision**: Extend `certify` with `--report-dir <DIR>`, emitting `certification.json` and
`certification.md`. The verdict computation stays in `certify.rs`; report assembly lives in a new
`certification.rs` that consumes the `Certification` value rather than recomputing anything.

**Rationale**: The verdict and the report must not be able to disagree. Two commands reading the same
registry independently can drift — one gains a blocking condition the other does not render — and the
resulting artifact would claim certification the gate refused. Deriving the report from the verdict value
makes divergence unrepresentable.

**Alternatives considered**:
- *A separate `certification report` command parsing `certify --json`.* Rejected: adds a serialization
  round-trip whose schema can drift from the internal value, reintroducing the disagreement it was meant to
  avoid.
- *Extend the existing `report` command.* Rejected: `report` is deliberately non-gating and broad; the
  certification report is narrow and tied to a verdict. Conflating them would make it unclear which artifact
  a release consumed.

---

## D5. Snapshot staleness blocks; snapshot coverage blocks only for the certified profile

**Decision**: Three distinct outcomes, deliberately not collapsed:

| Condition | Verdict |
|---|---|
| Committed snapshot present but an evidence-determining input drifted | **Blocks** (FR-041(c) stale) |
| No committed snapshot for the profile **under certification** | **Blocks** (FR-041(h) missing evidence) |
| No committed snapshot for some *other* platform | Informational, as today |

**Rationale**: 022 made committed-snapshot coverage non-blocking on the stated ground that "a snapshot is a
reviewed artifact, not a release gate", and that reasoning is still correct for platforms nobody is
certifying. It is not correct for the platform being certified — there, an absent or drifted snapshot means
the certification claim has no evidence behind it. Reversing 022 wholesale would make every unrecorded
platform a release blocker and push maintainers toward recording snapshots to go green, which is the
blessing pressure this feature exists to remove. Scoping the block to the certified profile keeps both
properties.

**Alternatives considered**:
- *Keep all snapshot conditions non-blocking.* Rejected: FR-041(c) is explicit, and a stale snapshot is a
  claim backed by evidence that no longer applies.
- *Block on any missing snapshot anywhere.* Rejected as above — it manufactures pressure to bless.

**Note**: `snapshot::compare_staleness` already computes exactly the needed comparison and already excludes
the informational host-tool versions. No new staleness logic is required; only a new consumer.

---

## D6. Drift detection drives `git` and `npm` as subprocesses — no new dependency, no API token

**Decision**: Upstream observation uses `git` (blob-filtered partial clone, as `discovery/corpus_fetch.rs`
already does) for specification commits, schema documents, prose documents, and upstream test/changelog
changes; and `npm view @devcontainers/cli` for published reference releases. Both are bounded async
subprocesses in `parity-harness`.

**Rationale**: The precedent is established and its reasoning is recorded in `corpus_fetch.rs`: no API
token, no rate limit, and `git` is already a prerequisite of working in this repository. Adding an HTTP
client to `parity-harness` would introduce authentication, rate-limiting, and retry surface for a lane that
gates nothing. `npm` is already provisioned in the parity lane, so the second probe adds no new tooling
requirement either.

**Alternatives considered**:
- *`reqwest` against the GitHub and npm registry APIs.* Rejected: a new dependency in a dev-only crate,
  plus unauthenticated GitHub API rate limits that would make a nightly lane flaky for no benefit.
- *Shell out to `gh`.* Rejected: adds a tool that is not otherwise required and needs authentication.

---

## D7. Canary pins live in the discovery root and are policed by a D-class

**Decision**: `conformance/discovery/canary.json` holds canary pins. Their integrity is validated as **D6**
by `discovery check`, not as a V-class by `validate`.

**Rationale**: Clarification Q5 places canary pins outside the registry. The class assignment follows from
that placement rather than being a separate choice: D-classes police the discovery root and block a PR only
on queue integrity; V-classes police the registry and several feed `certify`. Making canary-pin integrity a
V-class would put canary state on a code path that reaches the release gate — precisely what FR-017a
forbids. Keeping the class boundary aligned with the root boundary is what makes the isolation checkable.

**Alternatives considered**:
- *A new registry revision record kind (`rev-canary-*`).* Rejected: `revisions.json` is loaded by `certify`,
  so a canary pin there could influence a release verdict, contradicting FR-017a and SC-016.
- *Canary pins in workflow YAML.* Rejected: FR-018 requires load-time rejection of mutable references, which
  needs a loader.

---

## D8. Two new nextest profiles, not five

**Decision**: Map the five lanes onto profiles as follows.

| Lane | Profile | Status |
|---|---|---|
| PR-Hermetic | `default` (+ `dev-fast` for the local loop) | existing; gains the new hermetic binaries |
| PR-Docker | `pr-docker` | **new** |
| Nightly stable differential | `parity` | existing; gains an identity assertion |
| Canary | `canary` | **new** |
| Release certification | `default` + the manifest from `pr-docker` | no profile of its own |

**Rationale**: A profile exists to make selection explicit, not to mirror the lane taxonomy one-for-one.
PR-Hermetic's units are exactly what `default` already selects plus three new hermetic binaries, so a new
profile would duplicate a large filter and create a second place for it to drift. Release certification runs
no test binaries at all — it validates data and reads a manifest — so it needs no profile. The two lanes that
genuinely select a distinct binary set get a profile each, with `binary(=…)` allow-lists.

**Alternatives considered**:
- *One profile per lane.* Rejected: two of the five would be near-duplicates of `default`, and each
  duplicate is a place for the filters to disagree.
- *Reuse `parity` for PR-Docker.* Rejected: `parity` requires the verified oracle by design, and FR-012
  requires PR-Docker to run without it.

**Known hazard**: the `default-filter` for the new profiles must be an explicit `binary(=…)` allow-list, never
a glob. Both the parity and discovery profiles document making the glob mistake; `conformance_docker_pinned`
would be captured by a `conformance_*` glob alongside the hermetic `conformance_replay`, silently dropping
the latter from the fast lane.

---

## D9. Lane membership for cases is derived from `oracleType`, not a new annotation

**Decision**: A case belongs to the oracle-free lanes iff its `oracleType` is `spec-expectation`,
`snapshot`, or `invariant-metamorphic`; to the live lane iff `live-differential`. Docker-vs-hermetic is
already encoded by `resourceGroup`. No new per-case field is added.

**Rationale**: The two properties that determine lane membership — "does this need a live reference?" and
"does this need a container?" — are already data on the case. Adding a `lane` field would create a second
source of truth that can contradict `oracleType`, and the contradiction would be silent: a case marked
oracle-free but typed `live-differential` would fail confusingly inside the runner rather than at load.
Deriving membership makes the inconsistency unrepresentable and keeps adding a case a pure data edit
(022's SC-001).

**Measured distribution** (204 cases, current `main`):

| | `spec-expectation` | `snapshot` | `invariant-metamorphic` | `live-differential` |
|---|---|---|---|---|
| non-Docker | 44 | 1 | 0 | 67 |
| `docker-shared` | 37 | 0 | 2 | 42 |

So PR-Hermetic replays 45 cases, PR-Docker executes 39, and the nightly stable differential retains 109.
Eleven legacy cases (no `oracleType`) stay with their existing carriers and are assigned to the nightly lane.

---

## D10. The upgrade proposal is assembled live and validated hermetically

**Decision**: `parity-harness`'s `oracle-upgrade-propose` bin produces the seven-section bundle (it needs
network, Docker, and both oracle versions); `deacon-conformance`'s `drift proposal check` validates
completeness and determinism hermetically. Neither writes a pin.

**Rationale**: The same split every other capability in this system uses. Assembly needs live access;
completeness is a property of the document and must be checkable on a PR without provisioning two oracles.
Splitting them also means the rejection path for an incomplete bundle (FR-030) runs in the fast lane.

**Determinism**: the bundle carries no timestamp and no absolute path, and records the input state it was
computed from (FR-027, spec edge case "prepared while the working tree has uncommitted registry edits"). The
existing `report`/`inventory diff` byte-stability approach applies directly.

**Alternatives considered**:
- *One command doing both.* Rejected: completeness validation would then require a live environment,
  removing it from the PR lane where an incomplete bundle should be caught.
- *Generate the bundle from committed data only.* Rejected: reference-behavior drift and newly-failing cases
  cannot be determined without running the candidate oracle.

---

## Resolved Technical Context items

| Item | Resolution |
|---|---|
| New dependencies | None. `git`/`npm` subprocesses cover network needs (D6). |
| Where network code may live | `parity-harness` only; `deacon-conformance` stays hermetic. |
| How certification proves Docker ran | Execution manifest (D3). |
| Whether snapshot handling changes | Refined, not reversed (D5). |
| Profile count | Two new (D8). |
| Case→lane mapping | Derived from `oracleType` + `resourceGroup` (D9). |
| Canary pin location and class | Discovery root, D-class D6 (D7). |

## Deferred Work

None. This feature has no phased deferrals — every functional requirement in the spec is scheduled for this
implementation. If any task is later deferred during `/speckit.tasks` or implementation, it must be recorded
here with a numbered decision and a matching entry under `## Deferred Work` in `tasks.md`, per constitution
Principle I.
