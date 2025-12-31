# Feature Specification: Test Parallelization with cargo-nextest

**Feature Branch**: `001-nextest-parallel-tests`  
**Created**: 2025-11-09  
**Status**: Draft  
**Input**: User description: "Implement cargo-nextest driven parallel testing with categorized concurrency limits for Deacon test suite."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Local developer speeds up feedback loop (Priority: P1)

A Deacon contributor running tests locally wants faster feedback without manually curating which suites are safe to parallelize.

**Why this priority**: Local iteration speed is the primary pain point; solving it delivers immediate productivity gains for all engineers.

**Independent Test**: Run `make test-nextest-fast` on a working copy that previously required serial execution and confirm the command finishes successfully in significantly less time than the serial baseline while leaving the workspace ready for further work.

**Acceptance Scenarios**:

1. **Given** a developer with cargo-nextest installed, **When** they execute the recommended fast test command, **Then** the suite completes without failures and reports the applied test groups.
2. **Given** the same developer, **When** they need to run the full suite, **Then** `make test-nextest` runs all tests in parallel-safe groupings without manual flag adjustments.

---

### User Story 2 - CI pipeline keeps deterministic outcomes (Priority: P2)

The CI system must execute the full test suite with predictable timing and no flakiness despite increased concurrency.

**Why this priority**: CI reliability affects the whole team; it is slightly less urgent than local feedback but still critical for gatekeeping releases.

**Independent Test**: Trigger the CI workflow on a feature branch and verify the nextest-based job completes using the conservative profile, producing stable results across multiple runs.

**Acceptance Scenarios**:

1. **Given** a CI job configured with the nextest profile, **When** the workflow runs on a clean branch, **Then** all required test groups execute, serial-only suites remain serialized, and the job passes.
2. **Given** a temporary Docker daemon outage in CI, **When** the workflow runs, **Then** failures surface clearly as isolation errors rather than hidden timeouts.

---

### User Story 3 - Maintainer categorizes new tests (Priority: P3)

A maintainer adding new integration tests needs clear guidance for assigning them to appropriate concurrency groups.

**Why this priority**: Documentation and guardrails prevent regressions as the suite evolves, ensuring the investment in parallelization persists.

**Independent Test**: Follow the written guidance to place a new test into the right group, update configuration accordingly, and confirm the classification command shows the assignment.

**Acceptance Scenarios**:

1. **Given** a new filesystem-heavy integration test, **When** the maintainer consults the documentation, **Then** they can identify the correct group and update the configuration without guessing.
2. **Given** an incorrectly classified test discovered later, **When** the maintainer follows the remediation steps, **Then** the test is moved to a safer group and the suite remains green.

---

### Edge Cases

- When cargo-nextest is unavailable locally or in CI, commands abort immediately with a clear error that links to the installation instructions.
- What happens if a newly added Docker-intensive test is not assigned to an exclusive group and causes resource contention?
- On machines with fewer CPU cores than configured maxima, cargo-nextest’s thread auto-detection caps concurrency automatically without extra configuration.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The project MUST provide a nextest configuration that defines test groups and profiles aligning with Docker exclusivity, shared access, filesystem intensity, smoke, and parity suites.
- **FR-002**: The developer workflow MUST expose make targets (or equivalent documented commands) that run the fast subset, the full suite, and the CI-conservative profile without additional flags.
- **FR-003**: The test execution strategy MUST maintain backward compatibility by preserving a fully serial option using existing commands for debugging or constrained environments.
- **FR-004**: CI workflows MUST install and invoke cargo-nextest using the conservative profile, surfacing failures with explicit test names and group context.
- **FR-005**: Documentation MUST explain how to install nextest, choose profiles, and classify new tests to avoid misgrouping.
- **FR-006**: The configuration MUST ensure smoke and parity tests continue to run serially so container lifecycle coverage remains deterministic.
- **FR-007**: The solution MUST measure and report baseline versus parallelized test durations during rollout to confirm the targeted speedup is achieved.

### Key Entities *(include if feature involves data)*

- **Test Group**: Conceptual bucket describing concurrency limits for a set of tests (e.g., docker-exclusive, docker-shared, filesystem-heavy); determines maximum parallel threads applied by nextest.
- **Execution Profile**: Named configuration (default, ci, dev-fast) that selects thread counts, filters, and overrides for different environments (local fast loop, CI, full suite).
- **Make Target / Workflow Command**: Canonical entry points (e.g., `make test-nextest-fast`) that wrap nextest invocations, ensuring consistent usage across developers and CI.

## Assumptions

- Developers have cargo-nextest available or can install it via documented steps before running the new workflows.
- Current serial execution of the full suite averages roughly 60 minutes across CI sharded jobs; speedup targets reference that baseline.
- Docker daemon access remains required for integration and smoke tests, so isolation needs stem from container lifecycle interactions.

## Clarifications

### Session 2025-11-09

- Q: How should the workflow behave when cargo-nextest is not installed on a local machine or CI runner? → A: Abort and print a clear install instruction error.
- Q: How should the workflow handle environments with fewer CPU cores than the configured thread maxima? → A: Rely on cargo-nextest auto-capping concurrency based on available cores.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Fast local loop (`make test-nextest-fast`) completes at least 40% faster than the existing serial `make test` on a standard developer laptop.
- **SC-002**: Full suite execution with nextest (`make test-nextest` or CI profile) reduces overall runtime by 50–70% compared with the serial baseline while maintaining zero flaky failures over three consecutive runs.
- **SC-003**: At least 95% of tests execute with concurrency limits appropriate to their resource usage, evidenced by nextest reports showing assignments with no resource contention incidents in the first month.
- **SC-004**: Documentation changes receive positive verification from two maintainers who can follow the steps to classify a new test without additional guidance.
