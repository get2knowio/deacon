# Feature Specification: Migrate Parity Assets into the Declarative Conformance System

**Feature Branch**: `023-migrate-parity-to-conformance`
**Created**: 2026-07-24
**Status**: Draft
**Input**: User description: "Create a feature specification for migrating Deacon's existing parity scripts, Rust parity binaries, fixtures, normalizers, and characterized divergences into the declarative conformance system without losing behavioral coverage."

## Overview

Deacon currently proves its fidelity to the upstream DevContainers specification through **two overlapping systems**:

1. **The parity harness** — a set of hand-written comparison programs, a valid-configuration corpus, an error corpus, a merged-configuration runner, container observable-state comparisons, an external pinned real-world corpus manifest, and a normalization/diff vocabulary that classifies each difference.
2. **The declarative conformance system** — a data-owned record where a case is *data* (operations, oracle type, expected observations) that a single shared runner executes, backed by a registry of behaviors, channels, contexts, dispositions, waivers, and committed snapshots.

The second system was introduced to *replace* the first, but the replacement is currently partial: most conformance cases are still **pointers** at hand-written comparison programs rather than executable data, comparison rules exist in more than one place, and some characterized exceptions and corpus sources live outside the registry.

This feature completes the migration. Its defining constraint is **conservation**: every behavioral assertion, every fixture, every diagnosable failure class, and every characterized exception that exists today must still exist afterwards — provably, via a before-and-after report — and superseded machinery may only be deleted once that proof holds.

This is a **migration**, not an expansion. New conformance coverage is out of scope except where it is the direct expression of coverage that exists today.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Establish and freeze the migration baseline (Priority: P1)

A maintainer preparing the migration needs an authoritative, machine-checkable inventory of what exists **today**, derived from the repository itself rather than from prose, prior reports, or remembered counts. Without this, "no coverage lost" is unfalsifiable.

**Why this priority**: Every other story is measured against this artifact. A migration that starts from an approximate baseline can silently drop coverage and still appear complete. Nothing else can be verified first.

**Independent Test**: Run the baseline enumeration on the unmodified repository. It produces a complete inventory of comparison programs, discovered corpus cases, per-program assertion units, fixtures, normalization rules, difference classes, and characterized exceptions — and re-running it on an unchanged repository produces a byte-identical result.

**Acceptance Scenarios**:

1. **Given** the repository at the migration start commit, **When** the baseline is enumerated, **Then** it lists every live comparison program, every internal assertion unit inside each program, every discovered corpus case in the valid corpus and the error corpus, every merged-configuration case, every observable-state comparison, every entry in the pinned real-world corpus manifest, every fixture directory, and every characterized exception record.
2. **Given** an enumerated baseline, **When** it is enumerated again with no repository changes, **Then** the two results are identical, and the baseline is committed as a version-controlled artifact.
3. **Given** a committed baseline, **When** a comparison program, corpus case, fixture, or exception record is added or removed without updating the baseline, **Then** the baseline check fails and names the specific drifted item.
4. **Given** the baseline, **When** it is compared against the counts asserted anywhere in existing documentation or prior reports, **Then** the enumerated result is authoritative and any documentation disagreement is corrected rather than the baseline being adjusted to match.

---

### User Story 2 - Migrate every case with a stable identity and a complete mapping (Priority: P1)

A maintainer migrating a case needs each unit of behavioral coverage to acquire a **stable identity** that survives the move, and to be mapped to the behavior(s) it exercises, the context(s) in which it applies, and the observable channel(s) it inspects. No test may exist without a mapping, and no fixture may exist without a consumer.

**Why this priority**: Identity and mapping are what make the declarative system a *record* rather than a pile of tests. Without them, coverage cannot be counted, diffed, or certified, so the no-loss proof is impossible.

**Independent Test**: For each baseline item, resolve its identity in the migrated registry and confirm the mapping is non-empty and resolvable. Confirm that no migrated fixture is unreferenced and no migrated case lacks a behavior.

**Acceptance Scenarios**:

1. **Given** a baseline assertion unit, **When** it is migrated, **Then** it has exactly one stable case identity, and that identity does not change when the case's descriptive annotations change.
2. **Given** a migrated case, **When** the registry is validated, **Then** the case resolves to at least one existing behavior, zero or more existing contexts, and at least one existing observable channel — all by identifier, with dangling identifiers rejected.
3. **Given** a migrated fixture, **When** the registry is validated, **Then** at least one case references it; an unreferenced fixture fails validation as an orphan.
4. **Given** a comparison program or corpus case present in the baseline, **When** the registry is validated, **Then** it maps to at least one migrated case; a baseline item with no mapped case fails validation as an orphan test.
5. **Given** a corpus fixture that is migrated, **When** the mapping is checked, **Then** the correspondence between the pre-migration fixture and its post-migration fixture is one-to-one — no fixture is silently merged into another and none is dropped.
6. **Given** a case that cannot yet be expressed as data because the shared runner lacks a required capability, **When** the registry is validated, **Then** the case is recorded with its identity, its mapping, and an explicit residual disposition naming the missing capability — it is never left unrecorded, and it never counts as migrated.

