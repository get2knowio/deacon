# Feature Specification: Devcontainer Up Gap Closure

**Feature Branch**: `[001-up-gap-spec]`  
**Created**: 2025-11-18  
**Status**: Draft  
**Input**: User description: "Define a specification to fulfill the tasks listed in docs/subcommand-specs/up/tasks/ which are an enumeration of the gap defined in docs/subcommand-specs/up/GAP.md of the devcontainer spec specified in docs/subcommand-specs/up/SPEC.md ."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Reliable up invocation with full flag coverage (Priority: P1)

A developer launches `deacon up` for a project and expects every documented flag to be available, validated, and reflected in a structured JSON result so they can immediately connect their tools without manual log parsing.

**Why this priority**: This unlocks day-to-day usability and automation by ensuring the command is predictable and integrator-friendly.

**Independent Test**: Run `deacon up` with workspace folder, id labels, mount/env/cache flags, and output options; verify JSON success shape appears on stdout and logs stay on stderr.

**Acceptance Scenarios**:

1. **Given** a valid devcontainer config and required flags, **When** the user runs `deacon up` with include-configuration options, **Then** stdout contains the success JSON with requested config blobs and exit code is 0.
2. **Given** a malformed mount string, **When** the user runs `deacon up`, **Then** execution halts before runtime actions and stdout contains the standardized error JSON with exit code 1.

---

### User Story 2 - CI prebuild and lifecycle orchestration (Priority: P2)

A build pipeline triggers `deacon up --prebuild` to produce a ready-to-use dev container image with Features, dotfiles, and lifecycle hooks executed in the correct order, stopping after updateContent when requested.

**Why this priority**: Prebuild workflows reduce cycle time for teams and must be deterministic for CI repeatability.

**Independent Test**: Invoke `deacon up --prebuild` on a fixture with Features and dotfiles; confirm lifecycle stops after updateContent on first run and reruns updateContent on subsequent runs without duplicating dotfiles installation.

**Acceptance Scenarios**:

1. **Given** a project with Features and dotfiles configured, **When** `--prebuild` is used, **Then** lifecycle runs through onCreate and updateContent, records completion, and exits before postCreate/postStart/postAttach.
2. **Given** a rerun of the same command, **When** previous prebuild markers exist, **Then** updateContent runs again, dotfiles are not reinstalled, and the JSON success output reflects the existing container.

---

### User Story 3 - Compose and reconnect workflows (Priority: P3)

An ops engineer uses compose-based dev environments with custom docker paths, profiles, and id labels to reconnect to existing containers while injecting remote env and secrets safely.

**Why this priority**: Compose users need parity with single-container flow to avoid environment drift and to enable remote/devbox-style setups.

**Independent Test**: Run `deacon up` on a compose fixture with profiles, additional mounts, remote env, and secrets files; ensure mounts convert to volumes, project name honors .env, and secret values are redacted in logs.

**Acceptance Scenarios**:

1. **Given** compose config and `.env` defining a profile, **When** `deacon up` runs with extra mounts, **Then** mounts are converted to compose volumes, the profile is applied, and the container starts with the expected labels.
2. **Given** provided id labels and `--expect-existing-container`, **When** no matching container exists, **Then** the command fails fast with a clear error JSON before any build or create call.

### Edge Cases

