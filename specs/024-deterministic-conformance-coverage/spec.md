# Feature Specification: Deterministic Conformance Coverage

**Feature Branch**: `024-deterministic-conformance-coverage`
**Created**: 2026-07-26
**Status**: Draft
**Input**: User description: "Fill the deterministic conformance gaps after the existing parity corpus has been migrated: a constrained context model (operation; image/Dockerfile/Compose config sources; container state; Features and Feature ordering; configuration layering and extends; runtime and platform profile; structured vs human-readable output modes) with applicability rules, pairwise coverage plus explicitly selected high-risk triples, deterministic cases across the shared consumer workflow, a Docker-backed error-path tier, per-behavior disposition (case / non-testable rationale / scoped expiring waiver / visible gap), and generated coverage reports with injected-regression proofs."

## Why This Feature Exists

The preceding migration was governed by a **conservation** constraint: nothing that was
covered before could stop being covered. It succeeded, and by succeeding it froze the shape
of the coverage it inherited. That shape is lopsided in ways nothing currently reports:

- **Operations.** All 82 declarative operations recorded today are configuration read (65),
  up (13), or exec (4). Seven in-scope consumer operations — build, down, run-user-commands,
  template application, outdated, upgrade, and diagnostics — have **no** declarative case at
  all.
- **Context.** Every declarative case declares an **empty** context. The applicability
  machinery is structurally valid precisely because nothing exercises it.
- **Observables.** Four observable channels carry a single observation each, one carries
  none, while two carry more than fifty. A channel with one observation is a channel nobody
  has shown can fail.
- **Behaviors.** The behavior denominator is 27 records, 13 of them in one area. That
  denominator is what strict certification measures against, so a behavior nobody wrote down
  is not merely uncovered — it is **invisible**, and certification passes.

The record is therefore *truthful about what it claims* and *silent about what it omits*.
This feature makes the omissions visible, bounds them with a constrained context model, and
fills them with deterministic evidence — without enumerating a combinatorial explosion
nobody can run or maintain.

## Clarifications

### Session 2026-07-26

- Q: How do behavior coverage and combination (pairwise/triple) coverage relate — one obligation kind or two? → A: Two obligation kinds sharing one disposition vocabulary: **behavior obligations** (behavior × its own required context) and **combination obligations** (a valid pair, or a selected triple, of scenario-dimension values). Both resolve to exactly one of the four dispositions and both gate certification. A single kind crossing every behavior with every pair would multiply 27 behaviors by the pair space, producing an obligation set nobody can disposition.
- Q: Is pairwise coverage computed globally across all valid values, or partitioned per operation? → A: **Per operation.** The operation dimension is a partition key; within each operation, every valid pair of the remaining applicable scenario dimensions must be covered. Environment dimensions are excluded from pairwise entirely — they determine runnability, not scenario.
- Q: How is an injected regression introduced — by mutating source, by perturbing captured evidence, or at the system boundary? → A: **At the system boundary** — perturb the real artifact, process result, or container the observer reads. Mutating the observer's own return value is forbidden, and mutating deacon's source is out of scope.
- Q: What bounds the live tier's runtime, and over how many runs is the flake count measured? → A: The container-backed tier MUST complete within **30 minutes** on the certification lane, with a **5-minute** fail-loud per-case timeout. Flake determination is **ten** consecutive runs for the hermetic set and **three** for the live set.
- Q: What is the canonical term for an obligation whose environment is not active? → A: **`inactive-environment`**, replacing the working phrase "not-runnable-here". It names the cause rather than a location, and cannot be confused with the existing `no-reference-for-platform` (evidence missing for an *active* platform) or with the `not-applicable` classification disposition.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See the shape of the hole (Priority: P1)

A maintainer wants to know, before writing a single new case, exactly which combinations of
operation, configuration source, container state, Features, layering, platform, and output
mode the conformance record actually exercises — and which valid combinations it has never
touched.

Today they cannot: the coverage denominator counts behaviors, and a combination no behavior
mentions contributes nothing to any number. The maintainer needs a **constrained context
model** with explicit **applicability rules**, and a **generated report** that divides valid
combinations into covered and uncovered.

**Why this priority**: Every other story depends on this one. Without a denominator that
includes what is missing, "filling the gaps" has no stopping condition and no way to show
progress. This story alone — with zero new test cases — converts an unknown into a measured,
reviewable backlog, which is the single highest-value increment.

**Independent Test**: Define the model and applicability rules, run the coverage report
against the *existing, unchanged* case set, and confirm it enumerates the valid combination
space, marks which combinations existing cases cover, and lists the remainder. Value is
delivered even if no case is ever added.

**Acceptance Scenarios**:

1. **Given** the context model with its dimensions and applicability rules, **When** the
   coverage report is generated, **Then** it lists every valid combination of the model,
   excludes every combination an applicability rule forbids, and states for each valid
   pairwise combination whether an executable case covers it.
2. **Given** an applicability rule that forbids a combination, **When** the report is
   generated, **Then** that combination appears in neither the covered nor the uncovered
   population, and the rule that excluded it is named.
3. **Given** the report is generated twice from an unchanged record, **When** the two outputs
   are compared, **Then** they are byte-identical.
4. **Given** a dimension value that no applicability rule permits in any combination, **When**
   the report is generated, **Then** it is reported as a dead value rather than silently
   carried.
5. **Given** a modelled but inactive environment, **When** the report is generated, **Then**
   its obligations are enumerated and reported inactive-environment — counted as neither covered
   nor gap — so the unexercised environment is visible as a backlog.
6. **Given** that inactive environment is subsequently marked active, **When** the report is
   regenerated, **Then** its obligations re-bucket to covered or uncovered with no change to
   the model, the applicability rules, or any case.

---

### User Story 2 - Nothing applicable stays unclassified (Priority: P1)

A release engineer needs the guarantee that every applicable behavior, in every context the
model says it must be exercised in, carries an explicit, reviewable decision — and that "we
never got to it" is a decision that **blocks the release** rather than one that hides.

Each applicable obligation — of either kind, behavior or combination — must resolve to exactly
one of four dispositions: an **executable deterministic case**, an explicit **non-testable
rationale**, a **scoped waiver with an expiry**, or a **visible unresolved gap**. Gaps and
expired waivers must prevent strict certification.

**Why this priority**: This is the enforcement half of Story 1. Story 1 makes the hole
visible; Story 2 makes it impossible to look away from. Shipping Story 1 without Story 2
produces a report nobody is obliged to act on.

**Independent Test**: Introduce an obligation with no disposition into a fixture record and
confirm strict certification fails naming that obligation; then apply each of the four
dispositions in turn and confirm the two that permit release do, and the two that block do.

**Acceptance Scenarios**:

1. **Given** an applicable obligation with no disposition, **When** strict certification runs,
   **Then** it exits non-zero and names the obligation, its behavior, and its context.
2. **Given** an obligation dispositioned as an unresolved gap, **When** strict certification
   runs, **Then** it exits non-zero.
3. **Given** an obligation dispositioned by a waiver whose expiry is earlier than the
   evaluation date, **When** strict certification runs, **Then** it exits non-zero and names
   the expired waiver.
4. **Given** an obligation dispositioned by an unexpired scoped waiver or an explicit
   non-testable rationale, **When** strict certification runs, **Then** it does not block, and
   the disposition is enumerated in the certification output.
5. **Given** an obligation carrying two dispositions, **When** the record is validated,
   **Then** validation fails — exactly one disposition is permitted.
6. **Given** a non-testable rationale that states no ground (a filler phrase), **When** the
   record is validated, **Then** validation rejects it.

---

### User Story 3 - Deterministic coverage of the shared consumer workflow (Priority: P2)

A contributor changing configuration discovery, parsing, substitution, merging, Feature
resolution, or any container-side step needs a deterministic case set that fails when their
change alters observable behavior — across the whole workflow, not just its first step.

The workflow to be covered end to end: configuration discovery; JSON and JSONC parsing;
variable substitution; validation timing; extends and merge behavior; Feature resolution and
ordering; lockfiles; image and Dockerfile builds; Compose behavior; container creation; setup;
lifecycle execution; restart and resume; exec; outdated; upgrade; down; and cleanup. Each
stage must carry cases for **valid** behavior, **boundary** behavior, **malformed** input,
**unsupported** input, and **reference-lenient** input.

**Why this priority**: This is the bulk of the value, but it is second because building it
before Stories 1–2 reproduces the original defect — a large case set with no way to say what
it still misses.

**Independent Test**: Run the deterministic case set against an unmodified tree and confirm it
passes; then perturb one workflow stage at a time and confirm the case set fails, attributing
the failure to the perturbed stage.

**Acceptance Scenarios**:

1. **Given** the full case set, **When** it runs against the current tree, **Then** every case
   reaches a definite verdict — agreement, characterized divergence, or failure — and no case
   is skipped, ignored, or conditionally excluded.
2. **Given** a workflow stage, **When** the per-operation coverage report is generated,
   **Then** that stage shows at least one valid-behavior case and at least one case from each
   input class the applicability rules permit for it.
3. **Given** a case exercising the reference-lenient input class, **When** it runs, **Then** it
   pins the **direction** of the difference — which side accepts and which rejects — and not
   merely that the two sides differ.
