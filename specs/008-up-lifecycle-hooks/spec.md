# Feature Specification: Up Lifecycle Semantics Compliance

**Feature Branch**: `008-up-lifecycle-hooks`  
**Created**: 2025-11-27  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec lifecycle semantics for up subcommand: enforce onCreate->updateContent->postCreate->postStart->postAttach; resume reruns postStart/attach only; --skip-post-create skips all hooks + dotfiles; prebuild stops after updateContent and skips dotfiles; updateContent reruns on subsequent prebuilds. Acceptance: marker-based integration for order/resume; skip-post-create prevents hooks/dotfiles; prebuild behavior verified; dotfiles ordering correct. Use 008 as the numerical identifier."

## Clarifications

### Session 2025-11-27

- Q: How should prebuild runs persist lifecycle markers for onCreate/updateContent relative to a subsequent normal `up`? → A: Prebuild uses separate/temporary markers so a later normal `up` reruns onCreate and updateContent before postCreate/postStart/postAttach.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Fresh up enforces lifecycle order (Priority: P1)

Developers running `up` on a new dev environment need lifecycle hooks and dotfiles to execute exactly once and in the spec-defined order so their container is configured predictably.

**Why this priority**: Incorrect ordering breaks environment setup and causes user-visible failures; it is the foundation for all other behaviors.

**Independent Test**: Run `up` on a fresh environment with all lifecycle hooks and dotfiles configured; verify each phase executes once in the required sequence without skipping or reordering.

**Acceptance Scenarios**:

1. **Given** a fresh environment with onCreate, updateContent, postCreate, postStart, postAttach hooks and dotfiles configured, **When** the user runs `up` without special flags, **Then** the system executes onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach in that order with no phase skipped.
2. **Given** lifecycle completion markers are recorded during a fresh run, **When** the run finishes, **Then** the system captures which phases ran to support future resume decisions without reordering.

---

### User Story 2 - Resume only reruns runtime hooks (Priority: P2)

Developers resuming an existing dev environment want only runtime hooks to rerun so they can reconnect quickly without repeating heavy setup or reapplying dotfiles.

**Why this priority**: Prevents wasted time and unintended side effects on resume while still honoring runtime setup needs.

**Independent Test**: After a successful initial `up`, run `up` again in resume mode; verify only postStart and postAttach run, and earlier phases plus dotfiles are skipped.

**Acceptance Scenarios**:

1. **Given** a prior `up` completed with markers present, **When** the user resumes the environment, **Then** only postStart and postAttach execute, and onCreate, updateContent, postCreate, and dotfiles do not rerun.
2. **Given** a previous run failed before postStart finished, **When** the user resumes, **Then** the system finishes any incomplete earlier phases in order before allowing postStart/postAttach to run.

---

### User Story 3 - Controlled skipping for flags and prebuild (Priority: P3)

Operators running `up` with skip or prebuild options need the command to honor limited-scope execution so automation remains fast and predictable.

**Why this priority**: CI/prebuild pipelines and skip flags must avoid unintended lifecycle side effects while still preparing content correctly.

**Independent Test**: Invoke `up` with `--skip-post-create` and with prebuild mode; verify only permitted phases run, dotfiles are skipped as required, and updateContent reruns on repeated prebuilds.

**Acceptance Scenarios**:

1. **Given** `--skip-post-create` is supplied, **When** the user runs `up`, **Then** base phases needed for creation and content updates run, but postCreate, postStart, postAttach, and dotfiles are all skipped.
2. **Given** prebuild mode is used, **When** the user runs `up` for a prebuild, **Then** onCreate and updateContent run, the command exits before postCreate, no dotfiles run, and updateContent executes again on subsequent prebuild runs.

---

### Edge Cases

- Resume is attempted when lifecycle markers are missing or corrupted: the system defaults to a full ordered run to avoid skipping required phases.
- Dotfiles are configured but either prebuild mode or `--skip-post-create` is active: dotfiles are explicitly skipped with clear reporting so users understand why they did not apply.
- A lifecycle hook fails mid-sequence: subsequent phases do not run, and the next invocation resumes from the earliest incomplete phase while preserving ordering guarantees.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `up` command MUST execute lifecycle phases for a fresh environment strictly in the order onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach with no reordering or omission.
- **FR-002**: The system MUST record completion markers per lifecycle phase to enforce ordering on subsequent invocations and to inform resume decisions.
- **FR-003**: On resume after a successful initial run, `up` MUST skip onCreate, updateContent, postCreate, and dotfiles, and rerun only postStart followed by postAttach.
- **FR-004**: If a prior run ended before postStart completed, a subsequent `up` MUST rerun any incomplete earlier phases in order before executing postStart and postAttach.
- **FR-005**: When `--skip-post-create` is provided, `up` MUST perform required base setup (container creation and content update) and MUST skip postCreate, postStart, postAttach, and dotfiles.
- **FR-006**: In prebuild mode, `up` MUST stop after completing updateContent, MUST skip dotfiles, postCreate, postStart, and postAttach, and MUST rerun updateContent on every prebuild invocation regardless of prior runs.
- **FR-007**: The command MUST present a summary indicating which phases executed or were skipped (including dotfiles) so users can verify lifecycle behavior.
- **FR-008**: Prebuild executions MUST keep lifecycle markers isolated from normal `up` runs so that a subsequent standard `up` reruns onCreate and updateContent before proceeding to postCreate, postStart, and postAttach.

### Key Entities *(include if feature involves data)*

- **Lifecycle Phase State**: Represents each phase’s completion status and any skip reason to guide ordering and resume behavior.
- **Invocation Context**: Captures the mode (`up`, resume, `--skip-post-create`, prebuild) and determines which phases are eligible to run.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In fresh `up` runs with hooks and dotfiles configured, 100% of observed runs execute onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach in that order without reordering or omission.
- **SC-002**: In resume runs after a completed initial `up`, 100% of runs skip onCreate, updateContent, postCreate, and dotfiles, while postStart and postAttach execute successfully.
- **SC-003**: With `--skip-post-create`, 100% of runs complete required base setup while skipping postCreate, postStart, postAttach, and dotfiles, with clear reporting of skipped phases.
- **SC-004**: In prebuild mode, 100% of runs stop after updateContent, skip dotfiles and all post* hooks, and updateContent reruns on every repeated prebuild invocation.

## Assumptions

- Lifecycle hooks and dotfiles are already defined in the dev container configuration; this feature controls when they run.
- The skip flag is `--skip-post-create`, and its use signals intent to bypass post-lifecycle steps entirely.
- Prebuild mode is a dedicated invocation intended to prepare content without starting interactive lifecycle phases.
