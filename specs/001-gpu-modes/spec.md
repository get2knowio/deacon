# Feature Specification: GPU Mode Handling for Up

**Feature Branch**: `001-gpu-modes`  
**Created**: 2025-11-26  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec GPU handling for the up subcommand: implement GPU modes (all|detect|none) across docker run/build and compose. Add GPU mode enum, propagate --gpus all for ‘all’, detect host GPU for ‘detect’ with warnings when absent, no-op for ‘none’. Apply to compose/build paths as applicable. Acceptance: GPU flags propagate; warnings on detect without GPUs; no-op for none."

## Clarifications

### Session 2025-11-26

- Q: What is the default GPU mode when the user does not specify one? → A: Default GPU mode is "none".

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Guarantee GPU access (Priority: P1)

Developers running GPU-dependent dev containers want a simple way to ensure every container and build started by the up command requests available GPUs.

**Why this priority**: GPU-first workflows block without explicit GPU access; providing a deterministic path prevents failed launches for core users.

**Independent Test**: Run up with GPU mode set to "all" on a GPU-capable host and confirm launched containers/build steps request GPU resources without extra flags.

**Acceptance Scenarios**:

1. **Given** a host with GPUs available, **When** a user runs up with GPU mode "all", **Then** all containers started by up request GPU resources.
2. **Given** a multi-service project that uses compose, **When** GPU mode "all" is chosen, **Then** each service started through up inherits the GPU request consistently.

---

### User Story 2 - Auto-detect with safe fallback (Priority: P2)

Developers unsure about host GPU availability want up to detect GPUs automatically, warn if none are present, and continue without blocking.

**Why this priority**: Reduces setup friction on varied hardware while keeping users informed about GPU availability.

**Independent Test**: Run up with GPU mode "detect" on a host without GPUs; confirm a warning is shown and containers start without GPU requests. Repeat on a GPU host and confirm GPUs are requested.

**Acceptance Scenarios**:

1. **Given** a host without GPUs, **When** GPU mode "detect" is used, **Then** a warning is shown before startup and containers proceed without GPU requests.
2. **Given** a host with GPUs, **When** GPU mode "detect" is used, **Then** GPU resources are requested for run/build/compose operations without additional user input.

---

### User Story 3 - Explicit CPU-only runs (Priority: P3)

Developers who do not need GPU resources want an explicit "none" option to ensure GPU settings are ignored and no warnings appear.

**Why this priority**: Avoids unexpected GPU interactions and keeps CPU-only workflows clean.

**Independent Test**: Run up with GPU mode "none" on any host and confirm no GPU requests or GPU-related warnings occur.

**Acceptance Scenarios**:

1. **Given** any host, **When** GPU mode "none" is selected, **Then** no GPU requests are sent for containers or builds.
2. **Given** repeated runs with GPU mode "none", **When** users inspect command output, **Then** no GPU-related notices appear across runs.

---

### Edge Cases

- Hosts with GPU hardware missing drivers or permissions still surface a clear warning in "detect" mode before proceeding without GPU requests.
- Selecting GPU mode "all" on a host that cannot honor GPU requests surfaces the runtime failure transparently without masking the error.
- Mixed workloads (some services needing GPUs, some not) still apply the chosen GPU mode consistently across all services started by up so behavior is predictable.
- Cached or default settings are overridden by an explicit GPU mode selection on each invocation to prevent stale GPU behavior.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The up command MUST accept a GPU mode choice covering "all", "detect", and "none" for all run, build, and compose pathways it orchestrates.
- **FR-002**: The chosen GPU mode MUST apply consistently across every container start and build action triggered during a single up invocation, without requiring duplicate user input.
- **FR-003**: In GPU mode "all" on GPU-capable hosts, the system MUST request GPU resources for all containers and builds initiated by up.
- **FR-004**: In GPU mode "detect", the system MUST check host GPU availability before starting containers or builds; if GPUs exist, behavior MUST match "all"; if not, the command MUST continue without GPU requests and present a clear warning once per invocation on stderr that GPUs were not found and execution will proceed without GPU acceleration.
- **FR-006**: In GPU mode "none", the system MUST refrain from sending any GPU requests and MUST avoid emitting GPU-related warnings.
- **FR-007**: User-facing output MUST communicate the applied GPU handling for the selected mode so users can confirm whether GPUs were requested or skipped; warnings/logs appear on stderr in text mode, while JSON mode preserves stdout for JSON and routes warnings to stderr.
- **FR-008**: Compose and build flows triggered by up MUST honor the selected GPU mode uniformly for all services and build targets to avoid partial application.
- **FR-009**: When no GPU mode is provided, the system MUST default to GPU mode "none".

### Key Entities

- **GPU Mode**: The selected option ("all", "detect", "none") that directs how GPU resources are requested or skipped for the up command.
- **Host GPU Capability**: The detected state of GPU availability and readiness on the host used to decide whether to request GPU resources.
- **Up Invocation Context**: The set of containers, services, and build targets started by a single up command that share the chosen GPU mode behavior.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of up runs using GPU mode "all" on GPU-capable hosts request GPU resources for every container and build initiated in that run.
- **SC-002**: 100% of up runs using GPU mode "detect" on hosts without GPUs display a pre-start warning and complete without issuing GPU requests.
- **SC-003**: 100% of up runs using GPU mode "none" issue zero GPU requests and display zero GPU-related warnings across run, build, and compose actions.
- **SC-004**: Users configuring GPU mode once per up invocation experience consistent GPU handling across run, build, and compose actions in at least 95% of observed test runs, reducing follow-up support inquiries about GPU behavior.

## Non-Functional & Observability

- GPU detection completes within 1s and does not measurably delay container startup.
- Warnings and diagnostics are emitted to stderr; stdout remains reserved for the primary result (JSON or text) per mode.
- Testing and validation follow targeted nextest profiles appropriate to touched areas, with fmt and clippy clean.
