# Feature Specification: Harden the Parity Test Harness

**Feature Branch**: `018-harden-parity-harness`
**Created**: 2026-07-19
**Status**: Draft
**Input**: User description: "Harden Deacon's existing parity test harness so that a reported passing result always proves the intended reference comparison actually ran. The exact stable oracle for this certification line is @devcontainers/cli 0.87.0. Before running live parity tests, the system must verify the exact oracle version and fail clearly if it is missing or different. Missing Docker, a missing oracle, an unavailable required fixture, an oracle crash, malformed oracle output, or normalization failure must never be reported as a passing parity test. Bring every existing parity test binary and corpus runner under one documented, nextest-only execution contract. … A difference may pass only when linked to an explicit characterized divergence or waiver. … Prevent CI profiles from claiming parity coverage when live parity was silently skipped. … Consolidate duplicate normalization behavior so runners do not disagree about what constitutes equivalence. Preserve raw outputs for diagnosis. Acceptance tests are mandatory. … No ignored tests, silent returns, or permissive fallback paths are allowed."

## Problem Statement

Deacon's claim of behavioral parity with the reference DevContainer CLI rests on a
parity test suite that can — and today routinely does — report success without ever
performing a comparison. Every live parity test self-skips with a quiet message and a
passing status when the reference CLI, Docker, or an opt-in environment flag is absent;
two of the three corpus runners always exit successfully regardless of what they find;
the oracle version (0.87.0) is documented in prose but never verified; and three
independent, disagreeing definitions of "equivalent output" exist across the harness.
The net effect: a green parity result is currently evidence of nothing. This feature
makes a passing parity result a trustworthy certification that the intended comparison
against the pinned oracle actually ran and actually matched.

## Clarifications

### Session 2026-07-19

- Q: How do the corpus runners come under the nextest-only contract? → A: Port the
  Tier 1, merged-configuration, and error corpus runners to native test binaries that
  share the harness's single normalization/waiver code; retire the standalone scripts.
- Q: When does the parity-certification lane run in CI? → A: Nightly on the main
  branch + manual dispatch + automatically on PRs that touch the parity harness,
  corpus/fixtures, or the oracle pin; not a blanket required check on every PR.
- Q: What time bound applies to oracle invocations? → A: 2 minutes for
  configuration-only invocations; 15 minutes for container-lifecycle invocations;
  exceeding the bound fails the check with a timeout cause.
- Q: What happens to the existing manual parity entry point (`make test-parity`)? →
  A: Retained as a thin alias that delegates to the sanctioned execution contract,
  with no gating logic of its own.
- Q: What shape does the unified waiver model take? → A: One documented waiver
  schema with a single shared loader/validator; records live adjacent to the cases
  they waive, each carrying case identity, expected difference, and rationale, and
  all are staleness-validated by the same code.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A passing parity result proves the comparison ran (Priority: P1)

A maintainer (or CI reviewer) sees a passing parity lane and needs to trust it as
evidence that Deacon's behavior was genuinely compared against the pinned reference
CLI and matched. If any prerequisite for that comparison was missing — the reference
CLI itself, the exact pinned version, Docker in a Docker-required lane, a required
fixture — or the comparison itself broke down (oracle crash, unparseable oracle
output, normalization failure), the result must be a clear failure naming the exact
cause, never a pass.

**Why this priority**: This is the core trust property. Every other improvement is
worthless if a green result can still mean "nothing was checked." Today all live
parity test binaries silently self-skip to a passing status when prerequisites are
missing, which is the primary defect being fixed.

**Independent Test**: On a machine with the correct oracle installed, run the live
parity suite and observe passes accompanied by evidence of real comparisons. Then
remove or replace the oracle (wrong version), remove Docker access, or delete a
required fixture, re-run, and observe hard failures that name the missing
prerequisite — never a pass, never a silent skip.

**Acceptance Scenarios**:

1. **Given** the pinned oracle (reference CLI 0.87.0) is installed and Docker is
   available, **When** the live parity suite runs, **Then** every executed check
   performs a real comparison and the run report records the verified oracle version.
2. **Given** the reference CLI is not installed, **When** a live parity check is
   selected to run, **Then** it fails with a message stating the oracle is missing and
   how to provision it — it does not pass or quietly skip.
3. **Given** a reference CLI of any version other than 0.87.0 is installed, **When**
   live parity runs, **Then** the run fails up front with a message naming the found
   version and the required version.
4. **Given** Docker is unavailable, **When** a Docker-required parity check is
   selected to run, **Then** it fails naming Docker as the missing prerequisite.
5. **Given** a required fixture is missing or unreadable, **When** the check that
   depends on it runs, **Then** that check fails naming the fixture.
6. **Given** the oracle crashes, emits malformed output, or output normalization
   itself fails during a comparison, **When** the check completes, **Then** it is
   reported as a failure with the underlying cause preserved for diagnosis — never
   as a pass.

---

### User Story 2 - One execution contract with explicit pass/fail for every runner (Priority: P2)

A maintainer runs the entire parity surface — all existing parity test binaries plus
the Tier 1 read-configuration corpus runner, the merged-configuration corpus runner,
the Tier 1c error-corpus runner, and the observable-state comparisons — through the
repository's single sanctioned test-execution system, and every one of them has
explicit, documented pass/fail semantics. A detected difference can pass only when it
is linked to an explicitly recorded characterized divergence or waiver.

**Why this priority**: Two of the three corpus runners today are report-only (they
always exit successfully, even on divergence), and the parity surface is split across
ad-hoc invocation paths outside the sanctioned test system. Without a single gateable
contract, User Story 1's guarantees can't be enforced uniformly.

**Independent Test**: Introduce a deliberate behavioral difference in a test build of
Deacon (or inject a modified fixture), run each runner through the sanctioned test
system, and confirm each one independently fails. Add a characterized-divergence
record for that difference and confirm the runner passes again, with the waiver
identified in the report.

**Acceptance Scenarios**:

1. **Given** the full parity surface, **When** it is executed, **Then** every parity
   binary and corpus runner runs under the repository's single sanctioned
   test-execution system with documented resource grouping — no side-channel or
   manual-only invocation paths remain for gating purposes.
2. **Given** the Tier 1 or merged-configuration corpus runner encounters an
   unexpected reference-only property, Deacon-only property, value mismatch, process
   failure, or normalization failure, **When** the run completes, **Then** the runner
   reports failure (nonzero outcome) identifying each offending case.
3. **Given** a difference that is covered by an explicit characterized-divergence or
   waiver record, **When** the affected runner executes, **Then** the case passes and
   the report identifies which record justified it.
4. **Given** a waiver record that no longer corresponds to any existing case or whose
   expected difference is no longer observed (stale waiver), **When** the suite runs,
   **Then** the run flags it as a failure so obsolete waivers cannot accumulate
   silently.
5. **Given** tests that carry a parity-suggesting name but never consult the
   reference CLI (internal-consistency checks), **When** the contract is applied,
   **Then** they are reclassified or renamed so that names, groups, and reports never
   overstate what was compared.

---

### User Story 3 - CI status that cannot claim unearned parity coverage (Priority: P3)

A reviewer looking at CI results can tell exactly which parity checks performed live
comparisons against the oracle, which ran in snapshot/replay mode (if any), and which
were deliberately excluded from a given lane. A lane that did not run live parity can
never present itself as having certified parity.

**Why this priority**: Today CI profiles compile and execute the parity binaries with
prerequisites absent, so every check self-skips and the lane shows green — an
actively misleading signal. Fixing the reporting/selection layer completes the trust
chain from individual checks (P1/P2) up to the CI badge a reviewer actually reads.

