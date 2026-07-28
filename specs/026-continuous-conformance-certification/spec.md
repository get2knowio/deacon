# Feature Specification: Continuous Conformance Operation & Release Certification

**Feature Branch**: `026-continuous-conformance-certification`
**Created**: 2026-07-28
**Status**: Draft
**Input**: User description: "Create a feature specification for operating the conformance system continuously through source drift detection, explicit CI lanes, and a release-grade parity certification report."

## Overview

The conformance system currently answers *"is this behavior recorded and covered?"* on demand. It does not yet answer *"is this release certified, against what, and are the sources it was certified against still current?"*

This feature turns the conformance record into a **continuously operated** system with three additions:

1. **Explicit lanes** — five named continuous-integration lanes with declared, allow-listed inclusion rules, so a green result never implies a check that did not run.
2. **Source drift detection** — automated observation of the upstream sources the record is pinned to, producing *review artifacts only*. Automation never blesses behavior, never advances a pin, never refreshes a snapshot, never changes a disposition.
3. **A release-grade certification report** — a reproducible statement of exactly what was certified (revision, oracle, spec, schemas, platform, engine, coverage, exceptions) with a scope that is explicitly non-transitive: Linux/amd64/Docker certification says nothing about Podman, macOS, or any other oracle version.

The governing principle throughout: **evidence is produced by execution, blessed by humans, and never inferred from silence.**

## Clarifications

### Session 2026-07-28

- Q: Does release certification invoke the reference oracle live, or is it fully deterministic over committed evidence? → A: Fully deterministic. Certification never installs or invokes the reference implementation; it verifies that recorded oracle identity matches the declared stable pin. Node, network, and a live reference install stay out of the release path.
- Q: If certification is hermetic, how does it prove that required container-backed execution actually happened? → A: A separate, required container-backed execution stage emits an execution manifest; certification verifies the manifest exists, covers the required case set, matches current case and fixture hashes, and was produced for the revision under certification. An absent, incomplete, or stale manifest is the *missing required execution* failure.
- Q: What defines the unit denominator that lane-integrity validation checks for full assignment? → A: Machine-derived from the same production enumeration the system already uses to discover validation classes, declarative cases, live execution programs, and snapshot replay targets — never a hand-authored list, so membership cannot be gamed by omission.
- Q: What is drift automation permitted to write? → A: Drift artifacts plus a review pull request, restricted by an enforced path allow-list covering only drift-artifact locations. Any proposed diff touching a registry record, a committed snapshot, or a pin aborts the automation rather than being committed.
- Q: Where do canary pins live relative to the conformance registry? → A: In the discovery data root, a sibling of the registry, unreachable by any registry loader path. Canary pins are never recorded in the registry's revision records.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Certify a release with an exact, honest scope (Priority: P1)

A maintainer is cutting a release. Before publishing, they need a single artifact stating precisely what conformance claim the release carries: which deacon revision, against which reference oracle version, which specification and schema revisions, on which platform, architecture and container engine, with what behavior and observable coverage, and which gaps, waivers, and intentional divergences remain open. Anyone reading the artifact later must be able to tell what was *not* certified without asking a human.

**Why this priority**: This is the headline deliverable. Everything else (lanes, drift detection) exists to make this statement trustworthy. Without it, "conformant" is an unqualified claim that overstates what was actually verified.

**Independent Test**: Run release certification on a known-good revision and inspect the produced report. Fully validated by checking that every required identity, coverage, and exception field is present and correct, that the scope statement names exactly one profile, and that re-running produces a byte-identical report.

**Acceptance Scenarios**:

1. **Given** a registry with no gaps, all applicable behaviors covered, and fresh snapshots, **When** release certification runs on the supported profile, **Then** it reports *certified* and emits a report naming the deacon revision, oracle version, spec revision, schema revisions, platform, architecture, container engine and version, Compose version, source scope, behavior count, context coverage, observable coverage, gaps, waivers, intentional divergences, and snapshot provenance.
2. **Given** a successful certification on Linux/amd64/Docker, **When** a reader consults the report for Podman conformance, **Then** the report explicitly states that certification does not extend to Podman, other architectures, other operating systems, or other oracle versions.
3. **Given** identical inputs, **When** certification runs twice on different machines, **Then** both reports are byte-identical (no timestamps, absolute paths, or environment-dependent ordering).
4. **Given** a certification run, **When** any applicable unit was excluded from execution, **Then** the report enumerates it under an explicit "not certified" section rather than omitting it.