---

### User Story 3 - Represent duplicate coverage as variants, not as new behaviors (Priority: P2)

Several existing programs assert the **same behavior** under different surface conditions — the same corpus exercised in plain and merged-configuration modes, the same container state examined by more than one comparison, the same configuration parsed through different entry paths. A maintainer needs these expressed as *variants of one behavior* so that the behavior denominator reflects distinct behaviors rather than repeated tests.

**Why this priority**: An inflated denominator makes conformance percentages meaningless and makes real gaps look proportionally smaller. It is a correctness property of the record, but the record is still usable while it is being deduplicated.

**Independent Test**: Migrate a known duplicate pair (the same corpus case compared in two modes) and confirm the behavior count is unchanged while the case/variant count increases by one.

**Acceptance Scenarios**:

1. **Given** two baseline assertion units that exercise the same normalized behavior under different conditions, **When** they are migrated, **Then** they become two cases (or variants) mapped to the *same* behavior, and the behavior denominator does not increase.
2. **Given** two cases mapped to the same behavior, **When** they differ in context, oracle type, or observed channel, **Then** those differences are recorded on the cases and are individually reportable.
3. **Given** the migrated registry, **When** duplicate detection runs, **Then** any two behaviors whose descriptions and mappings are indistinguishable are reported as a suspected duplicate requiring merge or explicit differentiation.
4. **Given** a behavior with multiple variants, **When** coverage is reported, **Then** the report shows both the behavior-level count and the variant-level count so neither number is mistaken for the other.

---

### User Story 4 - Preserve full failure diagnosis, including deacon-only data (Priority: P2)

An engineer investigating a failure needs the migrated system to distinguish, at minimum, the same result classes it distinguishes today: data present only in the reference, data present only in deacon, differing values, one side accepting while the other rejects, and process-level failures (reference crash, timeout, malformed output, unusable normalization, missing fixture, missing container runtime). Critically, **deacon-only data must not be dismissed as serialization noise**: each such difference must be compared, normalized away by an explicitly named rule, or characterized as an accepted difference.

**Why this priority**: Diagnostic resolution is the harness's practical value. A migration that collapses six failure causes into "failed" is a regression in engineering capability even if case counts are conserved.

**Independent Test**: Inject one instance of each result class against the migrated system and confirm each is reported with its own distinguishable classification and the evidence needed to act on it.

**Acceptance Scenarios**:

1. **Given** a difference where the reference emits data deacon omits, **When** the case runs, **Then** the result is classified as reference-only and names the specific observable path.
2. **Given** a difference where deacon emits data the reference omits, **When** the case runs, **Then** the result is classified as deacon-only and is either (a) a reported difference, (b) removed by a named, documented normalization rule that states why the data is not observable behavior, or (c) covered by an explicit accepted-difference record — and never silently discarded by a blanket rule.
3. **Given** a difference in a shared value, **When** the case runs, **Then** the result is classified as a value difference and reports both sides.
4. **Given** one side accepting input the other rejects, **When** the case runs, **Then** the result records the direction (which side was stricter) and the direction is part of what any accepted-difference record must match.
5. **Given** a reference process that crashes, hangs past its bound, or emits unparseable output, **Given** a missing fixture, **Given** an unavailable container runtime, or **Given** a normalization that cannot produce a comparable value, **When** the case runs, **Then** each is reported as its own distinct cause and never as a pass, a skip, or a generic failure.
6. **Given** any of the above, **When** the result is reported, **Then** the unmodified raw observations from both sides are preserved and locatable alongside the normalized ones.
7. **Given** the migrated system, **When** the set of normalization rules is enumerated, **Then** every rule is named, is scoped to specific fields or channels, and carries a stated justification; no rule blanket-removes a category of observable data.

---

### User Story 5 - Prove no coverage was lost with a before-and-after report (Priority: P2)

A reviewer approving the migration needs a single report that compares the frozen baseline to the migrated registry and demonstrates that **no case, behavior, observable channel, or error-path assertion disappeared** — with any intentional reduction called out explicitly and justified rather than implied by an unchanged total.

