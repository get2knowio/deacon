# Tasks: Compose mount & env injection

**Input**: Design documents from `specs/001-compose-mount-env/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Not explicitly requested; focus on implementation tasks with independent verification via acceptance checks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Confirm baseline compose injection points and data structures.

- [ ] T001 Review current compose override and injection flow in `crates/deacon/src/commands/up.rs` to map removal points for temporary override usage.
- [ ] T002 Inventory compose data structures and command builder capabilities in `crates/core/src/compose.rs` to align with required fields (profiles, env-files, project name, mounts, env).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Establish core data structures and helpers to support all user stories.

- [ ] T003 Update `ComposeProject` and related structs in `crates/core/src/compose.rs` to include external volume references and any missing fields from `specs/001-compose-mount-env/data-model.md`.
- [ ] T004 Add helper in `crates/core/src/compose.rs` or `crates/deacon/src/commands/up.rs` to merge CLI remote env with env-file/service defaults using CLI precedence for the primary service.
- [ ] T005 Ensure compose command builder in `crates/core/src/compose.rs` cleanly threads profiles, env-files, and project naming through all compose invocations (establish single entrypoint for user story tasks).

**Checkpoint**: Foundation ready - user story implementation can now begin.

---

## Phase 3: User Story 1 - Mounts and env applied to primary service (Priority: P1) 🎯 MVP

**Goal**: Inject CLI mounts and remote env into the primary service during `up` without temporary compose overrides.

**Independent Test**: Run `deacon up` with CLI mounts and remote env; verify inside primary service that mounts and env vars are present at startup without creating override files.

### Implementation for User Story 1

- [ ] T006 [US1] Remove temporary override file creation in `crates/deacon/src/commands/up.rs` and rely on native compose injection paths for mounts/env.
- [ ] T007 [P] [US1] Extend compose service preparation in `crates/core/src/compose.rs` (e.g., `ComposeCommand` or related helpers) to apply `additional_mounts` and `additional_env` directly to the primary service before `up`.
- [ ] T008 [US1] Wire remote env merge helper into the `up` flow in `crates/deacon/src/commands/up.rs` so CLI/remote env keys override env-files/service defaults for the primary service.
- [ ] T009 [US1] Ensure CLI mount parsing (including paths/options) in `crates/deacon/src/commands/up.rs` feeds into compose injection without dropping existing service mounts.
- [ ] T019 [US1] Manual/automated validation: primary service sees injected mounts/env at startup; record steps/results in `specs/001-compose-mount-env/quickstart.md`.

**Checkpoint**: User Story 1 fully functional and independently verifiable.

---

## Phase 4: User Story 2 - External volumes preserved and git root aligned (Priority: P2)

**Goal**: Keep external volumes intact and align mountWorkspaceGitRoot with other CLI mounts in compose injection.

**Independent Test**: Run `deacon up` on a project with external volumes and mountWorkspaceGitRoot enabled; confirm data persists via external volumes and Git root mount appears alongside other CLI mounts.

### Implementation for User Story 2

- [ ] T010 [P] [US2] Ensure external volume references from compose configs remain untouched when injecting mounts/env in `crates/deacon/src/commands/up.rs` and `crates/core/src/compose.rs`.
- [ ] T011 [US2] Align `mountWorkspaceGitRoot` handling in `crates/deacon/src/commands/up.rs` with the same mount resolution logic used for other CLI mounts before passing to compose injection.
- [ ] T018 [US2] Verify missing external volume surfaces compose error without bind fallback; document manual check in `specs/001-compose-mount-env/quickstart.md`.

**Checkpoint**: User Story 2 functional and independently verifiable.

---

## Phase 5: User Story 3 - Profiles, env-files, and project naming respected (Priority: P3)

**Goal**: Maintain compose profiles/env-files/project naming while injecting CLI mounts/env.

**Independent Test**: Run `deacon up` with selected profiles/env-files and custom project name while injecting CLI mounts/env; verify only profiled services start and naming/prefixes remain unchanged.

### Implementation for User Story 3

- [ ] T012 [P] [US3] Confirm compose command construction in `crates/core/src/compose.rs` retains project name, env-files, and profiles when additional mounts/env are injected.
- [ ] T013 [US3] Ensure only the primary service receives injected mounts/env while non-target services respect profile selection in `crates/deacon/src/commands/up.rs`.
- [ ] T020 [US3] Validation: profiles/env-files/project naming preserved alongside injected mounts/env; record steps/results in `specs/001-compose-mount-env/quickstart.md`.

**Checkpoint**: User Story 3 functional and independently verifiable.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Final alignment, documentation, and validation.

- [ ] T014 [P] Update `specs/001-compose-mount-env/quickstart.md` to reflect final CLI flags/behavior after injection changes.
- [ ] T015 Verify acceptance flows manually using instructions in `specs/001-compose-mount-env/quickstart.md` and adjust logging/messages in `crates/deacon/src/commands/up.rs` if clarity is needed.
- [ ] T016 [P] Ensure any new compose-related tests or groupings are configured in `.config/nextest.toml` if added during implementation.
- [ ] T017 [P] Measure compose `up` startup time with and without injection; document results in `specs/001-compose-mount-env/quickstart.md`.
- [ ] T021 [P] Add/adjust compose injection tests (unit in `crates/core/tests/compose_injection.rs`, integration in `crates/deacon/tests/compose_up_injection.rs` covering CLI mounts/env, git root mount, external volume preservation) and set nextest overrides in `.config/nextest.toml` (docker-shared for integration).

---

## Dependencies & Execution Order

- Setup (Phase 1) → Foundational (Phase 2) → User Story phases (3→5) → Polish (Phase 6).
- User stories can start after Phase 2; prioritize US1 (MVP) then US2, then US3. US2/US3 can run in parallel once US1 shared helpers are stable.

## Parallel Execution Examples

- In Phase 3, T007 [P] can proceed in parallel with T008 once T006 groundwork is done.
- In Phase 4, T010 [P] can proceed alongside T011 after foundational helpers are in place.
- In Phase 5, T012 [P] can run concurrently with T013 after US1 completion.
- Polish tasks T014 and T016 are parallelizable.

## Implementation Strategy

- Deliver MVP by completing User Story 1 (Phase 3) first to ensure mount/env injection works without overrides.
- Layer User Story 2 to preserve external volumes and align Git root mounts, then User Story 3 to confirm profiles/env-files/project naming remain intact.
- Keep compose helper changes centralized in `crates/core/src/compose.rs` and orchestrate via `crates/deacon/src/commands/up.rs` to avoid per-story divergence.