---

### User Story 2 - Refuse to certify when the evidence is incomplete (Priority: P1)

A maintainer attempts to cut a release while a snapshot is stale, a waiver has expired, or a required container-backed case never executed because the engine was unavailable. The release must be blocked, and the block must name the specific offending records — not a count, not a generic "certification failed".

**Why this priority**: A certification report that can be produced when the evidence is incomplete is worse than no report, because it launders absence of evidence into a positive claim. The failure conditions are the load-bearing half of User Story 1.

**Independent Test**: Inject each failure condition one at a time into an otherwise-certifiable registry, run certification, confirm it fails naming the injected record; remove the condition and confirm certification is restored.

**Acceptance Scenarios**:

1. **Given** a pinned source document containing a unit with no classification, **When** certification runs, **Then** it fails, naming the unclassified unit.
2. **Given** an applicable in-profile behavior with no covering case, waiver, or gap, **When** certification runs, **Then** it fails, naming the behavior.
3. **Given** a committed snapshot whose evidence-determining inputs have drifted, **When** certification runs, **Then** it fails as *stale*, naming the snapshot and the first drifted input.
4. **Given** a declared applicable unit the runner neither executed nor explicitly accounted for, **When** certification runs, **Then** it fails as an *unknown runner omission*, naming the unit.
5. **Given** a waiver whose expiry date has passed, **When** certification runs, **Then** it fails, naming the waiver.
6. **Given** any unresolved gap record, **When** certification runs, **Then** it fails, naming the gap.
7. **Given** an oracle version recorded in a snapshot's provenance or in the execution manifest that differs from the declared stable pin, **When** certification runs, **Then** it fails as *incorrect oracle*, naming both versions.
8. **Given** an unavailable container engine, **When** the required container-backed execution stage therefore produces no execution manifest, **Then** certification fails as *missing required execution* — it does not skip, warn, or pass.
9. **Given** an execution manifest produced for a different revision, or one whose recorded case and fixture hashes no longer match the current ones, **When** certification runs, **Then** it fails as *missing required execution*, naming the mismatch — a manifest from another revision is not evidence for this one.
10. **Given** a case whose result is neither pass, fail, nor an explicitly dispositioned exclusion, **When** certification runs, **Then** it fails as a *silently skipped case*, naming the case.
11. **Given** several failure conditions present simultaneously, **When** certification runs, **Then** it reports **all** of them in one run rather than stopping at the first.
12. **Given** any flag, environment variable, or configuration setting, **When** certification runs, **Then** no such setting can downgrade a failure condition to a warning.

---

### User Story 3 - Know which lane proved what (Priority: P1)

A contributor opens a pull request. They need to know, from the checks alone, exactly which conformance evidence their change was validated against — and equally, which evidence it was *not*. A lane that ran nothing must not appear indistinguishable from a lane that ran everything.

**Why this priority**: Truthful non-selection is the property the existing harness already depends on and the one most easily lost when lanes multiply. Without it, the certification report inherits an unearned confidence from pull-request-time green checks.

**Independent Test**: For each lane, inspect its declared inclusion rule and its produced result summary; confirm every declared unit is assigned to at least one lane and that each lane's summary distinguishes "ran and passed" from "deliberately excluded".

**Acceptance Scenarios**:

1. **Given** the five declared lanes, **When** lane-integrity validation runs, **Then** every declared validation and execution unit is assigned to at least one lane, and the count of unassigned units is zero.
2. **Given** a pull request touching only source code, **When** the hermetic pull-request lane runs, **Then** it performs registry validation, inventory validation, deterministic snapshot replay, and the hermetic non-container test set, requiring no container engine, no reference oracle, and no network.
3. **Given** a pull request, **When** the container-backed pull-request lane runs, **Then** it executes deacon against pinned expected observables for the single supported certification profile and blocks the merge on any divergence not covered by a resolvable waiver or intentional-divergence identity.
4. **Given** a lane whose precondition is unavailable (missing engine, missing or mismatched oracle, unreadable data root), **When** the lane runs, **Then** it fails loudly; it never silently skips, never marks itself ignored, and never reports green.
5. **Given** any lane's result, **When** a reader inspects it, **Then** it states which units it ran and which it deliberately excluded, so a green status implies nothing about unselected checks.
6. **Given** a new validation or execution unit added to the system, **When** it is not registered in any lane's inclusion rule, **Then** lane-integrity validation fails before merge.

---

### User Story 4 - Detect upstream drift without blessing it (Priority: P2)

