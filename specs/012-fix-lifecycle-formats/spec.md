# Feature Specification: Fix Lifecycle Command Format Support

**Feature Branch**: `012-fix-lifecycle-formats`
**Created**: 2026-02-21
**Status**: Draft
**Input**: User description: "Fix lifecycle command execution to support all three formats defined by the DevContainer specification — string, array, and object — for all six lifecycle commands in both container and host execution paths."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - String Lifecycle Commands (Priority: P1)

A developer configures a devcontainer with string-format lifecycle commands (e.g., `"postCreateCommand": "npm install"`) and expects them to execute through a shell, preserving existing behavior.

**Why this priority**: String format is the most common usage and must not regress. This is the baseline that currently works.

**Independent Test**: Can be tested by creating a devcontainer.json with a string lifecycle command, running `deacon up`, and verifying the command executes successfully.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json with `"postCreateCommand": "echo hello && echo world"`, **When** the container starts, **Then** both commands execute via shell interpretation and succeed.
2. **Given** a devcontainer.json with a string `initializeCommand`, **When** `deacon up` runs, **Then** the command executes on the host before container creation.

---

### User Story 2 - Array (Exec-Style) Lifecycle Commands (Priority: P1)

A developer configures a devcontainer with array-format lifecycle commands (e.g., `"postCreateCommand": ["npm", "install"]`) and expects them to execute directly without shell wrapping, following exec-style semantics.

**Why this priority**: Array format is defined by the DevContainer spec and is currently broken, producing runtime failures.

**Independent Test**: Can be tested by creating a devcontainer.json with an array lifecycle command, running `deacon up`, and verifying the command executes without shell interpretation.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json with `"postCreateCommand": ["echo", "hello world"]`, **When** the container starts, **Then** `echo` is invoked directly with the single argument `hello world` (no shell splitting).
2. **Given** a devcontainer.json with `"initializeCommand": ["/usr/bin/env", "bash", "-c", "echo test"]`, **When** `deacon up` runs on the host, **Then** the command executes directly without an outer shell wrapper.
3. **Given** a devcontainer.json with `"onCreateCommand": ["npm", "install"]`, **When** the container is created, **Then** `npm` is invoked with `install` as its argument via exec-style invocation, not via `sh -c "npm install"`.

---

### User Story 3 - Object (Parallel) Lifecycle Commands (Priority: P1)

A developer configures a devcontainer with object-format lifecycle commands (e.g., `"postCreateCommand": {"install": "npm install", "build": "npm run build"}`) and expects all named commands to execute concurrently.

**Why this priority**: Object format enables parallel setup workflows, is defined by the DevContainer spec, and is currently broken.

**Independent Test**: Can be tested by creating a devcontainer.json with an object lifecycle command containing multiple entries, running `deacon up`, and verifying all entries execute concurrently.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json with `"postCreateCommand": {"install": "npm install", "build": "npm run build"}`, **When** the container starts, **Then** both commands execute concurrently rather than sequentially.
2. **Given** an object lifecycle command where one entry fails, **When** execution completes, **Then** the entire lifecycle phase reports failure.
3. **Given** an object lifecycle command with mixed value types (`{"shell": "npm install", "exec": ["python", "-m", "setup"]}`), **When** the container starts, **Then** the string value runs via shell and the array value runs exec-style, both concurrently.
4. **Given** a devcontainer.json with `"initializeCommand": {"prep": "mkdir -p .cache", "check": "git status"}`, **When** `deacon up` runs, **Then** both commands execute concurrently on the host.

---

### User Story 4 - All Six Lifecycle Commands Support All Formats (Priority: P2)

A developer can use any of the three formats (string, array, object) with any of the six lifecycle commands: `initializeCommand`, `onCreateCommand`, `updateContentCommand`, `postCreateCommand`, `postStartCommand`, `postAttachCommand`.

**Why this priority**: Format support must be uniform across all lifecycle phases for spec compliance. Prioritized below individual format correctness since fixing the format handling centrally covers all commands.

**Independent Test**: Can be tested by configuring each lifecycle command with each format and verifying correct execution behavior.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json using array format for `onCreateCommand`, **When** the container is created, **Then** the command executes exec-style.
2. **Given** a devcontainer.json using object format for `postStartCommand`, **When** the container starts, **Then** all named commands execute concurrently.
3. **Given** a devcontainer.json using object format for `updateContentCommand`, **When** content update runs, **Then** all named commands execute concurrently.

---

### Edge Cases

- What happens when an array command contains zero elements? The command is treated as a no-op (skipped).
- What happens when an object command contains zero entries? The command is treated as a no-op (skipped).
- What happens when an object value is neither a string nor an array? The entry is skipped with a diagnostic log message.
- What happens when one of several parallel commands in an object fails? The phase waits for all commands to complete, then reports failure.
- What happens when an array contains non-string elements? The command fails with a clear validation error.
- What happens when a lifecycle command value is `null`? It is treated as a no-op (existing behavior).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST execute string-format lifecycle commands through a shell (`/bin/sh -c` in containers, platform shell on host) for all six lifecycle phases.
- **FR-002**: System MUST execute array-format lifecycle commands using exec-style invocation (no shell wrapper) for all six lifecycle phases.
- **FR-003**: System MUST execute object-format lifecycle commands concurrently, running all named entries in parallel for all six lifecycle phases.
- **FR-004**: For object-format commands, the system MUST wait for all concurrent entries to complete before considering the phase done.
- **FR-005**: For object-format commands, if any entry fails, the entire lifecycle phase MUST report failure.
- **FR-006**: Object-format command values MUST themselves support string (shell) and array (exec-style) formats.
- **FR-007**: All three formats MUST work in the container execution path (via container runtime exec).
- **FR-008**: All three formats MUST work in the host execution path (`initializeCommand` runs on the host before container creation).
- **FR-009**: Empty commands (null, empty string, empty array, empty object) MUST be treated as no-ops and skipped without error.
- **FR-010**: Array commands containing non-string elements MUST produce a clear validation error.
- **FR-011**: Object command entries with unsupported value types (not string or array) MUST be skipped with a diagnostic log message.
- **FR-012**: Output from parallel (object) commands MUST be attributable to their named key for diagnostic clarity.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All three lifecycle command formats (string, array, object) execute successfully for all six lifecycle phases without error.
- **SC-002**: Array-format commands execute without shell interpretation — arguments containing shell metacharacters (spaces, quotes, semicolons) are passed literally to the target program.
- **SC-003**: Object-format commands with two or more entries execute concurrently, completing faster than sequential execution would allow for I/O-bound commands.
- **SC-004**: When any entry in an object-format command fails, the lifecycle phase reports failure and the exit status reflects the error.
- **SC-005**: Existing devcontainer configurations using string-format lifecycle commands continue to work identically (zero regressions).
- **SC-006**: All existing lifecycle-related tests continue to pass without modification.
- **SC-007**: New automated tests cover array format and object format execution for at least one lifecycle command in each execution path (container and host).

## Assumptions

- The DevContainer specification (containers.dev) defines the authoritative behavior for all three lifecycle command formats. Deacon's implementation must match this specification.
- Shell interpretation for string commands uses `/bin/sh -c` inside containers and the platform-appropriate shell on the host.
- Exec-style (array) commands bypass the shell entirely, invoking the first element as the executable with remaining elements as arguments.
- Parallel execution of object entries does not require any specific ordering guarantee — all entries start as soon as the phase begins.
- Feature-contributed lifecycle commands follow the same format rules as config-defined lifecycle commands.