**Why this priority**: This report is the acceptance evidence for the whole feature and the precondition for deletion (User Story 7). It is P2 only because it is meaningless before the baseline (P1) and the mapping (P1) exist.

**Independent Test**: Produce the report against the migrated registry; then remove one migrated case and confirm the report fails and names exactly the lost item.

**Acceptance Scenarios**:

1. **Given** the frozen baseline and the migrated registry, **When** the report is produced, **Then** it accounts for every baseline item as one of: migrated (with its new identity), deduplicated into a named existing case, or explicitly and justifiably retired.
2. **Given** the report, **When** any baseline item is unaccounted for, **Then** the report fails and names the item, its origin, and what it asserted.
3. **Given** the report, **When** a behavior, observable channel, or characterized exception present in the baseline has no counterpart afterwards, **Then** the report fails naming the missing element and its category.
4. **Given** the report, **When** an error-path assertion (a case whose expectation is a rejection, a diagnostic, or a non-zero exit) is present in the baseline, **Then** its counterpart must assert the same rejection direction and the same diagnostic expectation, and a weakened counterpart fails the report.
5. **Given** the report, **When** it is produced twice from unchanged inputs, **Then** the output is identical, contains no timestamps or absolute paths, and is reviewable as a version-controlled diff.
6. **Given** the report, **When** a reviewer reads it, **Then** it states both the pre-migration and post-migration totals for cases, variants, behaviors, channels, fixtures, and characterized exceptions, and no total may silently shrink.

---

### User Story 6 - Cut every invocation surface over to the authoritative runner in one step (Priority: P3)

Every way the migrated coverage can be invoked — the maintainer-facing command, the certification lane, and anything documentation tells a reader to run — is cut over to the authoritative runner as part of this migration, with no parallel compatibility layer left behind. A maintainer needs the cut-over to land in lockstep across command, automation, and documentation, so that at no point does an invocation surface exist that decides anything for itself or that points at something no longer there.

**Why this priority**: This is a usability and single-source-of-truth concern rather than a coverage concern; the migration's conservation guarantee holds regardless of how the coverage is invoked. It is sequenced last because it depends on the replacement being proven (User Story 7).

**Independent Test**: Invoke every documented surface and confirm each produces results attributable solely to the authoritative runner; then confirm by inspection that no surface contains comparison, normalization, waiver, or verdict logic, and that no reference anywhere points at a removed surface.

**Acceptance Scenarios**:

1. **Given** an invocation surface, **When** it is invoked, **Then** it performs selection and reporting only, and delegates all execution, observation, normalization, comparison, and verdict decisions to the single authoritative runner.
2. **Given** an invocation surface, **When** the authoritative runner's rules change, **Then** its results change with them, with no second place requiring an edit.
3. **Given** the migrated system, **When** comparison rules are enumerated, **Then** exactly one implementation of each rule exists; a second implementation of any comparison or normalization rule fails validation.
4. **Given** a superseded invocation surface, **When** the migration lands, **Then** it is removed in the same change that introduces its replacement — no parallel compatibility surface is retained.
5. **Given** the migration has landed, **When** commands, automation lanes, and documentation are checked, **Then** every reference resolves to a surface that exists; a reference to a removed surface fails validation.

---

### User Story 7 - Retire superseded machinery only after proving equivalent-or-stricter outcomes (Priority: P3)

A maintainer deleting a superseded comparison program, script, or normalization path needs a defined, checkable condition for deletion: the replacement must produce, over the **full baseline**, outcomes that are equivalent or stricter — never more permissive. Any newly tolerated difference must be an explicit, justified record, not an artifact of the migration.

**Why this priority**: Deletion is the last step and the most dangerous one. Everything of value is already achieved once the replacement exists and is proven; deleting the old path is cleanup that must not be rushed.

**Independent Test**: Run both the old and the new path over the entire baseline and compare outcome-by-outcome; confirm the deletion gate blocks when a single case's outcome is more permissive under the replacement.

**Acceptance Scenarios**:

1. **Given** a superseded path and its replacement, **When** both run over the full baseline, **Then** deletion is permitted only if, for every case, the replacement's outcome is identical to or stricter than the superseded path's outcome.
2. **Given** a case where the replacement passes and the superseded path failed or reported a difference, **When** the gate evaluates it, **Then** deletion is blocked until the loosening is either fixed or recorded as an explicit, justified accepted difference.
3. **Given** a case where the replacement detects a difference the superseded path missed, **When** the gate evaluates it, **Then** this is permitted, is reported as a strictness improvement, and the newly detected difference is characterized rather than suppressed.
4. **Given** an unsatisfied deletion condition, **When** deletion is attempted, **Then** it is blocked with the specific unsatisfied condition named.
5. **Given** all deletion conditions satisfied for a superseded path, **When** it is deleted, **Then** the coverage report still accounts for every baseline item it previously carried.
6. **Given** the migrated system, **When** characterized exceptions are enumerated, **Then** they exist in exactly one place with explicit dispositions; any second source of exceptions is removed or is prevented from being introduced.