The upstream specification, its schemas, and the reference CLI all move. A maintainer needs to learn about a new specification commit, a schema change, a new stable reference release, a CLI-surface change, or a relevant upstream test/changelog change on a regular cadence — delivered as a *reviewable proposal*, never as a silently applied update.

**Why this priority**: Drift detection turns a snapshot-in-time record into a maintained one. It is P2 rather than P1 because the record remains correct-as-of-its-pins without it; it merely goes quietly out of date.

**Independent Test**: Point drift detection at an upstream state known to be ahead of the current pins; confirm it produces drift signals naming each changed source kind while leaving every pin, disposition, snapshot, and waiver byte-identical.

**Acceptance Scenarios**:

1. **Given** upstream specification commits newer than the pinned specification revision, **When** drift detection runs, **Then** it emits a drift signal naming the kind, the currently pinned revision, the newly observed revision, and the registry surfaces affected.
2. **Given** a changed upstream schema document, **When** drift detection runs, **Then** it emits a schema drift signal identifying the changed document and the inventory units potentially affected.
3. **Given** a newly published stable reference release, **When** drift detection runs, **Then** it emits a reference-release drift signal — and does **not** advance the stable oracle pin.
4. **Given** any detected drift, **When** automation acts on it, **Then** its only writes are to review artifacts; it does not modify a pin, a disposition, a snapshot, a waiver, a gap, or any registry record.
5. **Given** a drift-detection run that found nothing, **When** a reader inspects the result, **Then** "no drift detected" is distinguishable from "drift detection did not run".
6. **Given** any drift-detection outcome, **When** a pull request or release is evaluated, **Then** drift detection does not gate it — its status reflects whether it ran, never what it found.

---

### User Story 5 - Separate the stable reference from canary experiments (Priority: P2)

A maintainer wants early warning about upstream development revisions without letting that experimental signal contaminate the deterministic record. Canary comparisons must run against explicitly pinned development revisions, must never block anything, and must be structurally incapable of modifying stable snapshots or dispositions.

**Why this priority**: Early warning is valuable but strictly secondary to keeping the stable record clean. The isolation guarantee is what makes running canaries safe at all.

**Independent Test**: Run a canary comparison against a pinned upstream development revision; confirm it produces a separately labeled artifact, is non-blocking, and leaves the stable data tree byte-identical.

**Acceptance Scenarios**:

1. **Given** an explicitly pinned upstream development revision, **When** the canary lane runs, **Then** it compares against exactly that revision and labels its output with the canary pin.
2. **Given** a canary pin expressed as a mutable reference (branch name, moving tag, or distribution tag), **When** the pin is loaded, **Then** it is rejected — only immutable revision identifiers are accepted.
3. **Given** a canary run that surfaces differences, **When** the run completes, **Then** no committed snapshot, pin, disposition, waiver, or registry record has changed, and the run's status is non-blocking everywhere.
4. **Given** canary results, **When** the certification report is produced, **Then** canary results appear in a separate artifact and contribute nothing to the certification verdict.
5. **Given** the nightly stable differential lane, **When** the resolved reference is any version other than the declared stable pin, **Then** the lane fails as a machinery error — it does not report the difference as a divergence.
6. **Given** the nightly stable differential lane surfaces divergences, **When** a release is cut, **Then** the release is not blocked by that lane's status.

---

### User Story 6 - Propose a stable oracle upgrade as a reviewed change (Priority: P3)

A maintainer decides to move the stable reference oracle forward. They need a complete, deterministic review bundle showing everything the move changes, so acceptance is an informed human decision rather than a leap of faith.

**Why this priority**: The least frequent operation, but the highest-consequence one — an unreviewed pin advance silently redefines what "conformant" means for every prior claim.

**Independent Test**: Request an upgrade proposal from the current stable pin to a candidate version; confirm the bundle contains all seven required sections and that regenerating it from the same before/after pins reproduces it exactly.

**Acceptance Scenarios**:

1. **Given** a candidate stable oracle version, **When** an upgrade proposal is prepared, **Then** the bundle contains all seven sections: schema drift, specification drift, CLI-surface drift, reference-behavior drift, snapshot differences, newly failing cases, and affected dispositions.
2. **Given** a bundle missing any of the seven sections, **When** it is evaluated, **Then** it is rejected as incomplete; a partial bundle can never authorize an upgrade.
3. **Given** the same before/after pins, **When** the bundle is regenerated, **Then** it is byte-identical to the previous generation.
4. **Given** an accepted proposal, **When** affected snapshots are updated, **Then** they are re-recorded through the reviewed record path, with the resulting change diff serving as the review surface.
5. **Given** any automated process, **When** it attempts to advance the stable oracle pin, **Then** the attempt is impossible by construction — there is no automated write path to the pin.
6. **Given** a canary run against the candidate version, **When** it is offered as evidence for the upgrade, **Then** it is accepted only if every input was pinned by immutable identifier and the run was hermetic; otherwise it is recorded as informational only.

---

### Edge Cases

- **A lane's precondition disappears mid-run** (engine dies, oracle uninstalled): the lane fails naming the precondition. It never converts a partial run into a green result.
- **The supported profile has no committed snapshot for the running platform**: this is *no-reference-for-platform*, reported distinctly from *stale* and distinctly from a silent skip. Certification treats it as a missing-execution failure for that profile, not as coverage.
- **A unit is assigned to two lanes**: permitted. Overlap is redundancy, not a defect; only zero assignments is an error.
- **Drift detection cannot reach upstream** (network unavailable, rate limited): it fails as machinery, reports "did not run", and does not emit an empty "no drift" result.
- **Two drift signals arrive for the same source between review cycles**: the later supersedes the earlier; the review artifact reflects current upstream state, and no intermediate state is blessed.
- **An upgrade proposal is prepared while the working tree has uncommitted registry edits**: the bundle must identify the exact input state it was computed from, so a proposal computed against dirty state is recognizable as such.
- **A waiver expires between the pull-request lanes passing and the release certification running**: certification fails on the expired waiver even though every prior check was green. Time-dependent conditions are evaluated at certification time, and the report records the evaluation date used.
- **Certification is requested for an inactive profile** (e.g. Podman): certification refuses rather than reporting an empty pass, because zero applicable units is not the same as certified.
- **A case is covered only by a non-deterministic source** (corpus finding or generative campaign): it does not count toward certification coverage unless its inputs are fully pinned and hermetic.
- **All nine failure conditions fire at once**: all nine are reported in a single run, and the report is still byte-reproducible.

## Requirements *(mandatory)*

### Functional Requirements

#### Lane taxonomy and inclusion rules

- **FR-001**: The system MUST define exactly five named lanes — hermetic pull-request, container-backed pull-request, nightly stable differential, canary, and release certification — each with a declared, machine-readable inclusion rule.
- **FR-002**: Each lane's inclusion rule MUST select **programs and validation classes** by explicit allow-list. Pattern- or prefix-based selection of these is prohibited, because it can silently capture an unintended unit or silently drop a renamed one.
- **FR-002a**: **Cases** are the exception, and MUST instead be selected by a derived predicate over existing case properties rather than by an id list. A case-id allow-list would have to be edited every time a case is added, which is the failure mode FR-003a exists to prevent — a forgotten edit would leave the case selected by nothing. The predicate MUST be validated to **partition** the case space: every case matches exactly one lane's `includes`, with no overlap and no remainder, so derived selection can never silently drop a case.
- **FR-003**: Every declared validation or execution unit MUST be assigned to at least one lane. A unit with zero lane assignments MUST fail lane-integrity validation before merge.
- **FR-003a**: The unit denominator that FR-003 checks MUST be derived mechanically from the same enumeration the system already uses to discover validation classes, declarative cases, live execution programs, and snapshot replay targets. A hand-authored denominator is prohibited, because a unit omitted from such a list would satisfy full-assignment validation while being covered by nothing.
- **FR-004**: A lane MUST fail, never skip, when a declared precondition is unavailable — including a missing container engine, a missing or version-mismatched reference oracle, an unreadable data root, or an unresolvable pin.
- **FR-005**: Every lane result MUST state which units it executed and which it deliberately excluded, so that a green status implies nothing about units the lane did not select.
- **FR-006**: No lane may mark a unit as ignored, pending, or conditionally skipped. Every unit MUST reach one of: passed, failed, or explicitly excluded by the lane's declared inclusion rule.

#### Hermetic pull-request lane

- **FR-007**: The hermetic pull-request lane MUST run registry validation, inventory validation (both machine-extracted schema constraints and normative prose clauses), deterministic snapshot replay, and the hermetic non-container test set.
- **FR-008**: The hermetic pull-request lane MUST require no container engine, no reference oracle, and no network access, and MUST fail if any of those are required at run time.
- **FR-009**: The hermetic pull-request lane MUST block merge on any failure.
- **FR-010**: Snapshot replay in this lane MUST be strictly read-only. No flag, environment variable, or configuration may cause it to create, refresh, or delete a committed snapshot.

