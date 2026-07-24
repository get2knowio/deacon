# Feature Specification: Declarative Conformance Runner

**Feature Branch**: `022-conformance-runner`  
**Created**: 2026-07-24  
**Status**: Draft  
**Input**: User description: "Create a feature specification for a declarative conformance runner that can execute one registered case against Deacon, the pinned reference CLI, or a stored reference snapshot and compare all relevant observable outcomes."

## Overview

The project already records *what* is conformant in the conformance registry (behaviors, dispositions, waivers, gaps) and *surfaces* deacon-vs-reference differences through the parity harness. What is missing is a single, **data-driven execution engine** that takes one registered case, runs it against a chosen oracle (deacon itself, the pinned reference CLI, or a stored snapshot), captures **every relevant observable channel**, normalizes the evidence with named rules, and produces a pass/fail verdict scoped to characterized behaviors.

Today, exercising a new observable outcome typically means writing a new Rust test function. This feature makes cases and assertions **declarative**: a case author adds a data record and fixtures, and the runner does the rest. The result is broader, cheaper conformance coverage with durable, auditable evidence.

## Clarifications

### Session 2026-07-24

- Q: How is "host filesystem effects" capture scoped? → A: A per-case declared allowlist of paths/globs (rooted at the isolated workspace or a declared output directory), compared after path-token normalization — not a full-tree diff.
- Q: Where are reference snapshots stored? → A: Committed, version-controlled under the `conformance/` area, so a refresh appears as a reviewable PR diff and replay is hermetic.
- Q: How does snapshot replay behave on a platform/arch with no matching snapshot? → A: Snapshots are keyed by platform + architecture; a case with no snapshot for the current platform/arch yields an explicit "no reference for platform" verdict — a coverage gap, distinct from a staleness failure and from a silent skip.
- Q: What does the case hash cover (i.e., what triggers staleness)? → A: Only behavior-affecting inputs (operations, argv, fixtures, oracle type); editing rationale, notes, or allowed-difference prose does not change the case hash.
- Q: What is the failure-phase vocabulary? → A: Reuse deacon's existing lifecycle/execution phase vocabulary (config-resolution, build, container-create, lifecycle hooks, exec) as the closed set, rather than a new enum.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Author and run a declarative case against a selectable oracle (Priority: P1)

A conformance author defines a case as data — its behaviors, context, inputs, operations, oracle mode, expected observables, allowed differences, and cleanup requirements — and runs it. The runner executes the case's operations against the selected target (deacon under test, the pinned reference CLI, or a stored reference snapshot) and compares all relevant observable channels, reporting a per-channel verdict. Adding this case, or adding a new assertion to it, requires **no new Rust test function**.

**Why this priority**: This is the core value. Without declarative case execution and multi-channel comparison, nothing else matters. It is the minimum viable slice: one case, one run, one comparison.

**Independent Test**: Register a single case (e.g., a `read-configuration` invocation) as data with expected CLI-process observables, run it against deacon and against the reference oracle, and confirm the runner reports a per-channel verdict — all without editing Rust source.

**Acceptance Scenarios**:

1. **Given** a registered case declaring its behaviors, context, inputs, operations, oracle mode, expected observables, allowed differences, and cleanup, **When** the author runs the case against deacon, **Then** the runner executes the operations and reports a per-channel pass/fail verdict against the expected observables.
2. **Given** the same case, **When** the author runs it in live-differential mode, **Then** the runner executes both deacon and the pinned reference CLI and reports per-channel agreement or divergence.
3. **Given** an existing case, **When** the author adds a new fixture and a new expected observable to the case data, **Then** the new assertion is exercised on the next run with no change to any Rust test function.
4. **Given** a case that references an unknown behavior or a malformed field, **When** it is loaded, **Then** the runner fails loudly with a specific, actionable error rather than silently skipping the case.

---

### User Story 2 - Record and replay provenance-tracked snapshots with staleness gating (Priority: P2)

A maintainer captures a reference snapshot for a case and later replays cases against that stored snapshot instead of invoking the live reference CLI. Every snapshot carries full provenance. When the case inputs, fixtures, or environment that produced a snapshot no longer match, replay **fails as stale** rather than passing on outdated evidence. Refreshing a snapshot is an explicit, reviewed action that never happens during an ordinary test run.

**Why this priority**: Snapshot replay makes conformance runs fast, hermetic, and runnable without Node/Docker, but only if snapshots cannot silently rot. Staleness gating and governed refresh are what make stored evidence trustworthy.