4. **Given** an operation for which the pinned reference has no equivalent, **When** its cases
   run, **Then** they are evaluated against the declared specification expectation rather than
   a differential, and the report states why.
5. **Given** a case whose observable output would otherwise vary between runs, **When** it runs
   repeatedly, **Then** it produces the same verdict every time.

---

### User Story 4 - Parity does not stop at acceptance (Priority: P2)

A maintainer must not be able to conclude "the reference accepts this too" and stop there,
when the reference merely **defers** its validation to a later stage that only runs with a
container runtime present. Today's error-path coverage lives almost entirely at configuration
read time, which is exactly where the reference is most lenient.

A container-backed error-path tier is required: cases where configuration read succeeds on
both sides and the divergence — or the agreement — appears only during build, container
creation, Feature installation, lifecycle execution, or teardown.

**Why this priority**: It closes a systematic blind spot rather than adding volume, so it
ranks with the workflow build-out rather than after it. It is separable because it depends on
runtime availability the earlier stories do not.

**Independent Test**: Take an input both sides accept at configuration read, run the
container-backed tier, and confirm the tier reports the later-stage outcome for both sides
with a definite verdict.

**Acceptance Scenarios**:

1. **Given** an input both sides accept at configuration read but that fails later, **When**
   the container-backed tier runs, **Then** it records the failing stage and the observable
   outcome for each side.
2. **Given** the container runtime or the pinned reference is unavailable, **When** the tier is
   selected, **Then** the run fails with a cause-specific error naming what was missing —
   never a skip, and never a pass.
3. **Given** the tier runs, **When** it completes, **Then** every container, network, volume,
   and temporary directory it created has been reclaimed, on success and on failure alike.
4. **Given** two container-backed cases run concurrently, **When** both complete, **Then**
   neither observed the other's resources.

---

### User Story 5 - Fields that broad normalization used to hide (Priority: P2)

A reviewer needs the specific fields that previous blanket normalization suppressed to be
compared, because a field that is captured but never compared is exactly where a false claim
of equivalence survives longest.

In scope: lifecycle hooks in array versus object form; commands; entrypoints; environment
merge precedence; PATH construction; users and the effects of UID/GID; metadata label
namespaces; mounts and their sources; networks; Compose project resources; Feature install
order; and the distinction between null, empty, and omitted.

**Why this priority**: These are known, named, high-yield targets — the preceding phase found
two real defects the moment one such field stopped being suppressed. They rank with the
build-out because each needs its own case, not merely a normalization change.

**Independent Test**: For each named field, confirm a case observes it, and confirm that
changing that field alone in the tree makes that case fail.

**Acceptance Scenarios**:

1. **Given** a named field from the list above, **When** the per-observable coverage report is
   generated, **Then** at least one executable case compares that field.
2. **Given** a configuration distinguishing null from empty from omitted for a field, **When**
   the cases run, **Then** the three states produce three distinguishable recorded
   observations.
3. **Given** a lifecycle hook expressed as an array and the same hook expressed as an object,
   **When** the cases run, **Then** both forms are observed and any difference in execution is
   recorded.
4. **Given** a normalization rule that removes or collapses observable content, **When** the
   record is validated, **Then** the rule must be named, scoped to a specific field, and
   justified — an unscoped rule is rejected.
5. **Given** Features whose declared install order is ambiguous, **When** the case runs,
   **Then** the case either pins a deterministic order or is dispositioned, never left to
   chance.

---

### User Story 6 - Prove the suite can fail (Priority: P3)

A maintainer must be able to demonstrate that each observable channel is **live** — that a
regression visible on that channel actually turns the suite red. A green suite whose channels
are inert is worse than no suite, because it is trusted.

For each observable channel, a deliberate, reverted regression is injected and the suite must
fail, attributing the failure to that channel.

**Why this priority**: It validates the other stories rather than adding coverage, so it
follows them — but it is the criterion that makes their green result mean something.

**Independent Test**: Run the injected-regression harness; confirm each channel has at least
one injected regression the suite detects, and that a channel with no detecting case is
reported inert.

**Acceptance Scenarios**:

1. **Given** an injected regression targeting one observable channel, **When** the suite runs,
   **Then** it fails and the failure names that channel.
2. **Given** an observable channel for which no injected regression causes a failure, **When**
   the injected-regression report is generated, **Then** that channel is reported inert and
   the result is treated as a failure of this story's acceptance.
3. **Given** the injected-regression run completes, **When** the tree is inspected, **Then** no
   injected regression remains applied.
4. **Given** injected regressions are run twice, **When** the two reports are compared, **Then**
   the detected/inert classification is identical.

---

### Edge Cases