#### Container-backed pull-request lane

- **FR-011**: The container-backed pull-request lane MUST execute deacon against pinned expected observables for the single supported certification profile, and MUST block merge on any divergence not covered by a resolvable waiver or intentional-divergence identity.
- **FR-012**: This lane MUST NOT invoke the reference oracle; its expected observables are pinned committed data, making the lane deterministic and independent of upstream availability.
- **FR-013**: Every container image and external input used by this lane MUST be pinned by immutable digest or a concrete version tag; a mutable tag MUST be rejected.

#### Nightly stable differential lane

- **FR-014**: The nightly stable differential lane MUST verify that the resolved reference implementation is *exactly* the declared stable pin. Any other version MUST fail the lane as a machinery error, never be reported as a behavioral divergence.
- **FR-015**: This lane MUST report every surfaced divergence but MUST NOT gate a pull request or a release; its status reflects whether it ran, not what it found.
- **FR-016**: This lane MUST NOT write to any committed snapshot, pin, disposition, waiver, gap, or registry record.

#### Canary lane

- **FR-017**: The canary lane MUST compare against explicitly declared canary pins identifying upstream development revisions. Canary pins MUST reside in the discovery data root — a sibling of the conformance registry — and MUST NOT appear among the registry's revision records.
- **FR-017a**: No registry loader path may reach a canary pin. The isolation MUST hold structurally, so that the presence, absence, or content of any canary pin cannot alter the outcome of registry validation or release certification.
- **FR-018**: A canary pin MUST be an immutable revision identifier (a full commit identifier or an exact published version). A branch name, moving tag, or distribution tag MUST be rejected at load.
- **FR-019**: The canary lane MUST be non-blocking in every context — pull requests, nightly runs, and releases alike.
- **FR-020**: The canary lane MUST NOT modify stable snapshots, the stable pin, dispositions, waivers, gaps, or any registry record. This isolation MUST be enforced structurally, not by convention, and MUST be asserted by an automated test.
- **FR-021**: Canary results MUST be emitted to an artifact distinct from the certification report, labeled with the canary pin they were produced against, and MUST contribute nothing to any certification verdict.

#### Source drift detection

- **FR-022**: The system MUST detect and report, on a regular cadence: new upstream specification commits, changes to upstream schema documents, new stable reference releases, changes to the reference CLI surface, and relevant upstream test or changelog changes.
- **FR-023**: For each detected change, the system MUST produce a drift signal naming the drift kind, the currently pinned revision, the newly observed revision, and the registry surfaces potentially affected.
- **FR-024**: Drift automation MUST NOT bless new behavior, alter any disposition, refresh any snapshot, advance any pin, resolve any gap, or extend any waiver.
- **FR-024a**: Drift automation's permitted writes are exactly: drift artifacts, and a review pull request whose diff is confined to drift-artifact locations. The permitted set MUST be enforced as a path allow-list checked before any write is published.
- **FR-024b**: If a proposed drift diff touches any registry record, committed snapshot, or pin, the automation MUST abort and report the attempted out-of-scope path. It MUST NOT commit a partial diff with the offending paths dropped, because a silently narrowed diff misrepresents what the drift implies.
- **FR-025**: A drift-detection result of "no drift" MUST be distinguishable from "drift detection did not run".
- **FR-026**: Drift detection MUST NOT gate a pull request or a release on what it found. Only an inability to run — unreachable upstream, unresolvable pin, unwritable artifact location — is a failure.
- **FR-027**: Every drift signal MUST be traceable to the review artifact it produced, and every review artifact MUST identify the exact input state it was computed from.

#### Stable oracle pin governance

- **FR-028**: The stable oracle pin MUST NOT advance without a human-reviewed change. No automated path may write it.
- **FR-029**: A proposed stable oracle upgrade MUST produce a review bundle containing all seven sections: schema drift, specification drift, CLI-surface drift, reference-behavior drift, snapshot differences, newly failing cases, and affected dispositions.
- **FR-030**: A bundle missing any of the seven sections MUST be rejected as incomplete and MUST NOT be usable to authorize an upgrade.
- **FR-031**: The bundle MUST be deterministic: regenerating it from the same before-and-after pins MUST reproduce it byte-for-byte.
- **FR-032**: Accepting a proposal MUST require affected snapshots to be re-recorded through the reviewed record path, with the resulting change diff serving as the review surface.
- **FR-033**: Canary evidence MUST NOT support a stable upgrade decision unless every input was pinned by immutable identifier and the run was hermetic; otherwise it is recorded as informational only.

