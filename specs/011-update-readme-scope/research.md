# Research: Update README for Consumer-Only Scope

**Branch**: `011-update-readme-scope`
**Date**: 2026-02-20

## Research Summary

This feature is a documentation-only update to `README.md`. Research focused on identifying exactly which sections need changes and verifying no feature-authoring references exist in the current README.

## Decision 1: Scope of Tagline Change

**Decision**: Replace only line 3 (project description) with the new consumer-only positioning. Do not modify the `# deacon` heading or badge block.

**Rationale**: The user's spec requires the positioning "The DevContainer CLI, minus the parts you don't use." to be visible within the first 3 lines. Line 3 is the natural location for a project tagline. The heading and badges are stable infrastructure that must not change.

**Alternatives considered**:
- Adding a separate "About" section below badges — rejected because it pushes the positioning down and fails SC-002 (positioning visible in first 3 lines).
- Adding a subtitle under the heading — rejected because it changes the README structure unnecessarily.

## Decision 2: "In Progress" Table — No Feature-Authoring Rows Exist

**Decision**: No changes needed to the "In Progress" table.

**Rationale**: Reviewed all 6 rows in the current table:
1. Docker Compose profiles — consumer capability
2. Features installation during `up` — consumer capability (installing features, not authoring)
3. Dotfiles (container-side) — consumer capability
4. `--expect-existing-container` — consumer capability
5. Port forwarding — consumer capability
6. Podman runtime — consumer capability

None reference feature authoring. The table is already clean.

**Alternatives considered**: None needed.

## Decision 3: Roadmap Section Wording

**Decision**: Update "Feature system for reusable development environment components" to clarify it refers to feature consumption (installing community features into dev containers), not authoring. Also update the introductory sentence to reflect consumer-only scope.

**Rationale**: The current wording is ambiguous — "Feature system" could imply Deacon provides tools for creating features. Since Deacon has explicitly removed all authoring commands, the Roadmap should be unambiguous about consumer scope.

**Alternatives considered**:
- Removing the "Feature system" bullet entirely — rejected because feature consumption (installation during up/build) is a real shipped capability.
- Leaving the wording as-is — rejected because it contradicts the consumer-only positioning.

## Decision 4: Examples Section — No Changes Needed

**Decision**: Leave the Examples section unchanged.

**Rationale**: The examples reference "Feature System: dependencies, parallelism, caching, and lockfile support" which describes consumer-side behavior (how features are installed, resolved, and cached during `up`/`build`). This is accurate for a consumer-only tool.

**Alternatives considered**:
- Renaming to "Feature Installation" — rejected as unnecessary granularity; the current wording is accurate in context.

## Decision 5: MVP-ROADMAP.md Link Validity

**Decision**: Keep the link to `docs/MVP-ROADMAP.md` unchanged.

**Rationale**: The MVP-ROADMAP.md covers `up` and `exec` commands — purely consumer-side. No feature-authoring content exists in that document. The link remains valid and relevant.

**Alternatives considered**: None needed.