**Independent Test**: Record a snapshot for a case, replay the case and confirm it passes, then change a case input (or its fixture) and confirm the replay now fails as stale — and that an ordinary run never rewrites the snapshot.

**Acceptance Scenarios**:

1. **Given** a case with an oracle mode of "pinned snapshot", **When** it is recorded, **Then** the stored snapshot includes oracle version, source revision, case hash, fixture hash, full argv, platform, architecture, Node version, Docker version, Compose version, image digests, normalizer version, and the captured observables.
2. **Given** a recorded snapshot, **When** the case is replayed with matching provenance and inputs, **Then** the replay reproduces the recorded verdict.
3. **Given** a recorded snapshot, **When** the case hash, fixture hash, or a provenance field no longer matches the current inputs/environment, **Then** replay fails with a staleness error naming the mismatched field.
4. **Given** an ordinary conformance run (any profile), **When** it executes, **Then** no snapshot is created, overwritten, or deleted.
5. **Given** an explicit, reviewed refresh action, **When** it runs, **Then** snapshots are regenerated and the change is surfaced for review (diff of raw and normalized evidence).

---

### User Story 3 - Normalize evidence semantically, storing raw and normalized separately (Priority: P2)

The runner persists both the raw captured evidence and a normalized copy, kept separate. Normalization uses named, field-specific canonicalization rules — never a blanket scrub. Temporary workspace and project paths are rewritten to stable tokens (not deleted). Semantic distinctions between missing, null, empty, and defaulted values are preserved unless a named rule says otherwise. Metadata labels are parsed semantically, mount sources are compared after canonical path substitution, and PATH is compared segment-wise (with executable probes where appropriate).

**Why this priority**: Comparison is only meaningful if normalization is faithful. Over-normalization (blanket ignore lists, collapsing null/empty) hides real divergences; under-normalization produces false failures from incidental paths. Named rules make every canonicalization auditable.

**Independent Test**: Capture evidence containing a temp workspace path, a null field, an empty field, a defaulted field, a metadata label, a mount source, and a PATH; run normalization and confirm the temp path became a stable token, the null/empty/default distinctions survived, and no environment variable, label, mount source, entrypoint, command, or network was blanket-removed.

**Acceptance Scenarios**:

1. **Given** raw evidence, **When** normalization runs, **Then** the raw evidence and the normalized evidence are both persisted and distinguishable.
2. **Given** evidence containing a temporary workspace or project path, **When** normalized, **Then** the path is rewritten to a stable, named token rather than removed.
3. **Given** fields that are respectively missing, null, empty, and defaulted, **When** normalized, **Then** the four states remain distinguishable unless an explicit named rule collapses them.
4. **Given** built-image metadata labels, **When** normalized, **Then** labels are parsed and compared semantically, not as opaque strings.
5. **Given** two mount graphs whose sources differ only by a temporary path, **When** compared, **Then** they compare equal after canonical path substitution.
6. **Given** two PATH values, **When** compared, **Then** the comparison is segment-wise and, where a named rule requires it, verified by probing the resolved executable — not a raw string equality that a reordering would break.
7. **Given** any normalization run, **When** it completes, **Then** it has not blanket-removed environment variables, labels, mount sources, entrypoints, commands, or networks.

---

### User Story 4 - Scope allowed differences to a behavior and a waiver identity (Priority: P3)

When deacon and the reference legitimately differ, the case author records an **allowed difference** scoped to a specific behavior, context, and observable path, with a rationale and a registry waiver or intentional-divergence identity. There are no broad, global ignore lists: a difference is tolerated only where it is characterized and only for the observable path it names.

**Why this priority**: This is what keeps the runner honest. Global ignore lists silently mask regressions; scoped, waiver-backed differences keep every tolerated divergence tied to an auditable registry record that self-invalidates when it stops reproducing.

**Independent Test**: Characterize one divergence as a scoped allowed difference tied to a waiver identity, confirm the case passes only for that behavior/observable-path, and confirm that the same divergence appearing on a different observable path or behavior still fails.

**Acceptance Scenarios**:

1. **Given** a characterized divergence, **When** an allowed difference is declared, **Then** it names a behavior, a context, an observable path, a rationale, and a registry waiver or intentional-divergence identity.
2. **Given** an allowed difference scoped to observable path A, **When** the same kind of difference appears on observable path B, **Then** the run still fails for path B.
3. **Given** an allowed difference whose backing difference no longer reproduces, **When** the case runs, **Then** the allowed difference is reported as stale (mirroring the registry's self-invalidating waiver model).
4. **Given** a case, **When** an author attempts to suppress a channel with a broad global ignore list, **Then** the runner rejects it in favor of scoped allowed differences.

---

### User Story 5 - Run Docker-backed cases in isolated workspaces with reliable cleanup (Priority: P2)

Cases that require Docker run in isolated, external temporary workspaces with collision-resistant resource names and deterministic pinned inputs. Resources (containers, images built for the case, networks, volumes, temp directories) are reliably cleaned up whether the case succeeds or fails, and each Docker case is assigned an appropriate resource group so concurrent runs do not interfere.

**Why this priority**: The most valuable conformance evidence (image config, container/network/volume/mount graph, injected process behavior, temporal lifecycle transitions) requires Docker. Without isolation and guaranteed cleanup, these cases leak resources and become flaky, which erodes trust in the whole runner.

**Independent Test**: Run a Docker-backed case to success and to a forced failure; in both outcomes confirm no residual containers, images, networks, volumes, or temp directories remain, and that concurrently running two Docker cases does not collide on resource names.

**Acceptance Scenarios**:

1. **Given** a Docker-backed case, **When** it runs, **Then** it operates in an isolated external temporary workspace, not the repository tree.
2. **Given** two Docker-backed cases running concurrently, **When** they allocate resources, **Then** their container/network/volume names do not collide.
3. **Given** a Docker-backed case that succeeds, **When** it finishes, **Then** all resources it created are removed.
4. **Given** a Docker-backed case that fails or is interrupted, **When** it terminates, **Then** cleanup still removes all resources it created.
5. **Given** a Docker-backed case, **When** it is registered, **Then** it declares the resource group that governs its concurrency.

---

### User Story 6 - Choose the oracle type per case (Priority: P3)

A case declares which oracle type governs its verdict, and the four oracle types are treated as distinct: (a) specification expectations, (b) pinned reference snapshots, (c) live differential comparison against the reference CLI, and (d) invariant or metamorphic assertions. The runner applies the semantics of the declared oracle type; the same case can be re-pointed at a different oracle type without rewriting its inputs or fixtures.

**Why this priority**: Different conformance questions need different oracles — an absolute spec expectation, a recorded reference, a live cross-check, or a relationship that must hold regardless of the exact output. Conflating them produces weak or misleading verdicts.

**Independent Test**: Take one case and evaluate it under each of the four oracle types, confirming the runner applies distinct semantics (e.g., a metamorphic assertion checks a relationship between two operations rather than a fixed expected output).

**Acceptance Scenarios**:

1. **Given** a case with oracle type "specification expectation", **When** it runs, **Then** the verdict is against declared expected observables independent of the reference CLI.
2. **Given** oracle type "pinned reference snapshot", **When** it runs, **Then** the verdict is against the stored, provenance-checked snapshot.
3. **Given** oracle type "live differential", **When** it runs, **Then** the verdict is deacon-vs-reference agreement on the normalized observables.
4. **Given** oracle type "invariant/metamorphic", **When** it runs, **Then** the verdict is whether a declared relationship holds (e.g., idempotence of re-`up`, first-create vs restart consistency) rather than a fixed output match.

---

### Edge Cases

- **Reference/Docker unavailable**: A live-differential or Docker case run where the reference CLI or Docker daemon is absent must fail loudly with a cause-specific error (no silent skip), consistent with the harness's truthful-non-selection principle.
- **Partial capture**: An operation that crashes mid-way (a failure phase) must still record whatever observables were captured and identify the phase at which it failed, rather than discarding evidence.
- **Provenance drift with unchanged inputs**: If environment provenance (oracle/Node/Docker/Compose versions, image digests) changed but case inputs did not, replay must still flag the environment mismatch as stale.
- **Non-deterministic output**: Observables that legitimately vary run-to-run (timestamps, generated IDs, temp paths) must be handled by a named normalization rule, never by deleting the field.
- **Conflicting allowed differences**: Two allowed differences claiming the same behavior/observable path with contradictory rationales must be rejected at load time.
- **Concurrent snapshot refresh**: A reviewed refresh running while ordinary runs read snapshots must not corrupt stored evidence (atomic writes).
- **Empty vs missing observable**: A channel that produced nothing (e.g., empty stderr) must be distinguishable from a channel that was not captured.

## Requirements *(mandatory)*

### Functional Requirements

#### Case model (data-driven)

- **FR-001**: The system MUST represent each case as declarative data that identifies its behaviors, context(s), inputs, operations, oracle mode, expected observables, allowed differences, and cleanup requirements.
- **FR-002**: The system MUST allow adding a new case, a new fixture, or a new assertion to an existing case without adding or modifying any Rust test function.
- **FR-003**: The system MUST validate every case at load time and fail loudly with a specific error when a case references an unknown behavior, omits a required field, or is otherwise malformed — never silently skipping it.
- **FR-004**: Each case MUST link to one or more behaviors in the conformance registry so verdicts are attributable to characterized behaviors.

#### Oracle modes and execution targets

- **FR-005**: The system MUST be able to execute one registered case against deacon (the subject under test), the pinned reference CLI, or a stored reference snapshot.
- **FR-006**: The system MUST support four distinct oracle types — specification expectation, pinned reference snapshot, live differential comparison, and invariant/metamorphic assertion — and apply the correct verdict semantics for the declared type.
- **FR-007**: The system MUST allow re-pointing a case at a different oracle type/target without changing its inputs or fixtures.
- **FR-008**: For invariant/metamorphic oracles, the system MUST evaluate a declared relationship between operations (e.g., idempotence, first-create vs restart consistency, resume) rather than a fixed expected output.

#### Observable channels

- **FR-009**: The system MUST capture and compare CLI-process observables: exit code, stdout, stderr, structured output, and the failure phase when an operation fails. The failure phase MUST be drawn from deacon's existing lifecycle/execution phase vocabulary (config-resolution, build, container-create, lifecycle hooks, exec) as a closed set, not an ad-hoc per-case string.
- **FR-010**: The system MUST capture and compare host filesystem effects produced by a case, scoped to a per-case declared allowlist of paths/globs (rooted at the isolated workspace or a declared output directory) and compared after path-token normalization. A full-tree diff MUST NOT be used.
- **FR-011**: The system MUST capture and compare built-image configuration and metadata.
- **FR-012**: The system MUST capture and compare the container, network, volume, and mount graph.
- **FR-013**: The system MUST capture and compare injected process behavior: environment, user, working directory, PATH resolution, signals, TTY, and exit propagation.
- **FR-014**: The system MUST capture and compare temporal transitions: lifecycle ordering, first-create versus restart behavior, resume, and cleanup.
- **FR-015**: Each observable channel MUST produce an independent per-channel verdict so a case reports which channels agreed and which diverged.

#### Evidence and provenance

- **FR-016**: The system MUST persist raw evidence and normalized evidence separately and keep them distinguishable. Reference snapshots MUST be stored version-controlled under the `conformance/` area (committed), so replay is hermetic and a refresh appears as a reviewable diff.
- **FR-016a**: Snapshots MUST be keyed by platform and architecture. A case with no snapshot for the current platform/architecture MUST yield an explicit "no reference for platform" verdict — a coverage gap, distinct from a staleness failure and from a silent skip.
- **FR-017**: Every recorded snapshot MUST include thirteen elements: oracle version, source revision, case hash, fixture hash, full argv (temporary paths tokenized for portability; the verbatim argv is retained in the raw evidence), platform, architecture, Node version, Docker version, Compose version, image digests, normalizer version, and the captured observables. The first twelve are stored in the snapshot's provenance record; the thirteenth — the captured observables — is stored in the sibling raw and normalized evidence (per FR-016). An informational `capturedAt` timestamp MAY also be recorded but is not one of the thirteen and is excluded from staleness (FR-020).
- **FR-018**: The system MUST distinguish an observable that was captured-but-empty from one that was not captured, and preserve that distinction in stored evidence.
- **FR-019**: The system MUST write stored evidence atomically so concurrent readers never observe a partially written snapshot.

#### Snapshot replay and refresh governance

- **FR-020**: Snapshot replay MUST fail when provenance or case inputs are stale (any recorded provenance field or the case/fixture hash no longer matches current inputs/environment), naming the mismatched field. The case hash MUST cover only behavior-affecting inputs (operations, argv, fixtures, oracle type); editing rationale, notes, or allowed-difference prose MUST NOT change the case hash or force re-recording.
- **FR-021**: Ordinary test/conformance runs MUST NOT create, overwrite, or delete any snapshot.
- **FR-022**: Refreshing snapshots MUST be an explicit, separately invoked, reviewed action that surfaces the change (a diff of raw and normalized evidence) for human review.

#### Normalization

- **FR-023**: Normalization MUST use named, field-specific canonicalization rules; a blanket/global scrub is prohibited.
- **FR-024**: Normalization MUST rewrite temporary workspace and project paths to stable, named tokens rather than deleting them.
- **FR-025**: Normalization MUST preserve distinctions between missing, null, empty, and defaulted values unless an explicit named rule collapses a specific field.
- **FR-026**: The system MUST parse metadata labels semantically for comparison rather than treating them as opaque strings.
- **FR-027**: The system MUST compare mount sources after canonical path substitution.
- **FR-028**: The system MUST compare PATH segment-wise, and MUST verify resolution via executable probes where a named rule requires it.
- **FR-029**: Normalization MUST NOT blanket-remove environment variables, labels, mount sources, entrypoints, commands, or networks.
- **FR-030**: The normalizer MUST carry a version identity that is recorded in provenance and participates in staleness detection.

#### Allowed differences (scoped)

- **FR-031**: An allowed difference MUST be scoped to a behavior, a context, an observable path, a rationale, and a registry waiver or intentional-divergence identity.
- **FR-032**: The system MUST reject broad, global ignore lists as a means of tolerating differences.
- **FR-033**: An allowed difference MUST apply only to the observable path and behavior it names; the same class of difference on another path or behavior MUST still fail.
- **FR-034**: An allowed difference whose backing difference no longer reproduces MUST be reported as stale, consistent with the registry's self-invalidating waiver model.
- **FR-035**: The system MUST reject, at load time, two allowed differences that claim the same behavior and observable path with conflicting definitions.

#### Docker case execution

- **FR-036**: Docker-backed cases MUST run in isolated external temporary workspaces (outside the repository tree).
- **FR-037**: Docker-backed cases MUST use collision-resistant resource names so concurrent runs do not interfere.
- **FR-038**: Docker-backed cases MUST use deterministic, pinned inputs (pinned images/tags, no floating `latest`).
- **FR-039**: Docker-backed cases MUST reliably clean up every resource they create (containers, case-built images, networks, volumes, temp directories) on both success and failure/interruption.
- **FR-040**: Each Docker-backed case MUST declare the resource group that governs its concurrency, so the runner schedules it safely alongside other cases.

#### Reporting and integration

- **FR-041**: The runner MUST produce a deterministic per-case, per-channel verdict report suitable for CI consumption and human review.
- **FR-042**: Verdicts MUST be attributable to the linked registry behavior(s), so a divergence maps to a characterized behavior, a scoped allowed difference, or an uncharacterized failure.
- **FR-043**: The system MUST integrate with the existing conformance registry and waiver model rather than introducing a parallel tolerance mechanism.

### Key Entities *(include if feature involves data)*

- **Case**: The declarative unit of conformance. Identifies behaviors, contexts, inputs, operations, oracle mode/type, expected observables, allowed differences, and cleanup requirements. Carries a stable case hash computed over only its behavior-affecting inputs (operations, argv, fixtures, oracle type) — rationale/notes/allowed-difference prose are excluded so annotations do not force re-recording.
- **Fixture**: The concrete inputs (config files, workspace contents, pinned image references) a case operates on. Carries a fixture hash.
- **Operation**: A single action the runner performs for a case (e.g., a CLI invocation, an inspection). Records the argv and the failure phase if it fails.
- **Observable Channel**: One of the capturable dimensions — CLI process, host filesystem, built-image config/metadata, container/network/volume/mount graph, injected process behavior, temporal transitions.
- **Evidence (Raw / Normalized)**: The captured observables in two forms — verbatim raw and rule-normalized — persisted separately.
- **Snapshot**: A stored, version-controlled (committed under `conformance/`) reference evidence set plus its full provenance (oracle version, source revision, case hash, fixture hash, full argv, platform, architecture, Node version, Docker/Compose versions, image digests, normalizer version, captured observables). Keyed by platform + architecture.
- **Provenance Record**: The metadata that makes a snapshot reproducible and staleness-checkable.
- **Normalization Rule**: A named, field-specific canonicalization (path-token rewrite, label parse, mount-source substitution, PATH segmentation/probe, null-preservation). Versioned as a set.
- **Allowed Difference**: A scoped tolerance tied to a behavior, context, observable path, rationale, and a registry waiver / intentional-divergence identity.
- **Oracle Type**: Specification expectation, pinned reference snapshot, live differential, or invariant/metamorphic.
- **Verdict**: The per-channel and per-case outcome (agree / diverge / allowed-difference / stale / error) attributable to a registry behavior.
- **Resource Group**: The concurrency classification governing how a (typically Docker-backed) case is scheduled.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A conformance author can add a new case with fixtures and assertions covering at least one observable channel with **zero** new or modified Rust test functions.
- **SC-002**: **100%** of recorded snapshots carry all thirteen required elements — the twelve identity/environment fields in the provenance record (oracle version, source revision, case hash, fixture hash, full argv [temp paths tokenized], platform, architecture, Node version, Docker version, Compose version, image digests, normalizer version) plus the thirteenth, the captured observables, in the sibling raw/normalized evidence. (`capturedAt` is informational, not counted, and excluded from staleness.)
- **SC-003**: **100%** of replays with a stale provenance field or changed case/fixture input fail as stale and name the mismatched field; **0%** pass on stale evidence.
- **SC-004**: **0** snapshots are created, overwritten, or deleted during ordinary runs; snapshot refresh occurs only via the explicit reviewed action.
- **SC-005**: All **six** observable channels (CLI process, host filesystem, built-image config/metadata, container/network/volume/mount graph, injected process behavior, temporal transitions) each have at least one passing acceptance case producing an independent verdict.
- **SC-006**: Raw and normalized evidence are stored separately for **100%** of runs and are independently retrievable.
- **SC-007**: Normalization preserves the missing/null/empty/defaulted distinction in **100%** of cases except where a named rule explicitly collapses a field; **0** unintended collapses.
- **SC-008**: **0** allowed differences are expressed as broad global ignore lists; **100%** are scoped to a behavior, context, observable path, rationale, and waiver/divergence identity.
- **SC-009**: Docker-backed cases leave **0** residual containers, case-built images, networks, volumes, or temp directories after both successful and forced-failure runs.
- **SC-010**: Two Docker-backed cases run concurrently with **0** resource-name collisions.
- **SC-011**: A recorded case replayed from its snapshot yields the same verdict as its original recording (record/replay equivalence) for **100%** of unchanged cases.
- **SC-012**: Mandatory acceptance tests exist and pass for each observable channel, raw-vs-normalized evidence, stale snapshots, canonicalization, metadata labels, mounts, PATH, null semantics, allowed-difference scoping, process failures, resource cleanup, and record/replay equivalence.

## Assumptions

- **Builds on existing machinery**: This runner extends, and integrates with, the existing conformance registry (behaviors, dispositions, waivers, gaps) and reuses the parity harness's normalization/equivalence concepts and oracle-pinning; it does not introduce a second, parallel tolerance or waiver mechanism.
- **Consumer-only scope**: Cases exercise deacon's consumer surface (`up`, `down`, `exec`, `build`, `read-configuration`, `run-user-commands`, `templates apply`, `doctor`). Feature-authoring surfaces remain out of scope.
- **Oracle pinning**: The "reference CLI" is the pinned `@devcontainers/cli` oracle at the recorded version; the "source revision" is the pinned spec/source revision. Both are recorded in provenance and verified for exactness.
- **Case data format**: Cases are expressed in the project's existing strict-JSON registry style (hand-editable, version-controlled); the exact file layout is an implementation detail for planning, not a scope decision here.
- **Selection is profile-based**: Live-differential and Docker cases run under dedicated nextest profiles/resource groups (never an env-var opt-in), and non-selection is truthful — a green fast run never implies live/Docker cases ran.
- **Fail loud, never skip silently**: Missing reference CLI, missing Docker, or a normalization failure fails the run with a cause-specific error rather than a silent skip.
- **Reviewed refresh**: Snapshot refresh is a developer/maintainer action reviewed like any other registry change (diff surfaced in a PR); CI ordinary runs never refresh. This is only possible because snapshots are committed, version-controlled under `conformance/` (per FR-016), keyed by platform + architecture.
- **Platform reality**: Docker/live-differential cases require a Docker daemon and Node; they run in CI lanes and local environments that provide them, and are excluded (by non-selection) elsewhere.

## Dependencies

- The conformance registry (`conformance/registry/`) for behaviors, dispositions, waivers, and intentional-divergence records that verdicts and allowed differences reference.
- The parity harness's oracle resolution/verification and normalization vocabulary, reused rather than reimplemented.
- A pinned reference CLI oracle and pinned spec/source revision.
- Docker (and Compose) plus Node for live-differential and Docker-backed cases.
- nextest resource groups/profiles for scheduling and truthful selection.