#### Release certification execution model

- **FR-033a**: Release certification MUST be fully deterministic over committed evidence. It MUST NOT install, resolve, or invoke the reference implementation, and MUST NOT require network access. Oracle correctness is established by comparing recorded oracle identity against the declared stable pin, not by running the reference.
- **FR-033b**: A separate, required container-backed execution stage MUST run the required case set for the supported profile and emit an **execution manifest** recording, per case: the case identity, the case and fixture hashes it executed against, the observed outcome, and the environment identity (platform, architecture, container engine and version, Compose version).
- **FR-033c**: The execution manifest MUST identify the revision it was produced for, so a manifest from a different revision cannot be presented as evidence for this one.
- **FR-033d**: Certification MUST verify that the manifest exists, was produced for the revision under certification, and covers the required case set with results matching current case and fixture hashes. Any of absent, incomplete, revision-mismatched, or hash-stale MUST produce the *missing required execution* failure.
- **FR-033e**: Certification MUST NOT accept a manifest as a substitute for committed-snapshot freshness, nor a fresh snapshot as a substitute for a manifest. The two are independent evidence obligations and both MUST hold.

#### Release certification report content

- **FR-034**: The certification report MUST state all of: deacon revision; reference oracle version; specification revision; schema revisions; platform; architecture; container engine and its version; Compose version; source scope; behavior count; context coverage; observable coverage; gaps; waivers; intentional divergences; and snapshot provenance.
- **FR-035**: The report MUST carry an explicit scope statement naming exactly the certified profile and stating that certification does not extend to other container engines, operating systems, architectures, or reference oracle versions. A certification produced on Linux/amd64/Docker MUST NOT be readable as certifying Podman or any other platform.
- **FR-036**: The report MUST be byte-reproducible for identical inputs: no timestamps, absolute paths, hostnames, or environment-dependent ordering.
- **FR-037**: The report MUST enumerate what was *not* certified: inactive profiles, units dispositioned as non-testable or not-applicable, and platforms with no committed reference snapshot.
- **FR-038**: Source scope MUST identify the exact pinned source surface — schema documents, normative prose documents, and the reference CLI surface — together with their classification status.
- **FR-039**: Snapshot provenance MUST identify, per snapshot, the evidence-determining inputs it was recorded against and the platform it was recorded on.
- **FR-040**: The report MUST record the evaluation date used for any time-dependent condition (such as waiver expiry), so a later reader can reproduce the verdict.

#### Release certification failure conditions

- **FR-041**: Release certification MUST fail, and the release MUST be blocked, on any of the following: (a) an unclassified source change — a pinned source unit with no disposition, or a pinned source document not covered by the inventory; (b) an applicable in-profile behavior with no covering evidence; (c) a stale snapshot; (d) an unknown runner omission — a declared applicable unit the runner neither executed nor explicitly accounted for; (e) an expired waiver; (f) an unresolved gap; (g) an incorrect oracle — a recorded oracle version, in a snapshot's provenance or in the execution manifest, differing from the declared stable pin; (h) missing required container-backed execution — the execution manifest is absent, incomplete, produced for a different revision, or stale against current case and fixture hashes; (i) a silently skipped case — a case whose result is neither pass, fail, nor an explicitly dispositioned exclusion.
- **FR-042**: Every failure MUST name the specific offending record or records. A bare count, a summary line, or an unattributed total is insufficient.
- **FR-043**: Certification MUST evaluate and report **all** failing conditions in a single run rather than stopping at the first.
- **FR-044**: No flag, environment variable, or configuration setting may downgrade any certification failure condition to a warning or an informational note.
- **FR-045**: Certification MUST refuse to certify an inactive profile rather than reporting a vacuous pass over zero applicable units.

#### Separation of non-deterministic evidence

- **FR-046**: Real-world corpus results and generative or exploratory campaign results MUST be reported in an artifact separate from the certification report and MUST NOT contribute to the certification verdict.
- **FR-047**: Such results MAY be incorporated into deterministic certification only when every input is pinned by immutable identifier and the run is hermetic; when incorporated, the report MUST record which inputs qualified and why.
- **FR-048**: A finding originating from a non-deterministic source MUST NOT create coverage or change a disposition without an explicit human promotion step.

