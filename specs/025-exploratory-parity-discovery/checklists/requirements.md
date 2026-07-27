# Specification Quality Checklist: Exploratory Parity Discovery

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-27
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

**Iteration 1 findings and resolutions:**

1. *Acceptance scenarios missing a **When** clause* — US2 scenarios 4 and 6 stated a
   condition and an outcome without an explicit trigger. Rewritten with explicit
   **Given/When/Then** structure.
2. *Requirements section lacked the template's `### Functional Requirements` heading* —
   the FR groups were promoted under a single `### Functional Requirements` heading with
   `####` subgroups, preserving template section order.
3. *Classification vocabulary risked reading as identifiers* — the six classification values
   are stated in prose form in the requirements and entities so the spec stays free of
   implementation-level token names; the closed set is unambiguous either way.

**Deliberate judgement calls (not defects):**

- The spec names existing project artifacts conceptually ("the deterministic record", "the
  pinned reference", "the normalization definition", "observable channels") rather than by
  file path or crate. This matches the house style of specs 019–024, which describe the
  conformance record as a domain concept. No language, framework, or API is named.
- Every mandatory acceptance test named in the feature request has at least one covering
  acceptance scenario: seed reproduction (US1-1, SC-001), semantic generation (US1-2/3,
  SC-002/003), shrinking (US2-1..4, SC-004), metamorphic failures (US6-4/6, SC-011),
  classification (US4-1, SC-007), deduplication (US4-2/3, SC-006), review-only promotion
  (US5-1..3, SC-008/009), pinned real-world provenance (US7-1/2, SC-012), and
  hermetic/network lane isolation (US3-1..4, SC-013/014).
- Zero [NEEDS CLARIFICATION] markers: every gap in the request had a defensible default,
  each recorded explicitly in the **Assumptions** section (10 entries) so a reviewer can
  overturn a default rather than reverse-engineer it. The two highest-impact defaults are
  Assumption 1 (tiered comparison surface — bounds campaign cost) and Assumption 4
  (version-controlled findings queue — the only way cross-run deduplication and triage
  state can exist).

## Clarification Session — 2026-07-27

Five questions asked and answered; all recommendations accepted. Re-validated after
integration: 0 `[NEEDS CLARIFICATION]` markers, 5 clarification bullets, 69 functional
requirements, 19 success criteria, heading hierarchy intact.

| # | Decision | Where it landed |
|---|----------|-----------------|
| 1 | Findings queue lives outside the registry; never loaded, never an input to certification | FR-034a, US5-8, SC-018, Key Entities, Assumption 4 |
| 2 | Signature = channel + path + difference kind + value-shape class; concrete values excluded | FR-030a, Key Entities |
| 3 | Mutation seeds are committed fixtures only; corpus is a comparison input, never a seed | FR-008a, FR-054a, US7-6, Assumption 9 |
| 4 | Per-campaign admission cap with a reported suppressed count; never fails the campaign | FR-034b, FR-061, US4-7, SC-019 |
| 5 | Pipeline proven by an injected difference; a real promotion is reported, not required | FR-042a, US5-7, SC-016 |

Two of these overturned defaults the initial spec carried. SC-016 previously required a real
discovered-and-promoted behavior — an acceptance criterion no amount of effort can guarantee,
since it depends on the implementations actually disagreeing where the generator reaches. And
the findings queue was described as "version-controlled" without stating that it sits *outside*
the record, which left open the reading that would have let an unreviewed stochastic finding
reach the release gate.

## Notes

- Items marked incomplete require spec updates before `/speckit.clarify` or `/speckit.plan`.
