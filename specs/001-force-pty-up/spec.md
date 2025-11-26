# Feature Specification: Force PTY toggle for up lifecycle

**Feature Branch**: `001-force-pty-up`  
**Created**: 2025-11-26  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec force-PTY behavior for up subcommand: honor force_tty_if_json (and env hint) in up lifecycle/exec paths. Toggle PTY allocation in lifecycle exec based on flag/env while keeping JSON logs/stderr separation intact. Acceptance: integration showing PTY when flag+json logs; non-PTY when unset; no regression to exec behavior."

## Clarifications

### Session 2025-11-26

- Q: What environment variable and truthy/falsey rules should control forcing a PTY in JSON log mode when the flag is not provided? → A: Use env var `DEACON_FORCE_TTY_IF_JSON`; treat `true/1/yes` (case-insensitive) as enable, and `false/0/no` or unset as disable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Force PTY when JSON logs are requested (Priority: P1)

Operators running the `up` workflow with JSON-formatted logs need lifecycle exec steps to allocate a PTY whenever the `force_tty_if_json` toggle (or its environment equivalent) is set, so interactive commands behave correctly while logs stay structured.

**Why this priority**: This closes the primary compliance gap; without it, JSON logging breaks interactive lifecycle commands.

**Independent Test**: Run `up` with JSON logs and set the PTY toggle, then confirm lifecycle exec runs under a PTY while log output remains machine-readable.

**Acceptance Scenarios**:

1. Given JSON log mode is enabled and the `force_tty_if_json` flag is set, When `up` starts lifecycle exec commands, Then those commands run under a PTY and JSON log output remains structured with stderr separated.
2. Given JSON log mode is enabled and the environment hint requests PTY while the flag is unset, When lifecycle exec commands run, Then a PTY is allocated based on the env setting and JSON log formatting is preserved.

---

### User Story 2 - Preserve non-PTY default when not requested (Priority: P2)

Operators who do not request a PTY expect lifecycle exec commands in `up` to remain non-PTY even with JSON logs.

**Why this priority**: Prevents regressions for users relying on the current default and scripted environments.

**Independent Test**: Run `up` with JSON logs and leave both the flag and env hint unset; verify lifecycle exec uses non-PTY and log channels remain separated.

**Acceptance Scenarios**:

1. Given JSON log mode is enabled and no PTY toggle is provided via flag or env, When lifecycle exec runs, Then commands execute without a PTY and log/severity separation matches current behavior.
2. Given log mode is not JSON and any PTY toggle is set, When lifecycle exec runs, Then the existing non-JSON TTY behavior is preserved.

---

### User Story 3 - No regression to other exec paths (Priority: P3)

Users running other exec entry points expect their TTY behavior and log separation to remain unchanged.

**Why this priority**: Ensures the PTY toggle for lifecycle exec does not impact other exec workflows or exit codes.

**Independent Test**: Run a standard exec path with and without PTY toggles present and confirm behavior matches pre-change expectations.

**Acceptance Scenarios**:

1. Given a direct exec command outside the `up` lifecycle, When PTY toggles are present, Then exec behavior (TTY allocation, exit codes, log separation) matches existing defaults.

### Edge Cases

- Flag and environment hint conflict: explicit CLI flag takes precedence over the environment value for PTY selection.
- PTY is requested while JSON logs are disabled: lifecycle exec follows the existing non-JSON TTY behavior without forcing PTY.
- PTY allocation fails in the environment (e.g., unsupported host/daemon): the run surfaces a clear error and does not silently drop JSON log structure or PTY expectations.
- Multiple lifecycle exec steps within a single `up` run all honor the resolved PTY preference consistently.
- Lifecycle exec failures still propagate exit codes regardless of PTY selection.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When `up` runs in JSON log mode with `force_tty_if_json` set, lifecycle exec commands MUST allocate a PTY for their execution.
- **FR-002**: When JSON log mode is active and the environment variable `DEACON_FORCE_TTY_IF_JSON` is truthy (`true/1/yes`, case-insensitive), lifecycle exec commands MUST allocate a PTY even if the CLI flag is absent, unless overridden by an explicit flag value; falsey (`false/0/no`) or unset disables this behavior.
- **FR-003**: When neither flag nor environment hint requests PTY, lifecycle exec commands MUST run without a PTY, maintaining the current default even in JSON log mode.
- **FR-004**: JSON log output and stderr separation MUST remain intact and machine-readable regardless of PTY allocation choice during `up` lifecycle exec.
- **FR-005**: PTY selection logic MUST apply uniformly to all lifecycle exec steps within a single `up` invocation and follow precedence: CLI flag overrides environment hint, otherwise default behavior applies.
- **FR-006**: Exec entry points outside the `up` lifecycle MUST retain their existing TTY behavior and log separation; introducing the PTY toggle MUST NOT change their defaults or exit-code handling.
- **FR-007**: If PTY allocation is requested but cannot be honored, the system MUST present a clear, actionable message and avoid silently changing log formatting or TTY expectations.

### Key Entities *(include if feature involves data)*

- **TTY Preference**: Resolved PTY request derived from `force_tty_if_json` flag, the `DEACON_FORCE_TTY_IF_JSON` environment variable (truthy `true/1/yes`; falsey `false/0/no`; unset disabled), and default behavior; used to control lifecycle exec allocation during `up`.
- **Lifecycle Exec Command**: Each command executed during the `up` lifecycle that may be run under PTY/non-PTY based on the resolved preference while emitting JSON logs and stderr.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In tests, enabling the PTY toggle with JSON logs results in lifecycle exec commands running under a PTY in 100% of attempts.
- **SC-002**: In tests, omitting the PTY toggle results in lifecycle exec commands running without a PTY in 100% of attempts, matching current defaults.
- **SC-003**: Across PTY and non-PTY runs, JSON log streams remain machine-readable with separate stderr output and no parsing errors observed in validation runs.
- **SC-004**: Regression checks confirm exec entry points outside `up` show no change in TTY behavior, log separation, or exit-code handling before and after the feature.