#### Acceptance-test mandate

- **FR-049**: Automated acceptance tests MUST exist for each lane's inclusion rule, verifying both the units it selects and the units it excludes.
- **FR-050**: An automated test MUST verify stable/canary separation: a canary run leaves the stable data tree byte-identical.
- **FR-051**: An automated test MUST verify drift-artifact completeness, including rejection of a bundle missing any of the seven required sections.
- **FR-052**: Each of the nine certification failure conditions MUST have an injected positive-control test demonstrating that certification transitions from certified to not-certified when the condition is present, and back when it is removed.
- **FR-053**: An automated test MUST verify certification scope exactness — that the report names exactly one profile and does not imply any other.
- **FR-054**: An automated test MUST verify report reproducibility by generating the report twice and comparing byte-for-byte.
- **FR-055**: An automated test MUST verify that no lane other than the reviewed record path can write a committed snapshot, asserted both behaviorally and by inspecting which components hold a write path to the snapshot tree.
- **FR-056**: An automated test MUST verify that certification neither resolves nor invokes the reference implementation and does not require network access — certification MUST succeed with the reference absent and the network unavailable, given otherwise-complete committed evidence.
- **FR-057**: An automated test MUST verify each execution-manifest rejection mode independently: absent, incomplete, produced for a different revision, and stale against current case or fixture hashes.
- **FR-058**: An automated test MUST verify drift automation's path allow-list, including that a proposed diff touching a registry record, committed snapshot, or pin aborts the automation rather than being narrowed and committed.
- **FR-059**: An automated test MUST verify that the lane-integrity denominator is machine-derived: introducing a new unit without a lane assignment MUST fail validation without any hand edit to a unit list.
- **FR-060**: An automated test MUST verify canary-pin isolation: the certification verdict MUST be byte-identical with the canary pin surface populated and with it empty or absent.

### Key Entities

- **Lane**: A named continuous-integration execution context. Attributes: identifier, trigger cadence, explicit inclusion allow-list, blocking status, required environment (container engine / reference oracle / network), and whether it may write to the record.
- **Execution Unit**: The finest granularity for which a lane reports an independent outcome — a validation class, a case, a test program, or a replay target. Every unit belongs to at least one lane.
- **Drift Signal**: An observation that a pinned upstream source has moved. Attributes: drift kind (specification commit, schema change, stable reference release, CLI-surface change, upstream test/changelog change), currently pinned revision, newly observed revision, affected registry surfaces, and the review artifact it produced.
- **Upgrade Proposal**: A deterministic review bundle for advancing the stable oracle pin. Contains exactly seven sections: schema drift, specification drift, CLI-surface drift, reference-behavior drift, snapshot differences, newly failing cases, and affected dispositions. Also records the input state it was computed from.
- **Canary Pin**: An immutable identifier for an upstream development revision, used only by the canary lane. Resides in the discovery data root, a sibling of the conformance registry, and is unreachable by any registry loader path — so its presence, absence, or content cannot alter registry validation or certification.
- **Execution Manifest**: The receipt proving container-backed execution occurred. Records the revision it was produced for, and per case: case identity, case and fixture hashes executed against, observed outcome, and environment identity (platform, architecture, container engine and version, Compose version). Certification consumes it; an absent, incomplete, revision-mismatched, or hash-stale manifest is the *missing required execution* failure.
- **Certification Report**: The release-grade statement of what was certified. Carries identity fields (deacon revision, oracle version, spec revision, schema revisions), environment fields (platform, architecture, container engine and version, Compose version), scope fields (certified profile, explicit non-extension statement, source scope), coverage fields (behavior count, context coverage, observable coverage), exception fields (gaps, waivers, intentional divergences), snapshot provenance, and the evaluation date.
- **Certification Failure Condition**: One of nine enumerated, individually testable conditions that block a release. Each names its offending records.
- **Non-Deterministic Evidence**: Results from real-world corpora or generative campaigns. Reported separately by default; admissible to deterministic certification only when fully pinned and hermetic.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All five lanes have declared inclusion rules, and the count of declared execution units assigned to zero lanes is **zero**, enforced automatically before merge.
- **SC-002**: For every lane, a reader can determine from its result alone which units ran and which were excluded; no lane reports a green status that could be mistaken for coverage it did not provide.
- **SC-003**: All nine certification failure conditions have an injected positive-control test that flips the verdict from certified to not-certified — **9 of 9**, with zero conditions verified only by inspection.
- **SC-004**: The certification report contains **all sixteen** required identity, environment, scope, coverage, exception, and provenance fields; a report missing any field is rejected rather than published.
- **SC-005**: Certification run twice on identical inputs, on different machines, produces byte-identical reports **100%** of the time.
- **SC-006**: The number of automated paths capable of advancing the stable oracle pin or writing a committed snapshot, outside the reviewed record path, is **zero** — verified both behaviorally and by inspecting write-capable components.
- **SC-007**: An upgrade proposal missing any of the seven required drift sections is rejected **100%** of the time; a complete proposal regenerates byte-identically from the same pins.
- **SC-008**: A canary run produces **zero** modifications to the stable data tree, verified by byte comparison before and after.
- **SC-009**: A newly published upstream specification commit, schema change, or stable reference release is surfaced as a drift signal within one scheduled detection cycle of its publication.
- **SC-010**: A reader can determine the exact certified combination — platform, architecture, container engine, oracle version — and the exact set of combinations *not* certified, from the report alone, with no external lookup and no maintainer consultation.
- **SC-011**: No release can be published while any gap is unresolved, any applicable behavior is uncovered, any snapshot is stale, any waiver is expired, or any required container-backed execution is missing.
- **SC-012**: Non-deterministic evidence contributes **zero** units of coverage to the certification verdict unless its inputs are recorded as fully pinned and hermetic in the report.
- **SC-013**: Certification completes successfully with the reference implementation absent and the network unavailable, given otherwise-complete committed evidence — **zero** external dependencies in the release path.
- **SC-014**: All four execution-manifest rejection modes (absent, incomplete, revision-mismatched, hash-stale) block certification — **4 of 4**, each independently demonstrated.
- **SC-015**: The number of automation-authored diffs touching a registry record, committed snapshot, or pin is **zero**; every such attempt aborts and reports the out-of-scope path rather than committing a narrowed diff.
- **SC-016**: The certification verdict is byte-identical whether the canary pin surface is populated or absent, demonstrating that canary state cannot influence certification.

