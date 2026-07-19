# Feature Specification: Repository-Owned Conformance Registry

**Feature Branch**: `019-conformance-registry`
**Created**: 2026-07-19
**Status**: Draft
**Input**: User description: "Create a feature specification for a repository-owned conformance registry that provides the authoritative relationship between source requirements, normalized behaviors, contexts, observable outcomes, executable cases, known gaps, and intentional divergences."

## Overview

Deacon's conformance knowledge is currently scattered: waiver records in the parity corpus, a `deacon-stricter` error corpus, "Verified Non-Bugs" prose in contributor docs, intentional-divergence notes in commit messages, and exception lists embedded in test scripts. There is no single authoritative answer to "which spec requirements does Deacon cover, where does it diverge from the reference CLI, and is each divergence deliberate?"

This feature introduces a repository-owned conformance registry: a validated, versioned data set that records the full chain from pinned source material (schema, spec text, CLI surface, observed reference behavior) through normalized behaviors, applicability contexts, and executable test cases, down to expected outcomes — plus explicit records for gaps, waivers, and intentional Deacon extensions. The registry becomes the single source of truth that reports, release certification, and test infrastructure consume, replacing duplicate exception lists in scripts and prose.

## Clarifications

### Session 2026-07-19

- Q: Does a recorded gap satisfy structural validation for an in-profile behavior with no case or waiver? → A: Yes — a case, a waiver, or an explicit linked unresolved-gap record satisfies structural (per-PR) validation; gaps still always block strict certification. This is what makes incremental inventory population possible without hiding anything.
- Q: Is a registry test case a reference to an existing executable test or a new registry-executed artifact? → A: A reference — registry cases are stable-ID records linking to executable tests in the repository's existing test infrastructure; the registry records linkage, context, and expected outcomes and does not execute anything itself.
- Q: How does the registry relate to the existing parity-harness corpus (waiver records, error corpus, `registry.json`)? → A: Parity waiver records and error-corpus expectations migrate into the conformance registry and the parity harness loads them from it; the harness's execution mechanics (oracle resolution, exec, normalization) are unchanged by this feature.
- Q: Stable ID format? → A: Lowercase kebab-case with a record-type prefix (`rev-`, `src-`, `bhv-`, `dim-`, `chan-`, `case-`, `gap-`, `wvr-`, `ext-`, `prof-`); validation enforces the format and prefix/record-type agreement.
- Q: Is strict certification enforced or only manually invokable? → A: Enforced — strict certification is a blocking verification step in the release process; per-PR CI runs structural validation only.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Maintainer validates registry integrity (Priority: P1)

A Deacon maintainer edits the registry (adds a behavior, reclassifies a source unit, records a divergence) and runs registry validation. Validation either passes, or fails with a precise message identifying every integrity violation: a dangling reference, a duplicated stable ID, an orphaned test case, an unclassified source unit, an applicable behavior with no case, waiver, or recorded gap, an expired waiver, a stale source pin, a contradictory disposition, or an unknown observable type.

**Why this priority**: The registry is only authoritative if it cannot silently rot. Every other capability (reporting, certification, seeding) depends on the data being structurally sound, so the validation gate is the foundation and the minimum viable slice.

**Independent Test**: Can be fully tested with fixture registries — one valid, and one per violation class — by running validation and asserting pass/fail plus the specific violation reported. Delivers value on its own: even without reports, a validated registry is a trustworthy inventory.

**Acceptance Scenarios**:

1. **Given** a registry where every record is well-formed, uniquely identified, and fully cross-referenced, **When** validation runs, **Then** it succeeds and reports zero violations.
2. **Given** a registry containing a test case that references a behavior ID that does not exist, **When** validation runs, **Then** it fails and names the case, the missing behavior ID, and the violation class (invalid reference).
3. **Given** a registry where two records share the same stable ID, **When** validation runs, **Then** it fails and identifies both records and the duplicated ID.
4. **Given** a registry containing a test case not linked to any behavior, **When** validation runs, **Then** it fails and identifies the orphaned case.
5. **Given** a source unit present in a source inventory but mapped to no behavior and not explicitly classified as out-of-scope, **When** validation runs, **Then** it fails and identifies the unclassified unit.
6. **Given** a behavior applicable under the active certification profile that has no test case, no waiver, and no linked unresolved-gap record, **When** validation runs, **Then** it fails and identifies the uncovered behavior. **Given** the same behavior with an explicit linked gap record, **Then** structural validation passes and the gap remains visible in reports and blocks strict certification.
7. **Given** a waiver whose expiry date is in the past, **When** validation runs, **Then** it fails and identifies the expired waiver.
8. **Given** a registry whose recorded source revision no longer matches the pinned revision the project declares, **When** validation runs, **Then** it fails and identifies the stale pin.
9. **Given** a behavior whose recorded spec status, reference status, and project decision are mutually contradictory (per the contradiction rules in FR-014), **When** validation runs, **Then** it fails and identifies the behavior and the contradiction.
10. **Given** an expected outcome that names an observable channel type not in the registry's declared set, **When** validation runs, **Then** it fails and identifies the unknown observable type.

---

### User Story 2 - Release manager generates coverage and certification reports (Priority: P2)

Before cutting a release, a release manager generates conformance reports from the registry. A machine-readable report supports tooling and CI gating; a human-readable report lets maintainers and users see, at a glance, how much of the deduplicated behavior surface is conformant, divergent, waived, or uncovered — and trace any behavior from its source material through its contexts and cases to expected outcomes. Strict release certification fails while unresolved gaps remain visible in the registry.

**Why this priority**: Reporting is the payoff that makes the registry worth maintaining — it turns the validated data into release decisions and public conformance claims. It depends on P1's validated data.

**Independent Test**: Can be tested by running report generation against a fixture registry with a known mix of conformant, divergent, waived, gapped, and uncovered behaviors, and asserting both report formats contain the expected counts, classifications, and trace chains — and that strict certification fails while a gap exists and passes when none do.

**Acceptance Scenarios**:

1. **Given** a valid registry, **When** reports are generated, **Then** both a machine-readable and a human-readable report are produced, and generating them twice from the same registry yields byte-identical machine-readable output.
2. **Given** a behavior in the registry, **When** a reader consults either report, **Then** they can trace it from each contributing source unit, through its applicability contexts, to each test case and its expected outcomes per observable channel.
3. **Given** a registry containing behaviors in each of the states conformant, divergent, waived, and uncovered, **When** the summary is generated, **Then** each state is counted separately over deduplicated behaviors (never over raw source-unit counts), and the counts sum consistently with the behavior inventory.
4. **Given** a registry containing at least one unresolved gap, **When** strict release certification is evaluated, **Then** certification fails and the report lists each blocking gap; **When** the last gap is resolved or reclassified as an explicit decision, **Then** strict certification passes.
5. **Given** a behavior covered under waiver, **When** reports are generated, **Then** the waiver's rationale and expiry are visible in the report — waived coverage is never presented as conformant coverage.

---

### User Story 3 - Contributor records a divergence with a three-axis disposition (Priority: P3)

A contributor discovers that Deacon behaves differently from the reference CLI for some behavior. They record it in the registry with three separate judgments: how the behavior relates to the written specification (conformant, nonconformant, unspecified, not applicable), how it relates to the observed reference implementation (aligned, divergent, unknown, not applicable), and what the project has decided (follow spec, align with reference, Deacon extension, intentional divergence, unresolved gap). The registry accepts precise combinations — e.g. "spec-conformant but reference-divergent, intentional divergence" — and rejects contradictory ones.

**Why this priority**: The three-axis model is what elevates the registry above the current binary waiver system, where "different but acceptable" conflates spec violations, reference bugs, and deliberate extensions. It builds on P1's validation but delivers distinct value: defensible, auditable conformance claims.

**Independent Test**: Can be tested by recording behaviors with each meaningful combination of the three axes and asserting that valid combinations are accepted, contradictory combinations are rejected by validation, and reports render each axis distinctly.

**Acceptance Scenarios**:

1. **Given** a behavior where Deacon follows the spec but the reference deviates from it, **When** the contributor records spec status "conformant", reference status "divergent", and project decision "follow spec", **Then** the registry accepts it and reports show all three judgments separately.
2. **Given** a behavior the spec does not address, **When** the contributor records spec status "unspecified", reference status "aligned", and project decision "align with reference", **Then** the registry accepts it.
3. **Given** a Deacon-only capability with no spec or reference counterpart, **When** it is recorded as spec status "not applicable", reference status "not applicable", and project decision "Deacon extension", **Then** the registry accepts it and reports list it under extensions, not divergences.
4. **Given** a record claiming spec status "conformant", reference status "aligned", and project decision "unresolved gap", **When** validation runs, **Then** it fails as contradictory (a fully-aligned behavior cannot simultaneously be an unresolved gap).
5. **Given** an attempt to record only a single combined "different but acceptable" judgment without the three separate axes, **When** validation runs, **Then** the record is rejected as incomplete.

---

### User Story 4 - Seed the registry from existing documented divergences (Priority: P4)

The registry ships pre-populated with every Deacon/reference divergence the repository already documents — parity waiver records, the strictness-divergence error corpus, "Verified Non-Bugs" and intentional-divergence notes in contributor documentation, and exception lists embedded in scripts — so the registry is authoritative from day one and those duplicate lists can be retired in favor of registry references.

**Why this priority**: Seeding is what makes the registry the *single* source of truth rather than yet another parallel list. It is last because it consumes the model, validation, and disposition capabilities delivered by P1–P3.

**Independent Test**: Can be tested by enumerating the currently documented divergences in the repository and asserting that each appears in the seeded registry with a complete three-axis disposition, and that the seeded registry passes full validation.

**Acceptance Scenarios**:

1. **Given** the set of divergences currently documented in the repository (parity waivers, strictness error-corpus classifications, documented intentional divergences and verified non-bugs), **When** the seeded registry is inspected, **Then** every one of them is present as a registry record with stable ID, provenance, and three-axis disposition.
2. **Given** the seeded registry, **When** validation runs, **Then** it passes with zero violations.
3. **Given** a divergence formerly documented only in prose or a script exception list, **When** its registry record exists, **Then** the prose/script location either is removed or points to the registry record rather than restating the exception.

---

### Edge Cases

- **One source unit → many behaviors, many source units → one behavior**: a single spec clause may spawn several distinct behaviors, and a schema constraint, a spec clause, and an observed reference behavior may all describe the same behavior. Coverage denominators MUST count deduplicated behaviors; validation MUST NOT require a one-to-one mapping.
- **Behavior applicable in no currently defined profile** (e.g. a Podman-only behavior while only the Linux/Docker profile exists): the behavior is recorded with its applicability condition, is excluded from the active profile's coverage denominator, and is neither "uncovered" nor silently invisible — reports list it as out-of-profile.
- **Waiver expiring between validation runs**: expiry is evaluated against the current date at validation time; a waiver valid yesterday and expired today fails today's validation. No grace period.
- **Contradiction vs. legitimate tension**: "spec: nonconformant + decision: intentional divergence" is legitimate (a deliberate, documented departure); "spec: conformant + reference: aligned + decision: unresolved gap" is contradictory. The contradiction rules must be explicit and enumerable, not vibes-based.
- **Source pin advances**: when a pinned source revision is updated, previously classified source units may vanish or change meaning. Validation detects the stale pin; re-classification of affected units is a required part of the pin bump, not an optional follow-up.
- **Case whose declared context contradicts its behavior's applicability** (e.g. a case tagged Podman for a behavior applicable only to Docker): validation fails the case as inapplicable rather than counting it as coverage.
- **Gap that someone attempts to "cover" with a waiver**: gaps and waivers are distinct — a waiver asserts a characterized, accepted difference; a gap asserts missing knowledge or implementation. A gap MUST NOT be convertible to passing strict certification by wrapping it in a waiver without an explicit project decision recorded.
- **Duplicate seeding**: a divergence documented in two legacy locations (e.g. a waiver file *and* prose) must seed to one registry record, not two.

## Requirements *(mandatory)*

### Functional Requirements

**Registry model & identity**