- **A valid pair whose only coverage is a legacy carrier.** Coverage obligations are satisfied
  only by executable cases in the declarative record. A legacy carrier satisfies an obligation
  only while an open residual names it, and the obligation reverts to uncovered when that
  carrier is retired.
- **A high-risk triple that cannot be made deterministic** (for example, one requiring a
  network fetch). It must resolve to a non-testable rationale or a gap — never to silent
  omission, and never to a case expected to be flaky.
- **The pinned reference lacks an operation** the model requires. The case must fall back to a
  specification expectation and say so; it must not be dropped and must not compare against a
  non-existent oracle.
- **A required context whose environment is not active** (a non-default runtime, or a platform
  with no container runtime in the certification lane). The obligation is reported
  inactive-environment — distinct from covered, distinct from a gap, and never a silent skip.
- **A waiver that expires between the last green run and the release.** Certification evaluates
  expiry at run time against the evaluation date, so the release blocks.
- **A deacon-side failure occurring earlier than the reference's.** The differential alone
  proves only disagreement; the direction must be pinned by a companion expectation case.
- **Two dispositions claiming the same obligation**, or a disposition whose scope resolves to
  no obligation. Both fail validation; a tolerance that has stopped being exercised is
  reported stale rather than quietly retained.
- **A dimension value that becomes dead** after an applicability-rule change — reported, not
  carried.
- **Concurrent container-backed cases colliding** on names, ports, or workspace paths.
  Isolation is required; a collision is a defect, not a retry condition.
- **A newly added behavior with no obligation.** Adding a behavior must create its obligations;
  a behavior creating none fails validation, since otherwise enlarging the denominator would
  be a way to dilute it.
- **Reports consumed as a release gate while being regenerated.** Report generation is
  read-only with respect to the record; generating a report never records, refreshes, or
  repairs evidence.

## Requirements *(mandatory)*

### Functional Requirements

#### A. The constrained context model

- **FR-001**: The record MUST define a context model composed of named dimensions, each with
  an enumerated, closed set of values.
- **FR-002**: The model MUST distinguish **scenario dimensions** (what a case exercises) from
  **environment dimensions** (where a case can run), because the two have different
  consequences: an unexercised scenario is missing coverage, whereas an unavailable
  environment is unavailable evidence.
- **FR-003**: Scenario dimensions MUST include at least: **operation**; **configuration
  source** (image reference, Dockerfile, Compose); **container state**; **Features and Feature
  ordering**; **configuration layering and extends**; and **output mode** (structured versus
  human-readable).
- **FR-004**: Environment dimensions MUST include at least **container runtime** and
  **platform profile** (operating system, architecture, and pinned reference version).
- **FR-004a**: Exactly one environment profile is **active** for this feature; every other
  modelled environment MUST remain in the closed value set with its obligations
  **enumerated** and reported inactive-environment, so the unexercised environment is visible
  as a backlog rather than absent from the model.
- **FR-004b**: Activating a further environment later MUST be a **data change** — marking a
  profile active — and MUST NOT require changing the model, the applicability rules, or any
  case. On activation, the affected obligations MUST move from inactive-environment to covered
  or uncovered according to the evidence, with no re-authoring.
- **FR-005**: The operation dimension MUST enumerate every in-scope consumer operation:
  configuration read, build, up, exec, run-user-commands, down, outdated, upgrade, template
  application, and diagnostics.
- **FR-006**: The container-state dimension MUST distinguish at least: no container; a stopped
  container; a running container; and a running container whose configuration has since
  changed.
- **FR-007**: The Features dimension MUST distinguish at least: no Features; a single Feature;
  multiple Features with a declared order; multiple Features whose order is determined by
  dependency resolution; and Features resolved against a lockfile.
- **FR-008**: The layering dimension MUST distinguish at least: a single configuration; an
  extends chain; command-line overlays; and image-metadata-derived layers.
- **FR-009**: The output-mode dimension MUST distinguish structured from human-readable output
  for every operation that emits both.
- **FR-010**: Every dimension value MUST be reachable — a value permitted by no applicability
  rule in any combination MUST be reported as dead.

#### B. Applicability and combination selection

- **FR-011**: The record MUST express **applicability rules** that mark combinations invalid,
  each rule carrying a stated ground.
- **FR-012**: An invalid combination MUST be excluded from the coverage denominator entirely,
  and the report MUST name the rule that excluded it.
- **FR-013**: **Combination obligations** MUST be generated **pairwise** across valid values of
  distinct applicable **scenario** dimensions. The full Cartesian product MUST NOT be generated.
- **FR-013a**: Pairwise generation MUST be partitioned by **operation**: within each operation,
  every valid pair of the remaining applicable scenario dimensions MUST yield an obligation. A
  pair covered under one operation MUST NOT count as covering that pair under another, because
  the same pair means different things per operation and pooling would let one operation's
  coverage mask another's.