**Independent Test**: Run the test lanes as CI does. Confirm that lanes which exclude
parity say so via selection (the checks simply are not selected, and the lane makes
no parity claim), and that any lane claiming parity coverage fails when live
comparison prerequisites are absent. Confirm reports enumerate what ran, what was
omitted, and which oracle was used.

**Acceptance Scenarios**:

1. **Given** a test lane that does not provision the oracle, **When** it runs,
   **Then** parity checks are excluded by explicit selection (visible as not-run),
   not executed-and-self-passed — the lane's status makes no parity claim.
2. **Given** a lane designated as the parity-certification lane, **When**
   prerequisites are missing there, **Then** the lane fails (per User Story 1) rather
   than turning green.
3. **Given** any parity run, **When** it completes, **Then** its report identifies:
   which binaries and corpus cases executed, which were omitted and why, which oracle
   version was verified and used, and whether each check was a live comparison or a
   snapshot/replay evaluation.
4. **Given** names, reports, and CI status entries, **When** a snapshot-replay style
   check exists alongside live comparison checks, **Then** the two are visibly
   distinguished everywhere they appear.

---

### User Story 4 - One definition of equivalence, raw outputs preserved (Priority: P4)

A maintainer diagnosing a parity failure works from a single, shared definition of
output equivalence used by every runner, and always has access to the raw,
unnormalized outputs from both Deacon and the oracle for the failing case.

**Why this priority**: Three independent normalization implementations currently
disagree about what "equivalent" means, so the same behavioral difference can pass in
one runner and fail in another. Consolidation makes results consistent; raw-output
retention makes failures diagnosable. This builds on but does not block P1–P3.

**Independent Test**: Feed the same pair of differing outputs through each runner and
confirm they all reach the same verdict. Trigger a failure and confirm the raw
outputs of both sides are available alongside the normalized diff.

**Acceptance Scenarios**:

1. **Given** the same pair of outputs, **When** any two parity runners of the same
   comparison type evaluate them, **Then** they reach the same equivalence verdict,
   because they share one normalization definition.
2. **Given** any comparison that fails, **When** diagnosis is needed, **Then** the
   raw outputs of both Deacon and the oracle are preserved and locatable from the
   run report without re-running.
3. **Given** a change to the shared normalization rules, **When** the suite runs,
   **Then** all runners pick up the change together — no runner retains a private
   variant.

---

### User Story 5 - Acceptance tests that prove the harness cannot lie (Priority: P5)

A maintainer (or reviewer of this feature itself) can run a dedicated set of
acceptance tests that demonstrate, by fault injection, that every guaranteed failure
mode actually fails, and that every registered parity binary is actually executed.

**Why this priority**: These tests are the permanent regression guard for the trust
property. They are listed last only because they verify the behaviors delivered by
P1–P4; the feature is not complete without them.

**Independent Test**: Run the acceptance suite on a correctly provisioned machine;
all injections must produce failures and the registry-completeness check must pass.

**Acceptance Scenarios**:

1. **Given** the acceptance suite, **When** it simulates each of: wrong oracle
   version, missing oracle, missing Docker in a Docker-required lane, an injected
   output difference, malformed oracle output, and a normalization failure, **Then**
   each simulation demonstrably produces a failing parity result.
2. **Given** the registry of parity binaries and corpus runners, **When** the
   acceptance suite runs, **Then** it verifies every registered entry is executed by
   the sanctioned execution contract, and fails if any entry is unregistered,
   unreachable, ignored, or short-circuits without comparing.
3. **Given** the harness codebase, **When** the acceptance suite audits it, **Then**
   no ignored tests, silent early-return-as-pass paths, or permissive fallback paths
   remain in any parity check.

---

### Edge Cases

- **Two oracles resolvable at once** (an explicit override path and one on the
  system PATH, at different versions): version verification must apply to the binary
  that will actually be invoked, and the report must identify which one that was.
- **Oracle version query itself fails or emits garbage**: treated as "oracle
  missing/unverifiable" — a failure, not a pass.