## Assumptions

Informed defaults adopted where the description left room for interpretation. Items resolved in the Clarifications session above are now requirements and are no longer listed here.

1. **The single supported certification profile is Linux/amd64/Docker at reference oracle 0.87.0**, matching the one active profile in the existing record. All other profiles are inactive and cannot be certified.
2. **The nightly stable differential lane and the canary lane are both outside the release path.** This preserves the existing property that a red live-comparison lane never blocks a release, while certification blocks on committed-evidence failures only.
3. **Canary pins are hand-authored.** Drift automation may *propose* a canary pin update as a review artifact; it may not apply one, for the same reason it may not advance the stable pin.
4. **"Unclassified source change" is evaluated against the pinned source surface**, not against live upstream. Upstream moving ahead produces a drift signal (non-gating); the pinned surface containing an undispositioned unit produces a certification failure (gating). Drift detection informs; certification gates.
5. **"Unknown runner omission" means the executed set does not reconcile with the declared applicable set** — the runner must account for every applicable unit as executed, failed, or explicitly excluded by disposition. An unaccounted unit is the failure.
6. **Compose version is recorded as an environment identity field only.** It is reported in the certification scope because Compose behavior can affect observables; it is not itself independently certified.
7. **Drift detection cadence is at least daily**, matching the existing nightly schedule, and its "did not run" state is derived from the absence of a completed run record rather than from an empty result.
8. **The reviewed record path is the existing snapshot refresh mechanism** — a human-invoked operation whose output diff is the review surface. This feature adds no new write path to committed evidence.
9. **Lane definitions are data, not prose.** Inclusion rules are declared in a machine-readable form so lane-integrity validation can enforce full unit assignment mechanically.
10. **Context coverage and observable coverage reuse the existing coverage model.** Context coverage is the dispositioned state of the scenario-combination obligations; observable coverage is per-channel covering-case counts against the established minimum-covering-case floor. This feature reports them; it does not redefine them.

## Out of Scope

- Certifying additional platforms, architectures, or container engines (Podman, macOS, Windows, arm64). This feature makes the *absence* of those certifications explicit; it does not produce them.
- Automatically resolving gaps, authoring waivers, or classifying newly detected source units. Drift detection surfaces the work; humans do it.
- Changing the semantics of existing validation classes, dispositions, or the coverage model. This feature operates the existing record; it does not redefine it.
- Publishing certification reports to any external service or registry. The report is a build artifact of the release process.