- **FR-013b**: Environment dimensions MUST NOT participate in pairwise generation. They
  determine whether an obligation is runnable, not what it exercises.
- **FR-014**: The record MUST additionally carry an explicitly enumerated set of **high-risk
  triples**, each with a stated reason for its selection.
- **FR-015**: A high-risk triple MUST be satisfied by an executable case; a rationale or a
  waiver is not sufficient for a triple, though a gap remains available and blocking.
- **FR-016**: The set of high-risk triples MUST be hand-authored, never machine-derived, so
  that selection is a reviewable judgement rather than a side effect of enumeration.
- **FR-017**: Adding a dimension value or an applicability rule MUST regenerate obligations
  deterministically, such that the same model always yields the same obligation set.
- **FR-018**: Obligation generation MUST be machine-owned and MUST NOT alter any hand-authored
  disposition.

#### C. Disposition completeness and certification gating

- **FR-019**: The record MUST carry exactly **two** kinds of obligation, sharing one disposition
  vocabulary: a **behavior obligation** (a behavior paired with a context its own applicability
  requires) and a **combination obligation** (a valid pair, or a selected high-risk triple, of
  scenario-dimension values). The two kinds MUST NOT be multiplied together.
- **FR-019a**: Every applicable obligation of either kind MUST carry exactly one disposition: an
  executable deterministic case, an explicit non-testable rationale, a scoped waiver with an
  expiry, or an unresolved gap.
- **FR-020**: An obligation with zero dispositions, or with more than one, MUST fail validation.
- **FR-021**: An unresolved gap MUST always block strict certification.
- **FR-022**: A waiver whose expiry precedes the evaluation date MUST block strict
  certification and MUST be named in the output.
- **FR-023**: A waiver MUST be scoped to specific observable content — a blanket or
  channel-wide tolerance MUST be rejected.
- **FR-024**: A waiver or tolerance whose difference has stopped reproducing MUST be reported
  **stale**, so that acceptance decays rather than persisting unchallenged.
- **FR-025**: A non-testable rationale MUST name a ground — a stated principle or a specific
  unobservable mechanism. A filler phrase MUST be rejected.
- **FR-026**: Certification output MUST report, separately and without folding them together:
  covered obligations, waived obligations, non-testable obligations, gaps, and
  `inactive-environment` obligations.

#### D. Deterministic coverage of the consumer workflow

- **FR-027**: Deterministic cases MUST cover configuration discovery, including the locations
  that are and are not discovery locations.
- **FR-028**: Deterministic cases MUST cover JSON and JSONC parsing, including comments,
  trailing commas, duplicate keys, and hard syntax errors.
- **FR-029**: Deterministic cases MUST cover variable substitution across the field surfaces
  that carry user templates, including nested and object-shaped fields.
- **FR-030**: Deterministic cases MUST cover **validation timing** — which stage rejects a
  given input on each side.
- **FR-031**: Deterministic cases MUST cover extends chains and merge precedence, including
  conflicts, cycles, and missing parents.
- **FR-032**: Deterministic cases MUST cover Feature resolution and **install order**,
  including declared order, dependency-derived order, and command-line order overrides.
- **FR-033**: Deterministic cases MUST cover lockfile production and consumption.
- **FR-034**: Deterministic cases MUST cover image-reference and Dockerfile builds, including
  build arguments and build-time failure.
- **FR-035**: Deterministic cases MUST cover Compose configurations, including multi-service
  shapes and the project resources a run creates.
- **FR-036**: Deterministic cases MUST cover container creation and setup, including the
  container's identity, labels, mounts, environment, and user.
- **FR-037**: Deterministic cases MUST cover lifecycle execution, including hook ordering,
  hooks contributed by Features, and hook failure.
- **FR-038**: Deterministic cases MUST cover restart and resume, distinguishing first creation
  from re-entry into an existing container.
- **FR-039**: Deterministic cases MUST cover exec, outdated, upgrade, down, and cleanup,
  including the resources each removes and each leaves behind.
- **FR-040**: For each workflow stage, cases MUST span the input classes the applicability
  rules permit: valid, boundary, malformed, unsupported, and reference-lenient.

#### E. The container-backed error-path tier

- **FR-041**: The record MUST carry an error-path tier whose cases begin from inputs that
  configuration read **accepts** on both sides.
- **FR-042**: Each such case MUST record the stage at which the failure occurs and the
  observable outcome for each side.
- **FR-043**: When a difference exists, the case MUST pin its **direction**, not only its
  existence.