- **Oracle hangs**: comparisons must bound their wait and report a timeout failure
  rather than stalling the lane indefinitely or being killed into an ambiguous state.
- **Empty or partially discovered corpus**: a corpus runner that discovers zero cases
  (or fewer than the registered expectation) must fail — an empty run must not read
  as "all cases passed."
- **A new parity binary or corpus is added but not registered**: the
  registry-completeness acceptance check must fail until the addition is registered,
  so coverage claims can't silently rot.
- **Stale characterized-divergence/waiver records**: a waiver matching no current
  case or no observed difference fails the run (see US2 scenario 4).
- **Deacon-only checks with parity-style names**: internal-consistency tests that
  never invoke the oracle must not count toward, or appear as, live parity coverage
  (see US2 scenario 5).
- **Both sides fail identically**: when Deacon and the oracle both reject an input,
  that is a comparable outcome (as in the error corpus today) — an oracle process
  failure is only a harness failure when the expectation was a successful run.
- **Report writing itself fails**: inability to produce the run report or preserve
  raw outputs is a run failure, not a warning.

## Requirements *(mandatory)*

### Functional Requirements

**Oracle identity & verification**

- **FR-001**: The system MUST pin the parity oracle for this certification line to
  exactly @devcontainers/cli version 0.87.0, recorded in one authoritative place
  that the harness reads (not prose-only documentation).
- **FR-002**: Before any live parity comparison executes, the system MUST verify
  that the oracle binary that will be invoked reports exactly the pinned version,
  and MUST fail the run with a message naming the found version (or absence) and
  the required version when they differ.
- **FR-003**: Every parity run report MUST record the oracle version that was
  verified and the resolution path of the oracle binary actually used.

**No silent passes**

- **FR-004**: A live parity check MUST NOT report a passing result unless the
  intended comparison against the verified oracle actually executed to completion.
- **FR-005**: Each of the following conditions MUST produce a failing result (with a
  cause-specific message) for any parity check that encounters it: missing Docker in
  a Docker-required check, missing or wrong-version oracle, missing/unreadable
  required fixture, oracle process crash where success was expected, malformed or
  unparseable oracle output, and failure of normalization itself.
- **FR-006**: The harness MUST NOT contain conditional early-return, ignore/skip
  annotations, or fallback code paths that convert an unmet prerequisite into a
  passing or silently-absent result for any parity check. Environments that should
  not run parity MUST express that by not selecting the checks, never by the checks
  self-disabling.
- **FR-007**: Oracle invocations MUST be time-bounded — 2 minutes for
  configuration-only invocations, 15 minutes for container-lifecycle invocations;
  exceeding the bound is a failure attributed to the oracle invocation, with the
  partial evidence preserved.

**Unified execution contract**

- **FR-008**: All parity test binaries and all corpus runners — including the Tier 1
  read-configuration runner, the merged-configuration runner, the Tier 1c error
  runner, and the observable-state comparisons — MUST be executable and gateable
  exclusively through the repository's sanctioned test-execution system, with
  resource grouping declared for every profile per repository policy. The corpus
  runners MUST be ported to native test binaries that share the harness's single
  normalization and waiver code; the standalone runner scripts are retired.
  Convenience entry points (such as the existing manual make target) MAY remain
  only as thin aliases that delegate to the sanctioned contract with no gating
  logic of their own.
- **FR-009**: The Tier 1 and merged-configuration corpus runners MUST report failure
  (nonzero outcome) for any unexpected reference-only property, Deacon-only
  property, value mismatch, process failure, or normalization failure, identifying
  each offending case.
- **FR-010**: A detected difference MAY pass only when linked to an explicit
  characterized-divergence or waiver record that identifies the case, the expected
  difference, and the rationale; the pass MUST reference that record in the report.
- **FR-011**: Characterized-divergence and waiver records MUST be validated each
  run: a record that matches no existing case, or whose expected difference is no
  longer observed, MUST fail the run until the record is updated or removed.