---

### Edge Cases

- **A baseline program asserts several unrelated things in one unit.** It must be decomposed into separately identified cases; a single migrated case that silently absorbs several distinct assertions fails the mapping check.
- **A baseline assertion cannot be expressed as data with today's runner capabilities.** It is recorded with an explicit residual disposition naming the missing capability, is excluded from the "migrated" count, and blocks deletion of the program that currently carries it.
- **Two characterized exceptions describe the same difference with different justifications.** Migration must detect the conflict and require a single reconciled record; conflicting sources of exceptions must be impossible to reintroduce.
- **A characterized exception no longer reproduces.** It must be reported as stale rather than quietly retained — the migration must not convert self-invalidating exceptions into permanent ones.
- **A pre-migration exception has no counterpart concept afterwards.** It must be explicitly dispositioned (fixed, re-characterized, or reclassified as unobservable), never dropped.
- **The pinned external real-world corpus cannot be fetched** (network unavailable, upstream moved). Its entries must still have identities and mappings in the record; unavailability is reported as its own cause, never as a pass and never as a silent skip.
- **The valid corpus and the error corpus contain a case with the same name.** Identity must remain unique and unambiguous across corpora.
- **A migrated case's descriptive annotations change.** Its identity and any committed reference evidence must be unaffected.
- **A superseded program covers both migrated and residual cases.** It cannot be deleted while any residual case depends on it, even if every other case has migrated.
- **The baseline itself is edited to make the report pass.** Baseline changes must be reviewable as a diff and justified; the report must not be satisfiable by lowering the baseline.
- **A normalization rule removes a deacon-only field.** The rule must name the field or channel and state why the data is not observable behavior; a rule that removes an unbounded category fails validation.
- **An invocation surface is replaced while some coverage it carries is still residual.** The replacement must cover the residual items too, or the surface cannot be cut over — the cut-over is atomic, so a partially covered replacement blocks it.
- **Documentation refers to a surface removed by the cut-over.** This fails validation; commands, automation, and documentation move in the same change.

---

## Clarifications

### Session 2026-07-24

- **Q**: Must this feature eliminate every case that is merely a pointer at a hand-written comparison program, or may some remain? → **A**: Every baseline item receives an identity and a mapping, but items that cannot be expressed as data are recorded as explicit residual records naming the missing runner capability, and those residuals block deletion of whatever carries them. Full expressibility is not a precondition for completing this feature; silent omission is. (FR-013, edge cases.)
- **Q**: How is the pinned external real-world corpus manifest treated? → **A**: As a coverage source, not a comparison program. Its entries get identities and mappings; fetching stays an opt-in, out-of-band step, and unavailability is its own reported cause, never a pass or a silent skip. (A-003.)
- **Q**: Which invocation surfaces survive the migration, and for how long? → **A**: None as a compatibility layer. Superseded surfaces are removed in the same change that introduces their replacement, with commands, automation lanes, and documentation updated in lockstep. There is no transitional delegation period. (User Story 6, FR-029–FR-032, SC-010.)
- **Q**: What exactly counts as one baseline unit, and what form does a case identity take? → **A**: A baseline unit is the finest thing the current harness reports an independent outcome for — each per-case result a comparison program emits — plus each test function that emits no per-case result, which counts as exactly one unit. Identity is a hand-authored stable slug; content hashing is used only for evidence staleness, never as identity. (FR-049, FR-050, Key Entities.)
- **Q**: Does "characterized exceptions exist in exactly one source" mean one location or one mechanism? → **A**: One authoritative location — the conformance registry. The three existing mechanisms keep distinct roles and are not merged: a waiver characterizes a reproducing divergence; an extension records a deliberate capability with no reference equivalent; a scoped allowed-difference binds one observable path to exactly one waiver or extension. (FR-051, Key Entities.)
- **Q**: How are the baseline artifact and the coverage report gated, and what happens to them when the migration completes? → **A**: Both are committed data verified by hermetic checks that run in every lane, needing no container runtime or network. The baseline is frozen at the migration start commit. On completion both are retained as version-controlled evidence and the baseline drift check is removed, since it would otherwise permanently forbid changing the machinery this migration retires. (FR-052, FR-053.)
- **Q**: Do residual records block release certification? → **A**: No. A residual is a representation debt, not a coverage gap — the behavior remains covered by its existing carrier. Residuals are reported as a named queue, block deletion of their carrier, and each must name a specific missing runner capability and a tracked follow-up. Gaps remain blocking and are a distinct concept. (FR-054, FR-055.)
- **Q**: How is each preserved failure class proven independently reproducible? → **A**: By extending the existing hermetic fault-injection pattern (stub reference executables and synthetic evidence) to the declarative runner — one hermetic case per difference classification and per process-level cause. No second verification mechanism is introduced. (FR-056.)

