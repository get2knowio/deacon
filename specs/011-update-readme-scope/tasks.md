# Tasks: Update README for Consumer-Only Scope

**Input**: Design documents from `/specs/011-update-readme-scope/`
**Prerequisites**: plan.md (required), spec.md (required), research.md

**Tests**: No test tasks included — this is a documentation-only change. Existing test suite (`make test-nextest-fast`) validates no regressions.

**Organization**: Tasks are grouped by user story. All tasks operate on a single file (`README.md`), so parallelism is limited.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Foundational

**Purpose**: Read current state and establish baseline

- [x] T001 Read current README.md and identify all sections requiring changes per plan.md (sections: tagline line 3, Roadmap lines 436-447)

**Checkpoint**: Current README state understood, change targets identified

---

## Phase 2: User Story 1 — New Visitor Understands Deacon's Purpose (Priority: P1) MVP

**Goal**: A first-time visitor immediately understands Deacon is a consumer-only DevContainer CLI.

**Independent Test**: Read the first 3 lines of README.md and verify the positioning "The DevContainer CLI, minus the parts you don't use" is present with supporting text about being for developers who use dev containers and CI pipelines, not feature authors.

### Implementation for User Story 1

- [x] T002 [US1] Replace project tagline on line 3 of README.md — change `A fast, Rust-based [Dev Containers](https://containers.dev) CLI.` to consumer-only positioning per FR-001: include "The DevContainer CLI, minus the parts you don't use." and describe Deacon as a fast, focused Rust CLI for developers who use dev containers and CI pipelines, not for feature authors. Do NOT modify the `# deacon` heading (line 1) or badge block (lines 5-16).
- [x] T003 [US1] Update Roadmap section in README.md (lines 436-447) — change "Feature system for reusable development environment components" to clarify feature *consumption* (installing and resolving community features during container builds). Update the introductory sentence to reflect consumer-only scope per FR-006 and research.md Decision 3.
- [x] T004 [US1] Scan entire README.md for any remaining references to feature-authoring commands (features test, features info, features plan, features package, features publish) per FR-003. Remove or reword any found. Grep for patterns: `features test`, `features info`, `features plan`, `features package`, `features publish`, `feature author`.

**Checkpoint**: Positioning is consumer-only, Roadmap is clarified, no authoring references remain.

---

## Phase 3: User Story 2 — Existing User Sees Accurate Command List (Priority: P1)

**Goal**: The README lists exactly the shipped commands: up, down, exec, build, read-configuration, run-user-commands, templates apply, and doctor.

**Independent Test**: Search README.md for the command list and verify it matches the eight shipped commands exactly. Verify zero references to removed commands.

### Implementation for User Story 2

- [x] T005 [US2] Verify or add an explicit shipped command list in README.md per FR-002 listing exactly: up, down, exec, build, read-configuration, run-user-commands, templates apply, and doctor. If no explicit list exists, add one in an appropriate location (e.g., after Quick Start or as part of the updated positioning). Ensure the list reflects consumer-only surface.

**Checkpoint**: Command list is accurate and complete.

---

## Phase 4: User Story 3 — CI/Automation User Sees Current Status (Priority: P2)

**Goal**: The "In Progress" table contains no feature-authoring rows and accurately reflects current development status.

**Independent Test**: Read the "In Progress" table (lines 84-97) and verify every row describes a consumer capability.

### Implementation for User Story 3

- [x] T006 [US3] Verify "In Progress" table in README.md (lines 84-97) contains no feature-authoring rows per FR-004. Per research.md Decision 2, all 6 current rows are consumer capabilities (Docker Compose profiles, Features installation during up, Dotfiles, --expect-existing-container, Port forwarding, Podman runtime). Confirm no changes needed and document verification.

**Checkpoint**: "In Progress" table verified clean.

---

## Phase 5: User Story 4 — No Broken Links or Stale References (Priority: P2)

**Goal**: All badges, CI links, installation instructions, and internal references remain intact after the update.

**Independent Test**: Diff the badge block (lines 5-16), Install section (lines 18-64), and CI section (lines 474-490) against the original to verify byte-identical preservation.

### Implementation for User Story 4

- [x] T007 [US4] Verify all badge URLs (lines 5-16), installation instructions (lines 18-64), and CI references (lines 474-490) in README.md are unchanged from before any edits per FR-005. Compare against original content to confirm byte-identical preservation.
- [x] T008 [US4] Verify the link to `docs/MVP-ROADMAP.md` (line 97) still resolves to an existing file per research.md Decision 5. Verify the Examples section references (lines 99-108) remain accurate per research.md Decision 4.

**Checkpoint**: All links valid, all preserved sections unchanged.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final validation across all user stories

- [x] T009 Run `make test-nextest-fast` to verify no regressions from documentation changes in README.md
- [x] T010 Final full-document review of README.md: verify structure, tone, and formatting conventions preserved per FR-007. Verify SC-001 through SC-005 all pass. Confirm consumer-only positioning, accurate command list, clean In Progress table, valid links, and no stale references.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Foundational (Phase 1)**: No dependencies — start immediately
- **US1 (Phase 2)**: Depends on Phase 1 — core positioning change
- **US2 (Phase 3)**: Depends on Phase 2 (positioning informs where command list goes)
- **US3 (Phase 4)**: Depends on Phase 1 — verification only, can run after T001
- **US4 (Phase 5)**: Depends on Phase 2 and Phase 3 (must verify after all edits)
- **Polish (Phase 6)**: Depends on all prior phases

### User Story Dependencies

- **User Story 1 (P1)**: Can start after T001 — no dependencies on other stories
- **User Story 2 (P1)**: Can start after US1 edits (T002-T004) since command list placement may depend on updated positioning
- **User Story 3 (P2)**: Can start after T001 — independent verification, no dependencies on US1/US2
- **User Story 4 (P2)**: Must run after all edits (T002-T006) to verify nothing was broken

### Parallel Opportunities

- T006 (US3 verification) can run in parallel with T002-T005 (US1/US2 edits) since it only reads the "In Progress" table which is not being modified
- T007 and T008 (US4 verification) must wait until all edits are complete

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete T001: Read and understand current README
2. Complete T002-T004: Update positioning, Roadmap, remove authoring references
3. **STOP and VALIDATE**: Read first 3 lines — positioning is clear

### Incremental Delivery

1. T001 → Baseline established
2. T002-T004 (US1) → Positioning updated → Validate
3. T005 (US2) → Command list accurate → Validate
4. T006 (US3) → In Progress table verified → Validate
5. T007-T008 (US4) → Links and preservation verified → Validate
6. T009-T010 → Full validation and test suite pass

---

## Notes

- All tasks operate on a single file (`README.md`) so true file-level parallelism is minimal
- US3 (Phase 4) is verification-only — the "In Progress" table is already clean per research
- US4 (Phase 5) is verification-only — ensures edits in US1/US2 didn't break preserved sections
- T009 runs the existing test suite as a regression check, not to test new code
