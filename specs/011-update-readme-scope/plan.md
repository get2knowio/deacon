# Implementation Plan: Update README for Consumer-Only Scope

**Branch**: `011-update-readme-scope` | **Date**: 2026-02-20 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/011-update-readme-scope/spec.md`

## Summary

Update `README.md` to reflect Deacon's consumer-only positioning after the removal of feature-authoring commands (features test, info, plan, package, publish). The update replaces the project tagline, clarifies the feature list to match shipped commands (up, down, exec, build, read-configuration, run-user-commands, templates apply, doctor), cleans the "In Progress" table, and adjusts the Roadmap section — all while preserving existing badges, CI links, installation instructions, structure, and tone.

## Technical Context

**Language/Version**: N/A (Markdown documentation only)
**Primary Dependencies**: N/A
**Storage**: N/A
**Testing**: Manual review + existing test suite pass-through (`make test-nextest-fast`)
**Target Platform**: GitHub README.md rendering
**Project Type**: Single project (documentation change only)
**Performance Goals**: N/A
**Constraints**: Must preserve all badge URLs, CI references, installation instructions byte-identical
**Scale/Scope**: Single file (README.md), ~510 lines

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|-----------|--------|-------|
| I. Spec-Parity | PASS | No CLI behavior changes; documentation aligns with post-010 codebase state |
| II. Keep the Build Green | PASS | Run `make test-nextest-fast` after changes to verify no regressions |
| III. No Silent Fallbacks | N/A | No code changes |
| IV. Idiomatic Safe Rust | N/A | No code changes |
| V. Observability/Output | N/A | No output contract changes |
| VI. Testing Completeness | PASS | No new tests needed; verify existing pass |
| VII. Subcommand Consistency | N/A | No subcommand changes |
| VIII. Executable Examples | PASS | Examples section references feature *consumption*, remains valid |

All gates pass. No violations to justify.

## Project Structure

### Documentation (this feature)

```text
specs/011-update-readme-scope/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── spec.md              # Feature specification
└── checklists/
    └── requirements.md  # Spec quality checklist
```

### Source Code (repository root)

```text
README.md                # Only file modified by this feature
```

**Structure Decision**: This is a documentation-only feature. No source code files are created or modified beyond the root `README.md`.

## Implementation Approach

### Change 1: Update Project Tagline (Line 3)

**Current**: `A fast, Rust-based [Dev Containers](https://containers.dev) CLI.`

**Updated**: Replace with consumer-only positioning including the tagline "The DevContainer CLI, minus the parts you don't use." and a brief description of Deacon as a fast, focused Rust CLI for developers who use dev containers and CI pipelines — not for feature authors.

**Scope**: Lines 1-3 of README.md. Badges block (lines 5-16) MUST remain untouched.

### Change 2: Review "In Progress" Table (Lines 84-97)

**Current table rows**:
1. Docker Compose profiles — consumer, KEEP
2. Features installation during `up` — consumer, KEEP
3. Dotfiles (container-side) — consumer, KEEP
4. `--expect-existing-container` — consumer, KEEP
5. Port forwarding — consumer, KEEP
6. Podman runtime — consumer, KEEP

**Result**: No feature-authoring rows exist. Table is already clean. No changes needed.

### Change 3: Update Roadmap Section (Lines 436-447)

**Current**: "Feature system for reusable development environment components" — ambiguous, could be read as authoring.

**Updated**: Clarify to indicate feature *consumption* (installing and resolving features during container builds), not authoring. Also remove or adjust any wording that implies Deacon covers the full specification (it covers the consumer surface).

### Change 4: Verify Examples Section (Lines 99-108)

**Current**: References "Feature System: dependencies, parallelism, caching, and lockfile support" — this describes feature *consumption* during `up`/`build`, which is accurate.

**Result**: No changes needed. The examples describe consumer workflows.

### Change 5: Verify No Other Stale References

Scan the entire README for any remaining references to feature authoring commands or positioning that conflicts with consumer-only scope. The `grep` for `feature.*(test|info|plan|package|publish|author)` found only the generic word "features" in the "In Progress" table context (consumer usage).

## Complexity Tracking

No constitution violations to justify. This is a minimal-scope documentation update.
