# Feature Specification: Build Subcommand Parity Closure

**Feature Branch**: `006-build-subcommand`  
**Created**: 2025-11-14  
**Status**: Draft  
**Input**: User description: "Implement tasks to close the GAP in the build subcommand SPEC."

## Clarifications

### Session 2025-11-14

- Q: When handling compose-based workspaces, which services should `deacon build` target? → A: Build only the service named in the devcontainer configuration; error if missing.

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

### User Story 1 - Tagged Build Deliverable (Priority: P1)

Application developers need `deacon build` to produce a dev container image that carries all requested tags, devcontainer metadata, and user-supplied labels so they can reuse the same image across teammates and CI without manual docker commands.

**Why this priority**: Without predictable tagging and metadata, the build output is not reusable, blocking most downstream workflows and making the subcommand fail its core promise.

**Independent Test**: Execute `deacon build` against a representative Dockerfile-based workspace with multiple `--image-name` and `--label` inputs; confirm images exist locally with the specified tags, metadata labels include the merged devcontainer configuration, and stdout returns the spec-compliant success payload.

**Acceptance Scenarios**:

1. **Given** a Dockerfile-based workspace and multiple `--image-name` values, **When** a user runs `deacon build`, **Then** each provided tag exists locally, the deterministic fallback tag remains available, and stdout contains `{ "outcome": "success", "imageName": [ ... ] }` with those values.
2. **Given** a workspace using devcontainer Features and custom labels via `--label`, **When** a user runs `deacon build`, **Then** the resulting image carries the devcontainer metadata label plus each user-specified label, and feature customizations are reflected in that metadata.

---

### User Story 2 - Registry and Artifact Distribution (Priority: P2)

CI and automation engineers must distribute build outputs by pushing to registries or exporting archives while receiving clear blocking errors whenever prerequisites (such as BuildKit availability) are not met.

**Why this priority**: Automated environments depend on predictable artifact delivery; failing to support push/export workflows or to guard invalid combinations breaks multi-stage pipelines.

**Independent Test**: Run acceptance scenarios that exercise `--push` and `--output` paths on BuildKit-enabled and BuildKit-disabled hosts, verifying registry availability, exported artifacts, and validation messaging without needing additional tooling.

**Acceptance Scenarios**:

1. **Given** a BuildKit-enabled host and valid registry credentials, **When** a user runs `deacon build --push --image-name org/app:ci`, **Then** the image is retrievable from the registry under that tag and stdout reports success with the pushed tag listed.
2. **Given** a host without BuildKit support, **When** a user invokes `deacon build --push`, **Then** the command exits with code `1`, emits the documented BuildKit gating error, and no partial build occurs.
3. **Given** any host, **When** a user specifies both `--push` and `--output`, **Then** the command fails fast with the message "--push true cannot be used with --output." and no build is attempted.

---

### User Story 3 - Multi-source Configuration Coverage (Priority: P3)

Tooling integrators need the build subcommand to handle Compose-based projects and image-reference configurations so that Features, labels, and tagging parity extend beyond simple Dockerfile paths.

**Why this priority**: Parity with the reference CLI requires covering all configuration shapes; without this support, organizations running Compose or base image flows must fall back to manual scripts.

**Independent Test**: Execute builds for (a) Compose workspaces with supported flag combinations and (b) configurations referencing an upstream base image, confirming features, labels, and tagging behave as in Dockerfile mode.

**Acceptance Scenarios**:

1. **Given** a Compose-based workspace without unsupported flags, **When** a user runs `deacon build`, **Then** the targeted service image builds successfully, features are applied, and unsupported flags such as `--push` or `--output` are proactively rejected with the documented messages.
2. **Given** a configuration that uses the `image` property instead of a Dockerfile, **When** a user runs `deacon build`, **Then** the base image is extended with the requested Features, tagged per the CLI inputs, and the metadata label reflects the merged configuration.

---



### Edge Cases