- **FR-001**: The registry MUST model exactly four source inventories: schema constraints, normative specification clauses, shared CLI surface, and empirically observed reference behaviors. Each source unit MUST belong to exactly one inventory and carry provenance (which pinned source revision it derives from, and where within it).
- **FR-002**: The registry MUST record pinned source revisions (at minimum: the devcontainers specification revision, the schema revision, and the reference CLI oracle version) as first-class records that source units reference.
- **FR-003**: The registry MUST support many-to-many mapping between source units and normalized behavior units: multiple source units may map to one behavior, and one source unit may map to several behaviors. Raw source-unit counts MUST NOT be used as coverage denominators anywhere; all coverage arithmetic operates on deduplicated behaviors.
- **FR-004**: Every registry record (source revision, source unit, behavior, context dimension, context value, observable channel, test case, expected outcome, gap, waiver, extension, profile) MUST have a stable, human-assigned identifier that is unique across the registry and does not change when unrelated records are added, removed, or reordered. Identifiers are lowercase kebab-case with a record-type prefix (`rev-`, `src-`, `bhv-`, `dim-`, `chan-`, `case-`, `gap-`, `wvr-`, `ext-`, `prof-`); validation enforces the format and that the prefix agrees with the record type.
- **FR-005**: The registry MUST model applicability conditions that bind behaviors to context dimensions and values (e.g. platform, architecture, container runtime, oracle version), so that a behavior can be applicable in some contexts and not others.
- **FR-006**: The registry MUST declare a closed set of observable channels (e.g. standard output content, standard error content, exit code, container state, filesystem effect, generated file content), and every expected outcome MUST reference a declared channel. Outcomes referencing undeclared channel types are validation errors.
- **FR-007**: The registry MUST model test cases linked to one or more behaviors, each with declared context and one or more expected outcomes on declared observable channels. A test case is a stable-ID record that references an executable test in the repository's existing test infrastructure; the registry records linkage, context, and expected outcomes and MUST NOT introduce its own test execution engine.
- **FR-008**: The registry MUST model gaps (known missing coverage or unresolved differences), waivers (characterized, accepted differences with required rationale, scope, and expiry), and intentional Deacon extensions (capabilities beyond spec and reference) as distinct record types — never as flavors of one another.

**Disposition (three independent axes)**