- Missing both workspace and id labels triggers a validation error without contacting the container runtime.
- Mount or remote-env strings that fail the documented patterns are rejected with actionable errors and no runtime side effects.
- Secrets files that are unreadable or contain malformed lines stop execution and report which file failed without exposing secret values.
- Compose profile names that are absent in the compose config result in a clear error before attempting container start.
- GPU requests on unsupported hosts produce warnings and skip GPU features without blocking other work.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The `up` command MUST expose all documented flags for workspace identification, runtime behavior, mounts/env/cache/build, features/dotfiles/metadata, output shaping, docker/compose paths, and state folders, with defaults and help text matching the specification.
- **FR-002**: The CLI MUST enforce validation rules before runtime actions, including required workspace or id labels, override-config pairing, terminal dimension pairing, mount regex, remote-env format, and early failure for expect-existing when containers are absent.
- **FR-003**: Input processing MUST normalize arrays, parse JSON for additional features, resolve workspace/config paths, and capture provided id labels into a ParsedInput equivalent for downstream logic.
- **FR-004**: Configuration resolution MUST reject invalid devcontainer filenames, support override-only discovery, discover id labels when not provided, block disallowed Features, and merge image metadata into the resolved configuration.
- **FR-005**: Runtime behavior MUST honor build-no-cache, workspace mount consistency, GPU availability policy, default user env probe mode, update-remote-user-uid default, BuildKit on/off, cache-from/to, and build platform/push options whenever builds occur.
- **FR-006**: Container creation MUST support feature-driven image extension, UID update flows, security options (init, privileged, capabilities, security options), and entrypoint handling, producing container properties aligned with the spec.
- **FR-007**: Lifecycle execution MUST include updateContentCommand, respect skip-post-attach, enforce prebuild sequencing (stop after updateContent on first run and rerun it on subsequent runs), and await background tasks on success.
- **FR-008**: Dotfiles support MUST clone/install using provided repository/command/target path, run during setup (except when lifecycle is skipped as specified), and remain idempotent via marker tracking.
- **FR-009**: Remote environment and secrets handling MUST accept remote-env flags, load one or more secrets files, merge values deterministically, inject them into runtime and lifecycle, and redact secrets from logs and outputs.
- **FR-010**: Compose flow MUST convert additional mounts into compose volumes, honor compose profiles including those from .env project names, and apply metadata omission flags and build options in parity with single-container flow.
- **FR-011**: Output MUST emit structured JSON on stdout for success and error outcomes, include configuration blobs when requested, keep human-readable logs on stderr, and standardize error messages and exit codes.
- **FR-012**: State management MUST honor user-data and container-session data folders for caching (including environment probes) and handle container/system data folder options without breaking existing defaults.
- **FR-013**: A comprehensive test suite MUST cover happy paths (image, features, compose), validation failures (mount, remote-env, missing config), lifecycle modes (skip-post-create, prebuild, remove-existing, expect-existing), include-config output, and sample smoke tests/examples demonstrating JSON output and new flags.

### Key Entities *(include if feature involves data)*

- **Command Invocation**: User-provided flags and inputs representing how `up` should run, including identification, runtime, lifecycle, environment, and output preferences.
- **Resolved Devcontainer Configuration**: The merged configuration derived from files, overrides, image metadata, Features, dotfiles, mounts, env, secrets, and derived identifiers used to drive container creation.
- **Execution Result**: The structured outcome containing container identifiers (single or compose), remote user/workspace details, optional configuration blobs, and standardized error payloads when applicable.

### Assumptions

- Existing secret redaction and dotfiles helpers can be extended without changing their external behavior, and their use will not introduce new logging surfaces for secret values.
- GPU availability detection and user env probes follow the specification defaults; when unsupported on a platform, the system warns without blocking unrelated functionality.
- Reference fixtures for compose and feature scenarios are available or can be added alongside the test work to exercise new flags and lifecycle combinations.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of the tasks enumerated in `docs/subcommand-specs/up/tasks/` map to implemented and test-verified functional requirements, with all associated tests passing locally.
- **SC-002**: For the three representative scenarios (single-container, prebuild, compose with profiles), `deacon up` returns the expected JSON shape on stdout with correct optional fields and exit code 0 in under 3 minutes end-to-end.
- **SC-003**: Validation failures for at least five representative invalid input cases (missing workspace/id-label, bad mount, bad remote-env, unreadable secrets file, absent compose profile) emit the standardized error JSON on stdout, exit with code 1, and surface actionable messages without leaking secrets.
- **SC-004**: New and updated smoke/integration tests demonstrate JSON-only stdout with all human logs on stderr and achieve a deterministic pass rate across two consecutive runs on the reference fixtures.