- Builds run with `--push` on hosts where BuildKit or registry credentials are unavailable.
- Users provide `--output` destinations that are unwritable or conflict with existing files.
- `--image-name` includes duplicate tags or invalid formats that must be validated before build execution.
- Users supply devcontainer configs from unexpected paths or with invalid filenames (not `devcontainer.json` / `.devcontainer.json`).
- Compose configurations invoke unsupported flags (`--push`, `--output`, `--cache-to`, `--platform`) and must be rejected without starting a build.
- Feature definitions request build contexts or security options that require BuildKit-only handling.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The build subcommand MUST accept and preserve multiple `--image-name` values, applying each tag to the resulting image while retaining a deterministic fallback tag when none are supplied.
- **FR-002**: The build subcommand MUST expose `--push`, `--output`, and `--label` options in the CLI help and propagate their values through validation, execution, and result reporting, including documenting the mutually exclusive relationship between `--push` and `--output` with usage examples.
- **FR-003**: When `--push` is provided on BuildKit-capable hosts, the system MUST publish the built image to the target registry and report the final tags in the spec-compliant success payload.
- **FR-004**: When `--output` is specified, the system MUST produce the requested artifact (e.g., OCI archive or directory) and confirm the destination in the success payload.
- **FR-005**: The build subcommand MUST inject the devcontainer metadata label plus all user-specified labels into the built image and ensure feature customizations are captured in that metadata.
- **FR-006**: All validation rules defined in the build spec (config filename enforcement, mutually exclusive flags, BuildKit-only gating, Compose flag restrictions, label parsing) MUST be enforced with documented error messages and exit code `1`.
- **FR-007**: The system MUST return stdout payloads that match the specification (`{ "outcome": "success" | "error", ... }`) and ensure all diagnostic logging is confined to stderr.
- **FR-008**: The build workflow MUST install requested Features during Dockerfile, image-reference, and Compose builds, honoring skip flags for auto-mapping and persisted customizations.
- **FR-009**: Image-reference configurations (`"image"` property) MUST be supported by extending the base image with Features, labels, and tagging parity equal to Dockerfile mode.
- **FR-010**: Compose-based configurations MUST be supported for eligible scenarios by targeting only the service named in the resolved devcontainer configuration, generating any required overrides, rejecting unsupported flag combinations before Docker runs, and failing fast if the referenced service does not exist.
- **FR-011**: Feature-driven build contexts, security options, and metadata lockfiles MUST be handled or skipped with explicit user-facing errors according to the spec’s requirements for BuildKit-only capabilities, including detection of BuildKit-only feature metadata and halting execution with the documented error message when BuildKit is unavailable.

### Non-Functional Requirements

- **NFR-001**: Builds MUST maintain stdout/stderr separation, emitting only the JSON contract payload to stdout in machine modes and routing all diagnostics through `tracing` on stderr.
- **NFR-002**: Performance goals SHALL align with SC-003 by running parity benchmarks on the reference hardware outlined in Quickstart (16 GB RAM developer laptop or equivalent CI runner) using the sample workspaces defined in `examples/build/`.
- **NFR-003**: Logging MUST include structured spans (`build.plan`, `build.execute`, `build.push`) with identifiers for workspace root and selected image tags to support traceability.
- **NFR-004**: Security posture MUST reject untrusted shell execution paths and redact credential-like values in logs consistent with Constitution Principle V.

### Key Entities *(include if feature involves data)*

- **Build Request**: Aggregates workspace location, configuration source, CLI flags (image names, push/export directives, labels), and feature toggles used to execute a build.
- **Image Artifact**: Represents the resulting image or exported archive, including associated tags, label metadata, and availability (local daemon, registry, or filesystem path).
- **Feature Manifest**: Captures devcontainer features, installation order, customizations, and optional lockfile data applied during the build.
- **Validation Event**: Records the outcome of CLI and configuration checks, including error messages and exit codes specified by the build spec.

### Assumptions

- Users have Docker (including BuildKit) installed and configured according to the spec’s prerequisites; BuildKit detection logic can rely on standard Docker capabilities.
- Registry credentials and filesystem permissions are pre-configured by users or automation owners; the feature does not manage authentication flows.
- Performance measurements and success metrics are evaluated on the reference hardware used in parity testing (16 GB RAM developer laptop or equivalent CI runner).
- Devcontainer configurations adhere to the upstream specification; malformed configs outside that scope are treated as validation failures.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of acceptance test runs across Dockerfile, image-reference, and Compose scenarios produce the documented JSON success payload with accurate `imageName` entries when the build succeeds.
- **SC-002**: 100% of validation scenarios covering invalid filenames, BuildKit gating, unsupported Compose flags, and `--push`+`--output` combinations terminate within 5 seconds and emit the exact error messages defined in the spec.
- **SC-003**: In parity benchmarking, at least 90% of builds that request `--push` or `--output` complete within 12 minutes (per architecture) when measured on the reference hardware (16 GB RAM laptop or Linux CI runner) using the Dockerfile, image-reference, and Compose sample workspaces. Timing MUST be captured with the `time` utility or equivalent CI timing metadata and recorded in parity tracking docs.
- **SC-004**: Post-release developer survey (n ≥ 8) reports that at least 80% of respondents can complete tagging or pushing workflows using only the `deacon build` command, reducing reliance on manual docker commands.

