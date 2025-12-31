# Feature Specification: Exec Subcommand Parity (Close GAP)

**Feature Branch**: `007-exec-subcommand`  
**Created**: 2025-11-16  
**Status**: Draft  
**Input**: User description: "Implement the tasks to close the GAP in the SPEC (exec subcommand)."

## Clarifications

### Session 2025-11-16

- Q: What should happen if multiple container selection inputs are provided (e.g., `--container-id`, `--id-label`, and/or `--workspace-folder`)? → A: Allow multiple; precedence `--container-id` > `--id-label` > `--workspace-folder`.
- Q: When both CLI `--remote-env` and config `remoteEnv` set the same variable, which wins? → A: Merge order: shell → config → CLI (CLI wins).
- Q: If both config `remoteUser` and CLI `--user` are provided, which one should be used? → A: CLI `--user` overrides config `remoteUser` for this invocation.
- Q: Should `--log-format json` force PTY allocation even if stdout isn’t a TTY? → A: Yes, force PTY when `--log-format json`.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run a command in the intended container (Priority: P1)

Developers can reliably execute a command inside the correct dev container, selecting the target via container ID, label, or workspace folder discovery, and receive the command’s exit code and output consistently.

**Why this priority**: This is the primary value of the exec subcommand; targeting the right container is foundational for all workflows.

**Independent Test**: Provide a workspace with a configured dev container and a running container. Execute `exec` targeting via `--container-id`, `--id-label`, and `--workspace-folder` in separate runs and verify command output and exit code match expectations.

**Acceptance Scenarios**:

1. **Given** a running container and its ID, **When** the user runs `exec --container-id <id> echo hello`, **Then** stdout prints `hello` and exit code is 0.
2. **Given** a running container labeled for the workspace, **When** the user runs `exec --id-label devcontainer.local_folder=<abs-path> env`, **Then** the command runs in that container and prints environment variables.
3. **Given** a workspace folder with a devcontainer config and a running container, **When** the user runs `exec --workspace-folder <path> pwd`, **Then** the command runs in the correct container and prints the expected working directory.

---

### User Story 2 - Environment matches developer expectations (Priority: P1)

Commands run with an environment consistent with the dev container configuration and user shell initialization: shell-derived variables (PATH, etc.), plus any ad‑hoc variables, and configuration-defined `remoteEnv`.

**Why this priority**: Correct environment (especially PATH) is essential for tools to run as they do in the dev environment.

**Independent Test**: Run `exec` with and without additional `--remote-env` entries; validate that shell initialization variables are present and that CLI-provided variables and configuration `remoteEnv` are applied in the documented order.

**Acceptance Scenarios**:

1. **Given** a container where shell init modifies PATH, **When** the user runs `exec <cmd>` without flags, **Then** PATH reflects shell initialization.
2. **Given** `--remote-env FOO=bar`, **When** the user runs `exec env`, **Then** `FOO=bar` appears even if not in config.
3. **Given** config defines `remoteEnv: { FOO: baz }` and user passes `--remote-env FOO=bar`, **When** running `exec env`, **Then** the final `FOO` value matches the documented merge order.

---

### User Story 3 - Interactive and non-interactive usage (Priority: P2)

Users can interactively run commands with a PTY when attached to a terminal and control terminal dimensions when needed; non-interactive runs behave predictably with separate stdout/stderr.

**Why this priority**: Interactive workflows (REPLs, shells, package managers) require PTY behavior to function correctly.

**Independent Test**: Run an interactive command (e.g., a shell) and a non-interactive command in CI; verify PTY allocation rules and optional terminal size settings.

**Acceptance Scenarios**:

1. **Given** a TTY-attached session, **When** the user runs `exec bash -lc 'tty'`, **Then** the command reports a TTY is present.
2. **Given** terminal size overrides, **When** the user provides `--terminal-columns` and `--terminal-rows`, **Then** the PTY is created with those dimensions.
3. **Given** a non-interactive environment (stdout not a TTY), **When** the user runs `exec`, **Then** the command executes without requiring a TTY and preserves separate stdout/stderr streams.

---

### User Story 4 - Clear errors and exit codes (Priority: P2)

Users receive clear, actionable errors for invalid inputs or missing configuration and consistent exit codes (including POSIX signal mapping) suitable for scripting and CI.

**Why this priority**: Deterministic error handling and exit codes are critical for automation and diagnostics.

