# Feature Specification: Read-Configuration Spec Parity

**Feature Branch**: `001-read-config-parity`  
**Created**: 2025-10-31  
**Status**: Draft  
**Input**: User description: "Close gaps in read-configuration subcommand to achieve spec compliance per docs/subcommand-specs/read-configuration/SPEC.md and GAP.md"

## User Scenarios & Testing *(mandatory)*

<!--
  IMPORTANT: User stories should be PRIORITIZED as user journeys ordered by importance.
  Each user story/journey must be INDEPENDENTLY TESTABLE - meaning if you implement just ONE of them,
  you should still have a viable MVP (Minimum Viable Product) that delivers value.
  
  Assign priorities (P1, P2, P3, etc.) to each story, where P1 is the most critical.
  Think of each story as a standalone slice of functionality that can be:
  - Developed independently
  - Tested independently
  - Deployed independently
  - Demonstrated to users independently
-->

### User Story 1 - Emit Spec-Compliant JSON (Priority: P1)

Resolve configuration and emit a spec-compliant JSON payload to stdout with strict stdout/stderr separation. Always include `configuration`; include `mergedConfiguration` only when requested; include `featuresConfiguration` when requested.

**Why this priority**: Establishes the contract other commands and automations depend on; enables deterministic CI and tooling integrations.

**Independent Test**: Run `read-configuration` with `--workspace-folder` only, then with `--include-merged-configuration`, and with `--include-features-configuration`; verify stdout JSON fields and that logs go to stderr.

**Acceptance Scenarios**:

1. **Given** a workspace with `devcontainer.json`, **When** invoking with `--workspace-folder <path>`, **Then** stdout contains a JSON object with `configuration` and no `mergedConfiguration` or `featuresConfiguration`.
2. **Given** the same workspace, **When** adding `--include-merged-configuration`, **Then** stdout contains `configuration` and `mergedConfiguration` objects.
3. **Given** the same workspace, **When** adding `--include-features-configuration`, **Then** stdout contains `configuration` and `featuresConfiguration` (and if both flags are set, all three as applicable).
4. **Given** any mode, **When** running with `--log-format json` or text, **Then** logs appear on stderr only; stdout remains the single JSON document.

---

### User Story 2 - Container-Aware Resolution (Priority: P2)

Support container selection via `--container-id` or `--id-label <name=value>` (repeatable). When provided, enable before-container substitution for `${devcontainerId}` and support container-based metadata for merging when requested.

**Why this priority**: Enables parity with spec for container-aware workflows; required for accurate merged outputs and debugging in running environments.

**Independent Test**: With a known running container and labels, run with `--container-id` or `--id-label` and verify `${devcontainerId}` resolution and that `mergedConfiguration` uses container-derived metadata when requested.

**Acceptance Scenarios**:

1. **Given** a running container with labels, **When** invoking with `--id-label name=value`, **Then** `${devcontainerId}` resolves deterministically irrespective of label order.
2. **Given** a running container, **When** invoking with `--container-id <id>` and `--include-merged-configuration`, **Then** merged output includes container-derived metadata.
3. **Given** only container flags (no workspace/config), **When** invoking, **Then** stdout JSON includes `configuration: {}` and other requested fields as applicable.

---

### User Story 3 - Feature Resolution Options (Priority: P3)

Support `--include-features-configuration`, `--additional-features <JSON>`, and `--skip-feature-auto-mapping`. When requested, compute and include `featuresConfiguration` in stdout JSON; merge additional features; honor skip auto-mapping semantics.

**Why this priority**: Required for downstream commands (build/up) and parity with containers.dev Features flow.

**Independent Test**: Invoke with a config referencing Features, add `--include-features-configuration` and `--additional-features '{"id": {}}'`; verify `featuresConfiguration` correctness.

**Acceptance Scenarios**:

1. **Given** a config with Features, **When** invoking with `--include-features-configuration`, **Then** `featuresConfiguration` is present in stdout JSON.
2. **Given** additional features JSON, **When** invoking with `--additional-features '{"a":true}'`, **Then** the additional entries are merged appropriately in `featuresConfiguration`.
3. **Given** legacy Feature IDs, **When** invoking with `--skip-feature-auto-mapping`, **Then** auto-mapping is disabled for those IDs.

---

[Add more user stories as needed, each with an assigned priority]

### Edge Cases