- **FR-012**: Existing divergence-encoding mechanisms (the error-corpus expectation
  labels and the observable-state known-divergence lists) MUST be brought under a
  single documented waiver model — one schema and one shared loader/validator, with
  records living adjacent to the cases they waive and each record carrying case
  identity, expected difference, and rationale — so the same difference cannot be
  waived in one runner and fatal in another without an explicit, recorded reason.
- **FR-013**: Checks that do not invoke the oracle MUST NOT be named, grouped, or
  reported as parity comparisons; they MUST be reclassified as internal-consistency
  checks.

**Truthful reporting & CI**

- **FR-014**: No test lane or profile may execute parity checks whose prerequisites
  it does not provision; exclusion MUST be by explicit selection so the lane status
  shows the checks as not run rather than passed.
- **FR-015**: The lane designated for parity certification MUST provision the pinned
  oracle and all other prerequisites, and MUST fail when any prerequisite is absent.
  It runs nightly on the main branch, on manual dispatch, and automatically on
  changes that touch the parity harness, the corpus/fixtures, or the oracle pin; it
  is not a required check on unrelated changes.
- **FR-016**: Every parity run MUST produce a report identifying: which binaries and
  corpus cases executed; which registered items were omitted and why; the verified
  oracle version; and, per check, whether it was a live oracle comparison or a
  snapshot/replay evaluation.
- **FR-017**: Live-comparison checks and any snapshot/replay checks MUST be
  distinguishable by name, by grouping, and in reports and CI status.
- **FR-018**: Failure to produce the run report or to preserve required artifacts
  MUST itself fail the run.

**Normalization & diagnostics**

- **FR-019**: Exactly one shared definition of output equivalence (normalization
  rules) MUST be used by all parity binaries and corpus runners for a given
  comparison type; per-runner private variants MUST be eliminated.
- **FR-020**: The raw, unnormalized outputs of both Deacon and the oracle MUST be
  preserved for every comparison that fails (and be reproducibly obtainable for
  passes), locatable from the run report.

**Mandatory acceptance tests**

- **FR-021**: The feature MUST include acceptance tests demonstrating that each of
  the following produces a failing result: wrong oracle version, missing oracle,
  missing Docker in a Docker-required lane, an injected output difference, malformed
  oracle output, and a normalization failure.
- **FR-022**: The feature MUST maintain a registry of all parity binaries and corpus
  runners, and include an acceptance test proving every registered entry is executed
  under the sanctioned contract — failing when an entry is missing, ignored,
  unselected in the certification lane, or short-circuits without comparing.
- **FR-023**: An acceptance check MUST verify the harness contains no ignored parity
  tests, no silent-return-as-pass paths, and no permissive fallbacks, so regressions
  of the trust property are caught automatically.
- **FR-024**: A corpus runner MUST fail when it discovers zero cases or fewer cases
  than its registered expectation.

### Key Entities

- **Oracle**: The pinned reference implementation (@devcontainers/cli 0.87.0) that
  defines correct behavior for this certification line. Attributes: pinned version,
  resolution path of the invoked binary, verified-version evidence per run.
- **Parity Check**: A single comparison unit (a test within a parity binary, or a
  corpus case within a runner). Attributes: identity, prerequisites (oracle, Docker,
  fixtures), mode (live vs snapshot/replay), outcome with cause on failure.
- **Parity Registry**: The authoritative enumeration of all parity binaries and
  corpus runners that constitute claimed coverage; the basis for the
  registry-completeness acceptance test.
- **Characterized Divergence / Waiver Record**: An explicit, reviewable record that
  a specific difference is intentional. Attributes: case identity, expected
  difference, rationale, validity status (active vs stale).
- **Corpus Case**: A fixture-driven input with an expected comparison outcome
  (match, both-reject, characterized divergence).
- **Run Report**: The per-run account of executed/omitted checks, oracle identity,
  per-check mode and outcome, waivers applied, and pointers to preserved raw
  outputs.