**Independent Test**: Provide invalid flags/inputs and simulate signal termination; verify exact error messages and exit code mapping.

**Acceptance Scenarios**:

1. **Given** no `--container-id`, no `--id-label`, and no `--workspace-folder`, **When** running `exec`, **Then** it errors: “Missing required argument: One of --container-id, --id-label or --workspace-folder is required.”
2. **Given** a container process terminated by signal `SIGTERM (15)`, **When** running `exec`, **Then** exit code reported is `128 + 15 = 143`.
3. **Given** a devcontainer config path that does not exist, **When** discovery is requested, **Then** it errors with “Dev container config (<path>) not found.”

### Edge Cases

- Empty environment values provided (e.g., `--remote-env FOO=`) result in `FOO` being present with an empty value.
- Container exists but is stopped: command fails and surfaces Docker error with non-zero exit.
- Non-TTY stdout (e.g., redirected to file): exec runs without PTY.
- Workspace discovery requested but no config present: explicit error is shown.
- Large outputs in PTY mode stream continuously without truncation.

## Requirements *(mandatory)*

### Functional Requirements

 - **FR-001**: Users MUST be able to target the container by one of: `--container-id`, `--id-label` (repeatable), or `--workspace-folder` discovery; at least one of these must be provided for valid execution. When multiple are provided, precedence MUST be applied as: `--container-id` > `--id-label` > `--workspace-folder`.
- **FR-002**: `--id-label` MUST be validated as `name=value` with non-empty name and value; multiple labels are allowed.
- **FR-003**: `--remote-env` MUST accept `name=value` with value allowed to be empty and be repeatable; values are injected into the process environment for the executed command.
- **FR-004**: The environment used to run the command MUST follow the documented merge order: shell-derived via `userEnvProbe`, then configuration `remoteEnv`, then CLI `--remote-env` entries (later merges override earlier ones; CLI wins over config).
- **FR-005**: If the configuration does not define `userEnvProbe`, the default probe mode MUST be controlled by `--default-user-env-probe` with default `loginInteractiveShell`.
- **FR-006**: When a workspace or config path is provided, the tool MUST discover, read, and validate devcontainer configuration, returning “Dev container config (<path>) not found.” when missing or unreadable.
- **FR-007**: The effective configuration used for execution MUST reflect a merge of configuration with devcontainer image metadata labels and include variable substitution evaluated with container environment values where applicable.
- **FR-008**: Users MUST be able to influence Docker tooling paths and data folders via flags without changing core behavior (e.g., `--docker-path`, `--docker-compose-path`, `--container-data-folder`, `--container-system-data-folder`).
- **FR-009**: PTY allocation MUST be enabled when both stdin and stdout are TTYs; terminal dimensions MUST be settable via `--terminal-columns` and `--terminal-rows` when provided. When `--log-format json` is set, PTY MUST be forced regardless of TTY detection.
- **FR-010**: Exit codes MUST propagate from the container process; if terminated by signal, the exit code MUST be reported as `128 + <signal number>`; otherwise default to `1` when unknown.
- **FR-011**: Error messages for invalid inputs MUST be explicit and human-readable, including for invalid `--id-label`/`--remote-env` formats and missing required selection flags.
- **FR-012**: Logging level MUST be user-configurable (e.g., info/debug/trace) without altering the command’s stdout/stderr contract.
- **FR-013**: When both a config `remoteUser` and CLI `--user` are present, the CLI `--user` MUST take precedence for that invocation.

### Key Entities *(include if feature involves data)*

- **Target Container**: The container instance selected for execution using one of the supported selection inputs.
- **Effective Configuration**: The result of merging workspace config with image metadata labels and applying container-aware variable substitution.
- **Execution Environment**: The final set of environment variables provided to the executed process after probe and merges.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Container selection success rate ≥ 99% across targeting methods in automated tests (ID, label, workspace discovery).
- **SC-002**: Environment parity: ≥ 95% of sampled runs show PATH and expected variables present from shell init in test containers.
- **SC-003**: Acceptance tests confirm `--remote-env` empty values are honored in 100% of cases.
- **SC-004**: Error message conformance: 100% of negative tests match specified error texts exactly.
- **SC-005**: Exit code mapping correctness: 100% of signal-based terminations return `128 + signal` in tests.
- **SC-006**: Interactive commands run successfully with PTY in TTY contexts; ≥ 95% of runs respect provided terminal dimensions in tests.