- Only container flags provided: return `{ configuration: {} }` plus requested sections.
- `--id-label` order differences do not affect `${devcontainerId}`; adding/removing labels changes the ID.
- Terminal dimension flags must be paired: setting one without the other is an error.
- `--additional-features` must be valid JSON object; invalid JSON yields a command error.
- Overlapping `--additional-features` entries with base features: deep-merge per feature with additional-features values taking precedence.
- Both `--container-id` and `--id-label` provided: prefer `--container-id` and ignore `--id-label`.
- `--include-merged-configuration` with a selected container where inspect fails or container not found: error and exit non-zero; no fallback.
- Missing required selector: error “Missing required argument: One of --container-id, --id-label or --workspace-folder is required.”

## Requirements *(mandatory)*

<!--
  ACTION REQUIRED: The content in this section represents placeholders.
  Fill them out with the right functional requirements.
-->

### Functional Requirements

- **FR-001 (Selectors Required)**: The command MUST require at least one of `--container-id`, `--id-label`, or `--workspace-folder`; otherwise, emit the specified error and exit non-zero.
  - Precedence: If both `--container-id` and `--id-label` are provided, prefer `--container-id` and ignore `--id-label`.
- **FR-002 (Id-Label Validation)**: Each `--id-label` MUST match `<name>=<value>` with non-empty key and value; invalid entries cause an error and exit non-zero.
- **FR-003 (Terminal Pairing)**: `--terminal-columns` and `--terminal-rows` MUST be provided together or omitted together; otherwise, error and exit non-zero.
- **FR-004 (Docker Paths Defaults)**: If unspecified, default `--docker-path` to `docker` and `--docker-compose-path` to `docker-compose` (detect `docker compose` v2 where applicable) without contacting the network.
- **FR-005 (Before-Container Substitution)**: When container selection flags are provided, `${devcontainerId}` MUST be computed deterministically from labels; label order MUST NOT affect the value.
- **FR-006 (Container Substitution Scope)**: When container is selected, container-based substitutions (e.g., `${containerEnv:VAR}`) MUST be supported for merged computations; no commands are executed inside the container.
- **FR-007 (Configuration Output)**: Stdout MUST always include `configuration` (resolved with pre-container substitutions). Logs MUST be written to stderr, respecting log level/format.
- **FR-008 (Features Output)**: When `--include-features-configuration` is set, stdout MUST include `featuresConfiguration`. `--additional-features` JSON MUST be merged; `--skip-feature-auto-mapping` MUST disable auto-mapping of legacy IDs.
  - Conflict rule: When keys overlap, perform a deep-merge per feature with additional-features values taking precedence.
- **FR-009 (Merged Output)**: When `--include-merged-configuration` is set, stdout MUST include `mergedConfiguration` computed per spec: container metadata if container is selected; otherwise derive from image build info and features.
  - Failure mode: If a container is selected and inspect fails or the container is not found, the command MUST error and exit non-zero. No fallback to non-container merge and no silent omission of `mergedConfiguration`.
- **FR-010 (JSON Output Contract)**: In all modes, stdout MUST be a single JSON document; no other output on stdout. Errors/logs MUST go to stderr only.
- **FR-011 (Empty Base Config Case)**: When only container flags are given, base `configuration` MUST be `{}` and other requested sections computed accordingly.
- **FR-012 (Error Messages)**: Error messages MUST match the spec wording for missing selector, invalid id-label, invalid JSON in `--additional-features`, and unpaired terminal dimensions.

### Key Entities *(include if feature involves data)*

- **ParsedInput**: Logical set of flags/values after validation (selectors, docker paths, terminal dimensions, feature toggles, paths).
- **ReadConfigurationOutput**: JSON payload with fields: `configuration` (always), `featuresConfiguration` (optional), `mergedConfiguration` (optional).

## Success Criteria *(mandatory)*

<!--
  ACTION REQUIRED: Define measurable success criteria.
  These must be technology-agnostic and measurable.
-->

### Measurable Outcomes

- **SC-001**: All required flags and validation behaviors in GAP.md are implemented; CLI help reflects new flags.
- **SC-002**: Output structure includes `configuration` always, and includes `featuresConfiguration`/`mergedConfiguration` only when requested, matching spec.
- **SC-003**: End-to-end tests for selector requirement, id-label validation, features inclusion, merged semantics, and container-only mode pass locally and in CI.
- **SC-004**: Stdout/stderr contract holds under text and JSON logging; parsing stdout as JSON always succeeds.

## Clarifications

### Session 2025-10-31

- Q: When `--additional-features` provides entries that already exist in the base configuration’s features map, how should conflicts be resolved? → A: Deep-merge per feature with additional-features precedence.
- Q: When both `--container-id` and one or more `--id-label` flags are provided, which should take precedence? → A: Prefer `--container-id`; ignore `--id-label`.
- Q: When `--include-merged-configuration` is set and a selected container cannot be inspected/found, how should the command behave? → A: Error and exit non-zero; no fallback.
