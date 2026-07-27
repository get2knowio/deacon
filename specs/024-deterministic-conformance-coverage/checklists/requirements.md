# Specification Quality Checklist: Deterministic Conformance Coverage

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-26
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

## Notes

### Validation iteration 1 (2026-07-26)

Two [NEEDS CLARIFICATION] markers were raised, both scope-defining:

1. **FR-004 — environment-dimension activation.** Whether alternative container runtimes
   and non-Linux platforms are *activated* (new certification lanes, real pairwise coverage)
   or *modelled and dispositioned* (obligations reported inactive-environment). This changes
   the size of the feature by roughly an order of magnitude in live-run cost.
2. **Assumption 2 — repair boundary.** Whether newly surfaced non-conformances must be
   fixed before the feature is complete, or characterized and tracked separately. The
   second bounds the feature to evidence; the first couples completion to an unbounded
   amount of repair work.

**Resolutions (2026-07-26)**, folded into the spec; both markers removed:

1. **Model only, one active profile.** The Linux/amd64 default-runtime profile stays the sole
   active one (Assumption 10). Because activating another environment later must remain a
   *data* change rather than a re-modelling, this was strengthened into FR-004a/FR-004b,
   Story 1 scenarios 5–6, and SC-015 — the inactive environments are enumerated and reported
   inactive-environment, so deferring them leaves a visible backlog instead of a blind spot.
2. **Fix only what blocks determinism.** A newly surfaced non-conformance that prevents a
   required case from being deterministic is in scope; everything else is characterized and
   tracked separately (Assumption 2). This matches the precedent from the preceding phase.

Resolution 1 also introduced Assumption 11, making inactive-environment an explicit fifth
reporting bucket — it neither blocks nor counts as covered.

### Deliberate scope decisions recorded during validation

- **Coverage is pairwise plus hand-selected triples, never the Cartesian product** (FR-013,
  FR-016). The triple set is hand-authored so that selection stays a reviewable judgement.
- **`inactive-environment` is a distinct reporting bucket** from covered and from gap (FR-026),
  because collapsing it into either would make a green run in one environment read as
  evidence about another.
- **Report generation is read-only** (FR-063) and development-only (FR-064), preserving the
  existing separation between observing evidence and recording it.
- **Legacy carriers do not satisfy obligations** except while an open residual names them
  (Edge Cases), so retiring a carrier reopens its obligations rather than silently dropping
  them.