---

## Requirements *(mandatory)*

### Functional Requirements

#### Baseline

- **FR-001**: The system MUST produce a complete inventory of the pre-migration state, enumerated from the repository rather than from documentation or prior reports.
- **FR-002**: The inventory MUST cover: every live comparison program; every internal assertion unit within each program; every case discovered in the valid configuration corpus; every case in the error corpus; every merged-configuration case; every observable container-state comparison; every entry in the pinned real-world corpus manifest; every fixture directory; every normalization rule; every difference classification; and every characterized exception record.
- **FR-003**: The inventory MUST be deterministic — identical inputs produce identical output, free of timestamps, absolute paths, and machine-specific values.
- **FR-004**: The inventory MUST be committed as a version-controlled artifact and MUST be re-verifiable against the repository, failing and naming any drifted item.
- **FR-005**: Where existing documentation or prior reports state coverage counts that disagree with the enumerated inventory, the inventory MUST be treated as authoritative and the documentation MUST be corrected.
- **FR-049**: A **baseline unit** MUST be defined as the finest granularity for which the pre-migration system reports an independent outcome — each per-case result a comparison program emits — and additionally, each test function that emits no per-case result MUST count as exactly one baseline unit. Enumeration MUST NOT group several independently reported outcomes into one unit, and MUST NOT split one reported outcome into several.
- **FR-052**: The baseline artifact MUST be frozen at the migration start commit, and both the baseline drift check and the coverage report MUST be verifiable by hermetic checks that require no container runtime and no network, so they gate every change.
- **FR-053**: On completion of the migration, the baseline artifact and the final coverage report MUST be retained as version-controlled evidence, and the baseline drift check MUST be removed — it must not persist as a permanent constraint on the machinery this migration retires.

#### Identity and mapping

- **FR-006**: Every baseline assertion unit MUST receive exactly one stable identity in the migrated record.
- **FR-007**: An identity MUST be stable against changes to descriptive annotations and MUST NOT be reused for a different assertion.
- **FR-008**: Every migrated case MUST map to at least one behavior, to zero or more contexts, and to at least one observable channel, all by resolvable identifier.
- **FR-009**: Dangling identifiers in any mapping MUST fail validation.
- **FR-010**: Every fixture MUST be referenced by at least one case; unreferenced fixtures MUST fail validation as orphans.
- **FR-011**: Every baseline item MUST map to at least one migrated case or an explicit residual record; unmapped baseline items MUST fail validation as orphan tests.
- **FR-012**: Fixture migration MUST be one-to-one — no pre-migration fixture may be silently merged, split, or dropped, and each correspondence MUST be recorded.
- **FR-013**: A case that cannot be expressed as data MUST be recorded with its identity, its mapping, and a residual disposition naming the missing runner capability; it MUST NOT count as migrated and MUST block retirement of whatever currently carries it.
- **FR-050**: Case identity MUST be a hand-authored stable slug recorded in the registry. Content-derived hashing MUST be used only for detecting staleness of committed evidence and MUST NOT serve as identity, so that editing a case's content does not change what the case *is*.
- **FR-054**: A residual record MUST NOT block release certification — the behavior it covers remains covered by its existing carrier, so a residual is a representation debt rather than a coverage gap. Residuals MUST remain distinct from gaps, which continue to block.
- **FR-055**: Every residual record MUST name a specific missing runner capability and reference a tracked follow-up item, and the coverage report MUST enumerate all residuals as a named queue with their blocked carriers.

#### Deduplication

- **FR-014**: Baseline units exercising the same normalized behavior MUST map to that one behavior, represented as distinct cases or variants, without increasing the behavior count.
- **FR-015**: Variants of one behavior MUST record what distinguishes them (context, oracle type, observed channel, or input shape) and MUST be individually reportable.
- **FR-016**: The system MUST detect and report behaviors that are indistinguishable in description and mapping as suspected duplicates requiring merge or explicit differentiation.
- **FR-017**: Coverage reporting MUST present behavior-level and variant-level counts separately.

#### Failure classification