- **FR-044**: When the container runtime or the pinned reference is unavailable, selection of
  this tier MUST fail with a cause-specific error. Skipping and passing are both forbidden.
- **FR-045**: Each case MUST run in an isolated workspace with collision-resistant resource
  names, and MUST reclaim every container, network, volume, and temporary directory it creates,
  on success and on failure.
- **FR-046**: Every image input MUST be pinned to a digest or a concrete tag.

#### F. Observable fidelity

- **FR-047**: Cases MUST compare lifecycle hooks in both array and object forms.
- **FR-048**: Cases MUST compare commands and entrypoints, including chained entrypoints
  contributed by multiple Features.
- **FR-049**: Cases MUST compare environment **merge precedence** across configuration,
  Features, image metadata, and command-line sources.
- **FR-050**: Cases MUST compare PATH construction, including segments contributed by Features
  and by the probed environment.
- **FR-051**: Cases MUST compare the effective user and the observable effects of UID and GID,
  including a non-root user and a user created by a Feature.
- **FR-052**: Cases MUST compare metadata labels **by namespace**, including labels one side
  emits and the other does not.
- **FR-053**: Cases MUST compare mounts and their **sources**, distinguishing a differing source
  path from a differing mount shape.
- **FR-054**: Cases MUST compare networks and Compose project resources.
- **FR-055**: Cases MUST preserve the distinction between **null**, **empty**, and **omitted**
  for every field they observe.
- **FR-056**: Normalization rules MUST be named, scoped to a specific field, and justified. An
  unscoped or unjustified rule MUST be rejected, and evidence MUST be retained in both raw and
  normalized form.

#### G. Generated reports

- **FR-057**: A **pairwise-coverage report** MUST be generated, stating for each valid pair
  whether it is covered, and by which case or disposition.
- **FR-058**: A **high-risk-triple report** MUST be generated, stating for each selected triple
  its covering case or its blocking gap.
- **FR-059**: A **per-operation coverage report** MUST be generated, stating for each operation
  the input classes and observables it exercises.
- **FR-060**: A **per-observable coverage report** MUST be generated, stating for each
  observable channel the number of cases that compare it and the fields they compare.
- **FR-061**: Reports MUST state the count of unclassified applicable behaviors, which MUST be
  zero for certification to pass.
- **FR-062**: Reports MUST be deterministic and byte-stable: free of timestamps, absolute paths,
  and run-dependent ordering.
- **FR-063**: Report generation MUST be read-only with respect to the record and MUST NOT
  record, refresh, or repair evidence.
- **FR-064**: All reporting and validation commands MUST remain development-only and MUST NOT
  appear in the shipped consumer command surface.

#### H. Injected regressions

- **FR-065**: For each observable channel, at least one **injected regression** MUST exist that
  the case set detects.
- **FR-065a**: An injected regression MUST be introduced at the **system boundary** — the real
  artifact, process result, or container the observer reads — so that detection proves both that
  the observer reads the live system and that the comparison acts on what it reads.
- **FR-065b**: An injected regression MUST NOT be introduced by altering an observer's own
  returned value. Doing so would let a dead observer — one that never reads the system — appear
  live, which is the exact failure this story exists to exclude.
- **FR-066**: An injected regression MUST be applied and reverted by the harness, leaving the
  tree unmodified afterwards. Mutating deacon's source is out of scope; regressions perturb
  observed artifacts, not the program under test.
- **FR-067**: A channel with no detecting injected regression MUST be reported **inert**, and an
  inert channel MUST fail the injected-regression acceptance run.
- **FR-068**: Injected-regression results MUST name the channel and the case that detected the
  regression.
- **FR-069**: The injected-regression classification MUST be reproducible across runs.
- **FR-070**: Injected regressions MUST NOT be part of the ordinary case set and MUST NOT be
  able to leave a regression applied in a normal run.

#### I. Test policy and determinism

- **FR-071**: Automated acceptance tests are mandatory: every acceptance scenario in this
  specification MUST have an automated test.
- **FR-072**: All tests MUST be executed by the repository's single test executor, under a named
  profile. No other execution path is permitted.
- **FR-073**: Hermetic tests — model validity, applicability rules, obligation generation,
  disposition completeness, report determinism, certification gating — MUST run on every pull
  request.
- **FR-074**: Live tests requiring a container runtime or the pinned reference MUST run only
  under the dedicated live profile and MUST be excluded from every other profile, so that a
  green fast run never implies live coverage.
- **FR-075**: No test may be ignored, conditionally disabled, or gated behind an environment
  variable opt-in. Unavailable prerequisites fail loudly.
- **FR-076**: Each new live test program MUST be registered in the parity registry and given
  overrides in every profile, and the structural agreement check MUST enforce this.