- **Normalization Definition**: The single shared statement of which output
  differences are semantically irrelevant for a given comparison type.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero silent passes: across the entire parity surface, fault-injection
  testing of every guaranteed failure mode (wrong/missing oracle, missing Docker,
  missing fixture, oracle crash, malformed output, normalization failure) produces a
  failing result in 100% of injections.
- **SC-002**: Oracle certainty: 100% of passing live parity runs contain recorded
  evidence that the invoked oracle was verified as version 0.87.0; a run against any
  other version cannot produce a pass.
- **SC-003**: Coverage honesty: every parity run report enumerates executed and
  omitted checks such that the count of registered checks neither executed nor
  explicitly accounted for is zero; the registry-completeness test passes on the
  certification lane and fails when any registered check is removed from selection.
- **SC-004**: Gateable corpus runners: 100% of corpus runners (Tier 1,
  merged-configuration, Tier 1c errors, observable-state) return a failing outcome
  when given an unwaived difference, verified by injection for each runner.
- **SC-005**: Single equivalence verdict: identical output pairs evaluated by any
  two runners of the same comparison type yield the same verdict in 100% of sampled
  cases; the number of independent normalization implementations per comparison
  type is exactly one.
- **SC-006**: Diagnosability: for 100% of failing comparisons, raw outputs from both
  sides are preserved and reachable from the run report without re-running.
- **SC-007**: Truthful lane status: no test lane reports a passing status while
  containing parity checks that executed without their prerequisites; lanes without
  prerequisites show the checks as not selected.

## Assumptions

- "Nextest-only execution contract" is interpreted as: the repository's sanctioned
  parallel test-execution system is the sole gateable entry point for all parity
  checks. Per clarification, the corpus runners that today are standalone scripts
  are ported to native test binaries under that system (not wrapped), and the
  standalone scripts are retired.
- The certification lane (the lane that provisions the oracle and asserts live
  parity) is created in CI as part of this feature; today no CI lane provisions the
  oracle at all, so "prevent CI from claiming coverage" requires both truthful
  selection in existing lanes and the certification lane where live parity genuinely
  runs and gates, on the cadence fixed in the Clarifications section.
- Snapshot/replay checks do not currently exist in the harness (all comparisons are
  live). The naming/reporting distinction (FR-017) is specified so that any future
  replay mode cannot be conflated with live certification; this feature does not
  require building a replay mode.
- The two existing parity-named checks that never invoke the reference CLI are in
  scope only for reclassification/renaming (FR-013), not for gaining oracle
  comparisons.
- Local developer machines without the oracle installed remain supported: parity
  checks are simply not selected in default developer test flows; explicitly
  selecting them without prerequisites yields failures by design.
- Upgrading the oracle to a newer version in the future is a deliberate
  re-certification event (update the single pinned-version record and re-run), not
  something this feature automates.
- The existing per-fixture expectation labels in the error corpus already satisfy
  the "characterized divergence" concept and will be preserved, subject to
  unification under the single waiver model (FR-012).
- During planning, the current repository, the official containers.dev
  specification, and the pinned reference implementation will be inspected before
  changes are proposed, per the project's spec-parity principle.

## Dependencies

- Availability of @devcontainers/cli 0.87.0 as an installable artifact for the
  certification lane and for developers running live parity locally.
- Docker availability in the certification lane for Docker-required checks.
- The repository's existing test-grouping policy (all-profile resource-group
  declarations) governs how parity checks are grouped and selected.

## Out of Scope

- Adding new behavioral parity coverage (new commands or comparison dimensions
  beyond what the registry-completeness and fault-injection tests require).
- Building a snapshot/replay comparison mode.
- Supporting multiple oracle versions simultaneously or automating oracle upgrades.
- Changing Deacon's actual CLI behavior to close any real divergence the hardened
  harness may reveal (each such divergence becomes its own follow-up: fix or
  characterize).
- Feature-authoring parity (permanently out of scope for the project).
