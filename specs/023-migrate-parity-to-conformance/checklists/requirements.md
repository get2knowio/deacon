# Specification Quality Checklist: Migrate Parity Assets into the Declarative Conformance System

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-24
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Validation Notes

**Iteration 1 findings and fixes**:

1. *Implementation leakage*: initial phrasing named concrete repository artifacts (crate names, file paths, test-binary names). Rewritten to describe roles — "comparison program", "authoritative runner", "record" — so the spec states WHAT must be conserved, not WHICH files carry it. Concrete counts survive only in Assumptions A-001, explicitly labelled a survey rather than a requirement.
2. *Unfalsifiable baseline*: "no coverage lost" was initially asserted without a defined subject. Added User Story 1 plus FR-001–FR-005 making the enumerated, committed, deterministic inventory the authoritative subject, and FR-045 preventing the report from being satisfied by editing the baseline downward.
3. *Deacon-only handling*: the original phrasing permitted "normalize away" without constraint. Tightened via FR-020/FR-021 (named, scoped, justified rules only; no blanket category removal) and SC-007/SC-008.
4. *Untestable "temporarily"*: compatibility retention had no verifiable bound. Resolved by the user's cut-over decision — FR-031 removes superseded surfaces in the same change that introduces their replacement, and FR-032 requires commands, automation, and documentation to move in lockstep with a dangling-reference check. Both are checkable and neither depends on a calendar.
5. *Deletion criteria*: "equivalent or stricter" was ambiguous between message text and outcome. Pinned in A-002 and made per-case checkable in FR-033–FR-038 with SC-011/SC-014.
6. *Missing residual path*: an assertion inexpressible as data had no defined home, which would have forced either silent loss or scope creep. Added the Residual Record entity, FR-013, and the retirement-blocking rule.

**Iteration 2**: all items re-checked and passing. Three scope decisions were resolved with the user rather than left as markers — see the Clarifications section of spec.md.

**Iteration 3** (`/speckit.clarify`): five further ambiguities resolved and integrated as FR-049–FR-056, SC-016–SC-017, and three Key Entity refinements:

1. *Undefined denominator*: "baseline assertion unit" had no granularity rule, making SC-001/SC-005 unfalsifiable. Pinned by FR-049 to the finest independently reported outcome, with whole-test-function fallback.
2. *Identity vs content hash*: "stable identity" did not say what it was stable against. FR-050 separates the hand-authored slug (identity) from content hashing (evidence staleness only).
3. *"Exactly one source" ambiguity*: readable as "merge the three exception mechanisms", which would have destroyed the disposition distinctions. FR-025 reworded to *location*; FR-051 forbids merging mechanisms.
4. *Residual vs gap*: unstated whether residuals block release certification. FR-054/FR-055 make them non-blocking-but-queued and require a named capability plus tracked follow-up; SC-016 makes it verifiable in both directions.
5. *Unverifiable gating*: no statement of where the checks run or what happens to them at completion. FR-052/FR-053 require hermetic checks in every lane and removal of the drift gate on completion; SC-017 measures it.

**Iteration 4** (`/speckit.analyze`): cross-artifact analysis found 1 critical and 3 high issues, all remediated:

1. *Critical — zero task coverage for exception migration*: FR-024–FR-028 and FR-051 (migrating the 16 characterized exceptions, preserving self-invalidation, direction, scope, and the one-location-not-one-mechanism rule) had no tasks at all. Added six tasks to US2.
2. *High — SC-005 contradicted FR-013*: SC-005 required `cases + variants ≥ 111`, which residuals make unsatisfiable. Reworded to accounting completeness (`migrated + deduplicated + residual + retired = units`) and propagated to data-model.md and contracts/migration-report.md.
3. *High — FR-031 violated by phase structure*: deletions sat in Phase 8 while reference updates sat in Phase 9, two PRs apart. Added an explicit FR-031 binding note making per-carrier reference updates part of each deletion change, with Phase 9 reduced to the end-state sweep.
4. *High — Constitution VII gap*: ~14 new hermetic test binaries with no nextest group configuration. Added a Foundational task and a Polish verification task.
5. *Medium* — unreferenced-fixture orphan detection (FR-010) untested; case→channel resolution (FR-008) unvalidated; `blockedCarrier` required but unrepresentable for the 33 external entries; `assertion`-only authoring in the freeze task; terminology drift after the `coverage.rs` collision rename (contract file renamed to `migration-report.md`).
6. *Medium — FR-021 vs registered rules*: `strip_intentional_labels` matches label *prefixes*, an open-ended set, so registering it would violate FR-021. FR-021 now distinguishes finite enumerated removal sets (permitted with justification) from open-ended predicates (must be narrowed or replaced), and data-model.md §6 requires an enumerated `removes` list on every `drop` rule.

Post-remediation: 106 tasks, sequential and unique, zero dangling references, **56/56 functional requirements explicitly cited**.

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
- The Assumptions A-001 survey figures are deliberately non-normative; planning must replace them with the enumerated baseline artifact (FR-001–FR-004) before any migration work begins.