- **FR-077**: Container-backed cases MUST declare a resource group appropriate to their
  concurrency, and MUST be free of inter-case interference.
- **FR-077a**: The container-backed tier MUST complete within **30 minutes** on the certification
  lane. Exceeding the budget is a failure of this feature's acceptance, not a reason to widen it.
- **FR-077b**: Each case MUST carry a **5-minute** timeout that fails loudly on expiry, so a hung
  case is reported as a failure rather than consuming the tier's budget.
- **FR-078**: Certification gating MUST itself be tested against fixture records covering each
  blocking and non-blocking disposition, so that the gate's own failure modes are demonstrated
  rather than assumed.

### Key Entities

- **Context Dimension**: A named axis of variation with a closed value set, classified as
  scenario (what a case exercises) or environment (where a case can run).
- **Applicability Rule**: A stated, grounded exclusion marking a combination of dimension values
  invalid and removing it from the coverage denominator.
- **Coverage Obligation**: A generated unit that must resolve to exactly one disposition.
  Machine-owned; never edited by hand. Exists in two kinds that are never multiplied together:
  a **behavior obligation** (a behavior paired with a context its applicability requires) and a
  **combination obligation** (a valid pair, or a selected high-risk triple, of
  scenario-dimension values, partitioned by operation).
- **Disposition**: One of four resolutions of an obligation — executable case, non-testable
  rationale, scoped expiring waiver, or unresolved gap. The first three permit certification
  while unexpired; the fourth, and an expired waiver, block it.
- **High-Risk Triple**: A hand-selected three-dimension combination carrying a stated reason,
  which only an executable case can satisfy.
- **Deterministic Case**: An executable case whose observations are stable across runs and which
  reaches a definite verdict on every run.
- **Error-Path Case**: A container-backed case beginning from an input that configuration read
  accepts on both sides, whose divergence or agreement appears at a later stage.
- **Observable Channel**: A distinct surface on which a difference can be seen — process exit
  status, standard output, standard error, structured output, filesystem, file content, image,
  container state, process graph, injected process, and lifecycle timing.
- **Injected Regression**: A deliberate, reverted perturbation of the real artifact, process
  result, or container an observer reads, targeting one observable channel, used to prove that
  channel is live. Never a perturbation of the observer's own returned value.
- **Coverage Report**: A deterministic, byte-stable rendering of the obligation set and its
  dispositions, in pairwise, triple, per-operation, and per-observable views.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The coverage report enumerates the full valid combination space of the context
  model, and the count of applicable obligations with no disposition is **zero**.
- **SC-002**: 100% of valid pairwise combinations across applicable dimensions are either
  covered by an executable case or carry an explicit disposition, with that disposition visible
  in the report.
- **SC-003**: At least **twelve** high-risk triples are explicitly selected with stated reasons,
  and **100%** of them are covered by executable cases.
- **SC-004**: Every in-scope consumer operation has at least one executable case for each
  configuration source the applicability rules permit for it, raising per-operation coverage
  from **three** operations today to **all ten**.
- **SC-005**: Every observable channel is compared by at least **three** executable cases,
  eliminating the single-observation and zero-observation channels that exist today.
- **SC-006**: Every observable channel has at least one injected regression the suite detects;
  the count of inert channels is **zero**.
- **SC-007**: The container-backed error-path tier contains at least one case for each
  later-stage failure point — build, container creation, Feature installation, lifecycle
  execution, and teardown — and no error-path case reaches its verdict at configuration read.
- **SC-008**: Each field previously suppressed by broad normalization — lifecycle array versus
  object, commands, entrypoints, environment merge precedence, PATH, user and UID/GID effects,
  metadata label namespaces, mounts and sources, networks, Compose project resources, Feature
  install order, and null/empty/omitted — is compared by at least one executable case, with
  **zero** unscoped normalization rules remaining.
- **SC-009**: Strict certification fails when an unresolved gap exists and when a waiver has
  expired, demonstrated by fixture records rather than asserted.
- **SC-010**: All four reports are byte-identical across repeated generation from an unchanged
  record, verified automatically.
- **SC-011**: The case set produces the same verdicts across **ten** consecutive runs of the
  hermetic set and **three** consecutive runs of the live set; the flake count is **zero**. The
  live set completes within **30 minutes** per run.
- **SC-012**: No case is skipped, ignored, or conditionally excluded in any run; an unavailable
  prerequisite produces a failure naming the missing prerequisite.
- **SC-013**: Adding a case, an assertion, a fixture, a dimension value, or an applicability
  rule requires only a data edit — the count of new hand-written test functions required is
  **zero**.
- **SC-014**: Adding a behavior without a disposition for its generated obligations is rejected,
  so the coverage denominator cannot be diluted by growth.