- **FR-009**: Every behavior MUST record a spec status from exactly: conformant, nonconformant, unspecified, not applicable.
- **FR-010**: Every behavior MUST record a reference status from exactly: aligned, divergent, unknown, not applicable.
- **FR-011**: Every behavior MUST record a project decision from exactly: follow spec, align with reference, Deacon extension, intentional divergence, unresolved gap.
- **FR-012**: The three axes MUST be stored and reported separately. The registry MUST NOT provide any single combined "different but acceptable" state, and validation MUST reject records that supply fewer than all three axes.
- **FR-013**: Reference status MUST be interpretable per certification profile: a reference status recorded under the initial profile makes no claim about other profiles.
- **FR-014**: Validation MUST enforce an explicit, enumerated set of contradiction rules across the three axes — at minimum: (a) project decision "unresolved gap" contradicts the combination spec "conformant" + reference "aligned"; (b) project decision "Deacon extension" contradicts spec status "conformant" or "nonconformant" (an extension is by definition outside the spec's scope, i.e. unspecified or not applicable); (c) project decision "intentional divergence" contradicts reference status "aligned"; (d) project decision "unresolved gap" is the only decision permitted to coexist with reference status "unknown" for behaviors applicable in the active profile. The full rule set MUST be documented in the registry itself so contributors can predict validation outcomes.

**Certification profiles**

- **FR-015**: The registry MUST define certification profiles as named combinations of context values. The initial profile MUST be: Linux, amd64, Docker, reference oracle `@devcontainers/cli` 0.87.0 (stable).
- **FR-016**: Claims certified under one profile MUST NOT be implied for any other profile. Other platforms, architectures, and Podman MUST be representable only as separate future profiles; the registry model MUST support adding them without restructuring existing records.
- **FR-017**: Behaviors whose applicability conditions exclude the active profile MUST be excluded from that profile's coverage denominator and reported as out-of-profile rather than uncovered.

**Validation**

- **FR-018**: Registry validation MUST fail (with a violation class and the offending record identified) on each of: (a) references to non-existent record IDs; (b) duplicate stable IDs; (c) test cases linked to no behavior; (d) source units mapped to no behavior and not explicitly classified out-of-scope; (e) behaviors applicable in the active profile with no test case, no waiver, and no linked unresolved-gap record (an explicit gap record satisfies structural validation; it never satisfies strict certification); (f) waivers past their expiry date; (g) recorded source pins that do not match the project's declared pinned revisions; (h) contradictory dispositions per FR-014; (i) expected outcomes with undeclared observable types; (j) test cases whose declared context is incompatible with their behavior's applicability.
- **FR-019**: Validation MUST report all violations found in a run, not stop at the first.
- **FR-020**: Gaps MAY be characterized (described, scoped, linked to behaviors) but MUST remain visible in every report and MUST cause strict release certification to fail while unresolved. No record type, including waivers, may hide a gap. An explicit linked gap record satisfies structural validation coverage for a behavior (FR-018e) precisely so gaps are recorded rather than hidden — visibility, not concealment, is the trade.

**Reporting**

- **FR-021**: The system MUST generate both a machine-readable coverage report and a human-readable coverage report from a validated registry.
- **FR-022**: Reports MUST support full traceability: source unit → behavior → applicability context → test case → expected outcome, navigable in both report forms.
- **FR-023**: Reports MUST summarize, over deduplicated behaviors within the active profile: conformant, divergent, waived, and uncovered counts, plus gaps and extensions, with waived coverage always distinguished from conformant coverage.
- **FR-024**: Report generation MUST be deterministic: the same registry content yields byte-identical machine-readable output regardless of when or where it is generated (no embedded timestamps or environment-dependent ordering).
- **FR-025**: The system MUST distinguish a strict certification evaluation (fails on any unresolved gap or uncovered applicable behavior) from ordinary report generation (always succeeds on a valid registry and describes the current state). Strict certification MUST run as a blocking verification step in the release process; per-change CI runs structural validation only.

**Seeding & single source of truth**

- **FR-026**: The initial registry content MUST include every Deacon/reference divergence currently documented in the repository — parity waiver records, the strictness-divergence error corpus classifications, documented intentional divergences (including the `extends` ahead-of-spec divergence), and "verified non-bug" divergence notes — each with provenance and a complete three-axis disposition.
- **FR-027**: After seeding, legacy duplicate exception lists in scripts and prose MUST be retired or reduced to pointers at registry records; the registry is the authoritative statement of each divergence. Specifically, parity waiver records and error-corpus expectations migrate into the registry and the parity test infrastructure loads them from it; the parity harness's execution mechanics (oracle resolution, bounded execution, normalization) are unchanged by this feature.
- **FR-028**: Seeding MUST deduplicate: a divergence documented in multiple legacy locations becomes one registry record referencing all its legacy provenance.

**Acceptance testing**

- **FR-029**: Automated acceptance tests MUST cover, at minimum: registry schema validation (valid and each invalid fixture class), traceability (chain navigation from source to outcome), applicability (in-profile vs out-of-profile behavior handling), waiver expiry (valid, expired, and boundary dates), gap handling (visibility and strict-certification blocking), and deterministic report generation (repeat-run byte equality). These tests MUST run in the repository's standard test infrastructure without network access or a container runtime.

**Scope boundary**

- **FR-030**: The registry, its validation, and its reports live entirely inside the Deacon repository. Creating or publishing a separate public conformance repository is out of scope.

### Key Entities

- **Source Revision**: A pinned, immutable identifier of upstream source material (spec revision, schema revision, reference CLI version). Anchors provenance; staleness against project-declared pins is a validation failure.
- **Source Unit**: One atomic item from one of the four source inventories (a schema constraint, a normative clause, a CLI surface element, or an observed reference behavior), with provenance to a Source Revision. Must be mapped to ≥1 behavior or explicitly classified out-of-scope.
- **Behavior Unit**: A normalized, deduplicated statement of externally observable behavior. The unit of all coverage arithmetic. Carries the three-axis disposition and applicability conditions; linked from ≥1 source unit.
- **Applicability Condition**: A predicate over context dimensions/values that determines where a behavior applies (e.g. "runtime = Docker").
- **Context Dimension / Context Value**: Named axes (platform, architecture, runtime, oracle version) and their enumerated values, from which applicability conditions and profiles are composed.
- **Certification Profile**: A named, complete assignment of context values under which coverage and certification are evaluated. Initial: Linux + amd64 + Docker + reference oracle 0.87.0. Profiles are mutually independent.
- **Observable Channel**: A declared type of externally observable effect (stdout, stderr, exit code, container state, filesystem effect, …). Closed set; outcomes must reference declared channels.
- **Test Case**: A stable-ID record referencing an executable test in the repository's existing test infrastructure, linked to ≥1 behavior, with declared context and ≥1 expected outcome. Orphan cases (no behavior) and context-incompatible cases are validation failures.
- **Expected Outcome**: What a test case asserts on one observable channel in one context.
- **Gap**: A recorded absence — missing coverage, unknown reference behavior, or unimplemented behavior. Always visible; blocks strict certification; distinct from waivers.
- **Waiver**: A characterized, accepted difference with required rationale, scope (behaviors/cases), and expiry date. Expired waivers fail validation. Counts as "waived", never "conformant".
- **Deacon Extension**: A deliberate capability beyond both spec and reference (e.g. workspace-trust gating), recorded so it is never misreported as a divergence or gap.

## Assumptions

- The registry is stored as version-controlled data files within the repository, edited by hand and reviewed via normal pull-request flow; no service, database, or UI is implied.
- "Stale source pin" means the revision recorded in the registry differs from the pinned revision the project otherwise declares (currently: devcontainers/spec commit `113500f4` and the parity oracle pin). The registry becomes the authoritative place for these pins after this feature; existing pin locations reference it.
- The initial profile's oracle is `@devcontainers/cli` 0.87.0 as stated in the feature description. If the parity harness's currently pinned oracle version differs, aligning the two pins is part of seeding, and the registry's pin wins going forward.
- Waiver expiry is a calendar date compared against the current date at validation time; expiry semantics are "valid through the stated date".
- Populating the four source inventories exhaustively (every schema constraint, every normative clause) is expected to be incremental; validation enforces integrity of what is recorded plus completeness of classification for recorded units, not instantaneous total enumeration of the upstream spec. Newly enumerated behaviors gain a case, waiver, or explicit gap record in the same change that adds them. The seeded divergences (FR-026) are the mandatory day-one content.
- Human-readable report format (e.g. rendered document in the repository) and machine-readable format (structured data file) are left to planning; the spec constrains only their required content, traceability, and determinism.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every divergence currently documented anywhere in the repository (waiver files, error-corpus classifications, contributor-doc divergence notes) appears exactly once in the registry with a complete three-axis disposition — verified by enumeration, with zero remaining authoritative exception statements outside the registry.
- **SC-002**: Registry validation detects 100% of the enumerated violation classes: for each of the ten failure classes in FR-018, a fixture exhibiting only that violation fails validation naming that class, and the fully valid fixture passes.
- **SC-003**: A maintainer can trace any behavior in the human-readable report from its source material to its test cases and expected outcomes without consulting any file outside the registry and its reports.
- **SC-004**: Generating the machine-readable report twice from the same registry state — on different days or machines — produces byte-identical output.
- **SC-005**: Strict release certification fails whenever at least one unresolved gap or uncovered in-profile behavior exists, and passes on a registry with none — demonstrated by fixtures on both sides of the boundary.
- **SC-006**: Coverage percentages published in reports are computed over deduplicated behaviors: for a fixture where several source units map to one behavior, the denominator counts that behavior once.
- **SC-007**: No claim in any generated report implies coverage of a platform, architecture, or runtime outside the profile it was generated for — verified by report content for the initial Linux/amd64/Docker/0.87.0 profile.
- **SC-008**: Registry validation and report generation complete in under 30 seconds on a developer workstation and run without network access or a container runtime, so they can gate every pull request.