- **FR-018**: The system MUST classify and report, as separately distinguishable results: reference-only data, deacon-only data, differing values, accept-versus-reject disagreement (with direction), and process-level failures.
- **FR-019**: Process-level failures MUST be further distinguished as at least: reference failure, reference timeout, malformed output, normalization failure, missing fixture, and unavailable container runtime.
- **FR-020**: Deacon-only data MUST NOT be assumed to be serialization noise. Each instance MUST be either reported as a difference, removed by a named field- or channel-scoped rule with a stated justification, or covered by an explicit accepted-difference record.
- **FR-021**: Every normalization rule MUST be named, scoped, and justified. No rule may blanket-remove a category of observable data. A rule that removes a **finite, enumerated** set of named fields is field-scoped and permitted with justification; a rule whose removal set is **open-ended** — a prefix match, a pattern, a type predicate such as "every empty value" — is a category removal and MUST be either narrowed to an enumerated set or replaced.
- **FR-022**: Raw, unmodified observations from both sides MUST be preserved separately from normalized observations for every compared case.
- **FR-023**: No result may be reported as a pass or silently skipped when the comparison did not actually occur.

#### Exceptions and dispositions

- **FR-024**: Every characterized exception in the baseline MUST be migrated into the conformance record with an explicit disposition.
- **FR-025**: After migration, characterized exceptions MUST exist in exactly one authoritative **location** — the conformance record; conflicting or duplicate sources MUST be removed and their reintroduction MUST fail validation.
- **FR-051**: "Exactly one source" constrains location, NOT mechanism. The distinct exception mechanisms MUST be preserved with their separate roles — one characterizing a reproducing divergence, one recording a deliberate capability with no reference equivalent, and one binding a single observable path to exactly one of the former two. Migration MUST map each baseline exception to exactly one mechanism, and MUST NOT merge the mechanisms into a single undifferentiated form.
- **FR-026**: Migrated exceptions MUST remain self-invalidating: an exception whose difference no longer reproduces MUST be reported as stale.
- **FR-027**: A migrated exception MUST preserve the direction and scope of the difference it characterizes; an exception that would tolerate a broader difference than its pre-migration form MUST fail validation.
- **FR-028**: A baseline exception with no post-migration counterpart concept MUST be explicitly dispositioned rather than dropped.

#### Invocation surfaces

- **FR-029**: Every invocation surface MUST delegate all execution, observation, normalization, comparison, and verdict decisions to the single authoritative runner, performing selection and reporting only.
- **FR-030**: No invocation surface may implement an independent comparison, normalization, waiver, or pass/fail rule; a second implementation of any such rule MUST fail validation.
- **FR-031**: Superseded invocation surfaces MUST be removed in the same change that introduces their replacement — no parallel compatibility surface may be retained.
- **FR-032**: Commands, automation lanes, and documentation MUST be updated in lockstep with the cut-over; a reference to a removed surface MUST fail validation.

#### Equivalence and retirement

- **FR-033**: Before any superseded path is deleted, the replacement MUST be run over the full baseline and its outcomes compared case-by-case with the superseded path's outcomes.
- **FR-034**: Deletion MUST be permitted only when, for every baseline case, the replacement's outcome is equivalent to or stricter than the superseded path's.
- **FR-035**: A case where the replacement is more permissive MUST block deletion until it is fixed or recorded as an explicit, justified accepted difference.
- **FR-036**: A case where the replacement detects a previously undetected difference MUST be permitted, reported as a strictness improvement, and the new difference MUST be characterized rather than suppressed.
- **FR-037**: A blocked deletion MUST name the specific unsatisfied condition.
- **FR-038**: After deletion, the coverage report MUST still account for every baseline item the deleted path carried.

#### No-coverage-loss report

- **FR-039**: The system MUST produce a before-and-after coverage report comparing the frozen baseline to the migrated record.
- **FR-040**: The report MUST account for every baseline item as migrated (with its new identity), deduplicated into a named case, or explicitly and justifiably retired.
- **FR-041**: The report MUST fail, naming the specific item and its category, when any case, behavior, observable channel, fixture, characterized exception, or error-path assertion has no counterpart.
- **FR-042**: The report MUST verify that error-path assertions preserve rejection direction and diagnostic expectation; a weakened counterpart MUST fail.
- **FR-043**: The report MUST be deterministic, free of timestamps and absolute paths, and reviewable as a version-controlled diff.
- **FR-044**: The report MUST state pre- and post-migration totals for cases, variants, behaviors, channels, fixtures, and characterized exceptions.
- **FR-045**: The report MUST NOT be satisfiable by editing the baseline downward; baseline changes MUST be separately reviewable and justified.

#### Mandatory acceptance coverage