- **SC-015**: Activating an additional environment profile requires **zero** changes to the
  context model, the applicability rules, or any case — demonstrated by activating a second
  profile in a fixture record and confirming its obligations re-bucket from inactive-environment.

## Assumptions

1. **Naming.** Earlier in-repository references to "024 Phase N" describe the deferral drain
   that completes the preceding migration. This document is the formal 024 feature; that drain
   is treated as an in-flight dependency, not as part of this scope.
2. **Scope is characterization, not repair.** Newly surfaced differences are classified,
   recorded, and — for the fix-flavored kind — tracked as separate work. Fixes land as their own
   reviewed changes, except where a difference prevents a required case from being
   **deterministic**, in which case fixing it is in scope here. This matches the precedent set
   by the preceding phase, where two defects were fixed precisely because they blocked cases.
3. **Waiver expiry defaults to six months** from the date added, matching existing practice, with
   no auto-renewal.
4. **Structural coverage does not imply agreement.** An obligation covered by a case that records
   a characterized divergence is covered; it is not conformant. The report keeps the two separate.
5. **Reference availability.** Where the pinned reference lacks an equivalent for an in-scope
   operation, cases for that operation are evaluated against declared specification expectations,
   and the report states the substitution.
6. **Determinism excludes the network.** Any obligation whose only realization requires a network
   fetch resolves to a non-testable rationale or a gap.
7. **Feature authoring stays out of scope**, consistent with the consumer-only constraint;
   obligations that would require it are permanently excluded with a stated ground.
8. **Existing coverage is preserved.** No currently covered obligation loses coverage as a result
   of this work; the model is additive to the migrated record.
9. **Pairwise coverage is a blocking condition**, not an advisory metric: an undispositioned valid
   pair blocks certification exactly as a gap does.
10. **One active environment profile.** The current Linux/amd64 default-runtime profile is the
    only active one. Alternative runtimes and non-Linux platforms are modelled, their obligations
    enumerated and reported inactive-environment, and their activation is deliberately deferred to
    later work — which FR-004b keeps to a data change rather than a re-modelling.

    **Outcome of T149 (attempted, deliberately not activated).** A second profile record now
    exists — `prof-linux-amd64-podman-0870`, identical but for `dim-runtime: podman` — so
    activation is the one-field data change FR-004b promises. It is `active: false`, and the
    reason is empirical rather than budgetary. Activation was tried for real against a copy of
    the registry, and `validate` plus `certify` returned results **identical to the docker
    profile's**: 787 obligations, 370 covered, 417 gap, `inactive-environment 0`, the same ten
    blocking gaps. The re-bucketing SC-015 predicts is real, but it is currently VACUOUS: all 55
    behaviors carry an empty `applicability`, so every one of them applies in every environment
    and nothing is runtime-conditioned for a profile swap to move. That identical output is
    precisely why the flag stays off — activation would move nothing, and so would verify
    nothing, while asserting that the whole registry's `reference` axis holds under a runtime no
    case has ever been executed against. Under this registry's own core principle ("statuses are
    evidence-backed claims, not aspirations") that is the one thing a profile flag must not do.
    Note also that activation can only ever be a **swap**, never an addition: coverage and
    validation both resolve the active profile with `.find(|p| p.active)`, so a second
    `active: true` is never consulted — a silent no-op now guarded by
    `registry_valid::exactly_one_environment_profile_is_active`. Genuine activation needs a
    podman execution lane (research Decision: an order-of-magnitude live-cost multiplier) and
    runtime-conditioned `applicability` on the behaviors that actually differ; neither is in
    scope here.
11. **`inactive-environment` never blocks and never counts as covered.** It is a fifth reporting
    bucket. Folding it into coverage would let a green run in one environment read as evidence
    about another; folding it into gaps would block every release on environments nobody has
    chosen to certify yet.

## Dependencies

- The migrated declarative case record, its runner, and its observation and normalization
  machinery.
- The pinned upstream specification revision and the pinned reference version, which together
  define the current platform profile.
- The dedicated live certification lane, which supplies the container runtime and the pinned
  reference for the live and error-path tiers.
- Completion of the outstanding deferral drain that retires the remaining legacy carriers; an
  unretired carrier continues to satisfy the obligations its residual names.

## Out of Scope

- Feature authoring operations, which the project excludes permanently.
- Non-behavioral differentiators — packaging, distribution shape, and performance
  characteristics — which have no observable effect and are recorded nowhere.
- Repairing every difference the new coverage surfaces (see Assumption 2).
- Adding predicates, quantifiers, or cross-field expressions to the assertion language; where an
  assertion appears to need a search, the observation gains a derived field instead.