- **FR-046**: Automated acceptance tests MUST exist for each of: baseline enumeration and determinism; one-to-one fixture migration; behavior deduplication; preserved failure classification (each class independently); invocation-surface delegation and cut-over completeness; stricter-difference detection; and the no-coverage-loss report.
- **FR-047**: Each acceptance test MUST be able to fail — verified by demonstrating the failure it is designed to catch (a dropped case, a merged fixture, an inflated behavior count, a collapsed failure class, a surface with its own comparison rule, a reference to a removed surface, a loosened outcome, an unaccounted baseline item).
- **FR-048**: Acceptance tests that verify record structure MUST run without a container runtime or network so that they gate every change.
- **FR-056**: Independent reproducibility of each difference classification and each process-level cause MUST be demonstrated by extending the existing hermetic fault-injection approach — substituting stub reference executables and synthetic evidence — to the authoritative runner, with one hermetic case per class. A second, parallel verification mechanism MUST NOT be introduced.

### Key Entities

- **Baseline Inventory**: The frozen, version-controlled enumeration of pre-migration coverage, captured at the migration start commit. Attributes: item identity, origin, category (program, corpus case, fixture, normalization rule, difference class, exception), asserted expectation, difference-classification capability.
- **Baseline Unit**: One entry of the inventory — the finest granularity for which the pre-migration system reports an independent outcome, or one whole test function where no per-case outcome is reported. The denominator of the no-loss proof.
- **Case Identity**: The hand-authored stable slug a unit of coverage carries across the migration. Stable against annotation and content changes; unique across corpora; never reused. Distinct from the content hash, which detects evidence staleness only.
- **Behavior**: A normalized statement of what the tool must do. The denominator of conformance. Exercised by one or more cases or variants.
- **Variant**: A case that exercises an already-recorded behavior under different conditions. Increases case count, never behavior count.
- **Context**: The conditions under which a case applies (platform, architecture, container runtime, reference version).
- **Observable Channel**: A distinct surface where behavior is observed (process exit status, standard output, standard error, structured output, filesystem, file content, image, process graph, injected process, timing, container state).
- **Difference Classification**: The result vocabulary — reference-only, deacon-only, value difference, accept-versus-reject with direction, and each distinct process-level cause.
- **Normalization Rule**: A named, scoped, justified transformation applied identically to both sides before comparison. Never a blanket removal.
- **Characterized Exception**: The umbrella term for a recorded, justified tolerance. It has three non-merged mechanisms, all living in the one authoritative record: a *divergence characterization* (a reproducing difference, carrying scope, direction, justification, and expiry, self-invalidating when the difference stops reproducing); a *deliberate capability record* (a behavior with no reference equivalent); and a *scoped tolerance* (one observable path bound to exactly one of the other two).
- **Residual Record**: A baseline item that could not be expressed as data, carrying its identity, mapping, the named missing capability, and a tracked follow-up. Blocks retirement of its carrier and is enumerated as a queue in the coverage report. Never blocks release certification, and is distinct from a coverage gap.
- **Invocation Surface**: Any way the migrated coverage can be run — a maintainer-facing command, an automation lane, or a documented instruction. Performs selection and reporting only; decides nothing.
- **Coverage Report**: The deterministic before-and-after accounting that proves conservation and gates deletion. Referred to throughout this spec by its conceptual name; the delivered artifact is named the **migration report** to keep it distinct from the pre-existing behavior-coverage evaluation, which answers a different question ("is every in-profile behavior covered?" rather than "did anything get lost in the move?").
- **Retirement Condition**: The checkable predicate that must hold before a superseded path may be deleted.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of baseline items are accounted for in the coverage report as migrated, deduplicated, recorded as residual, or explicitly retired — zero unaccounted items.
- **SC-002**: 100% of migrated cases resolve to at least one behavior, at least one observable channel, and zero dangling identifiers.
- **SC-003**: Zero orphan tests (baseline items with no mapped case) and zero unmapped fixtures (fixtures no case references).
- **SC-004**: Fixture correspondence is one-to-one across the migration, with a recorded mapping for every fixture and zero silent merges, splits, or drops.
- **SC-005**: The behavior count after migration is less than or equal to the behavior count before it, while **every** pre-migration assertion unit is accounted for by exactly one disposition — `migrated + deduplicated + residual + retired` equals the baseline unit count, with zero unaccounted. (Raw case count alone is not the measure: a unit recorded as residual conserves its coverage without producing a case, so requiring `cases ≥ units` would contradict FR-013.)
- **SC-006**: Every difference classification and every process-level failure cause that the baseline can diagnose is independently reproducible against the migrated system — zero collapsed classes.
- **SC-007**: Every deacon-only difference in the baseline is, after migration, either reported, covered by a named justified rule, or covered by an explicit accepted-difference record — zero instances discarded by a blanket rule.
- **SC-008**: Every normalization rule is named, scoped, and justified — zero unscoped or unjustified rules.
- **SC-009**: Characterized exceptions exist in exactly one authoritative location, each mapped to exactly one of the preserved mechanisms, and every baseline exception has a post-migration counterpart with an explicit disposition — zero conflicting locations, zero merged mechanisms, zero silently dropped exceptions.
- **SC-010**: Every invocation surface delegates all decisions to the single authoritative runner — zero surfaces containing independent comparison rules, zero parallel compatibility surfaces retained, and zero references (in commands, automation, or documentation) pointing at a removed surface.
- **SC-011**: Over the full baseline, zero cases have a more permissive outcome under the replacement than under the superseded path, and any strictness improvements are enumerated in the report.
- **SC-012**: The baseline enumeration and the coverage report each produce byte-identical output on repeated runs from unchanged inputs.
- **SC-013**: All seven mandated acceptance areas have automated tests, and each has been demonstrated to fail when the condition it guards is violated.
- **SC-014**: No superseded script, program, or normalization path is deleted while any unsatisfied retirement condition remains — verified by the gate blocking on a deliberately unsatisfied condition.
- **SC-015**: A reviewer can determine, from the coverage report alone and without reading source code, what moved where and what (if anything) was intentionally retired and why.
- **SC-016**: Every residual record names a missing capability and a tracked follow-up, appears in the report's residual queue with its blocked carrier, and blocks zero releases — verified by certification passing with residuals present and failing with a gap present.
- **SC-017**: The baseline drift check, the coverage report check, and every failure-classification acceptance test complete without a container runtime or network, so all of them gate every change rather than only the certification lane.

---

## Assumptions

- **A-001**: The migration is measured against the repository state at this feature's branch point. An initial survey at that point found: ten live comparison programs and two hermetic guard programs; twenty-five valid-corpus case directories and nine error-corpus case directories; a merged-configuration mode over the same valid corpus; fifteen observable container-state comparisons across two programs; thirty-three pinned entries in the external real-world corpus manifest; thirty-one recorded conformance cases of which twenty-five are pointers at hand-written programs and six are declarative; twenty-five behaviors; eleven observable channels; ten characterized exceptions; and two declarative fixtures. **These figures are a survey, not the baseline**: the authoritative baseline is the artifact produced under FR-001–FR-005 during planning, and any disagreement resolves in favor of the enumerated artifact.
- **A-002**: "Equivalent or stricter" is evaluated per case on the outcome, not on message text. Identical outcomes are equivalent; a difference detected only by the replacement is stricter; a difference detected only by the superseded path is more permissive and blocks deletion.
- **A-003**: The external real-world corpus manifest is a **coverage source**, not a comparison program. Its entries are given identities and mappings, but fetching remains an opt-in, out-of-band step; unavailability is a distinct reported cause and never a pass or a silent skip.
- **A-004**: Guard programs that verify the record's own structure (registry agreement, fault injection) are part of the baseline as *record-integrity coverage*. They are conserved but are not required to become data-driven cases.
- **A-005**: Descriptive annotations (notes, justifications, tolerance references) are excluded from case identity, so annotating a case never invalidates committed reference evidence.
- **A-006**: The migration introduces no new behavioral coverage beyond the direct expression of existing coverage. Newly detected differences arising from increased strictness are characterized as they surface, not designed in advance.
- **A-007**: This feature does not change deacon's user-facing command surface. All migration machinery remains development-only.

---

## Out of Scope

- Adding conformance coverage for behaviors not exercised by the current baseline.
- Fixing the deacon behaviors that the migrated record newly characterizes as divergent (these become their own tracked work items).
- Re-pinning the upstream specification revision or the reference implementation version.
- Changing the release gate's blocking criteria.
- Extending the shared runner with capabilities beyond those needed to express existing coverage; capabilities that existing coverage requires are in scope, and coverage requiring anything further is recorded as residual.

---

## Dependencies

- **D-001**: The declarative conformance record, its validation rules, its shared runner, and its committed-evidence mechanism exist and are the migration target.
- **D-002**: The single authoritative runner is the only permitted implementation of execution, observation, normalization, comparison, and verdict logic.
- **D-003**: Live comparison against the reference implementation requires the pinned reference and a container runtime, available only in the dedicated certification lane; structural validation of the record must remain independent of both.
- **D-004**: Deletion of superseded paths depends on the coverage report (User Story 5) and the equivalence gate (User Story 7) both passing.
