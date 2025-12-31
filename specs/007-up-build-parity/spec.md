# Feature Specification: Up Build Parity and Metadata

**Feature Branch**: `007-up-build-parity`  
**Created**: 2025-11-27  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec feature/build parity for up subcommand: respect BuildKit/cache-from/to/buildx flags in Dockerfile and feature builds; honor skip-feature-auto-mapping/ lockfile/frozen; merge feature metadata into mergedConfiguration. Acceptance: build options propagate; skip-auto-mapping behavior enforced; lockfile/ frozen honored; metadata present. Use 007 as the numerical prefix."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Build options respected (Priority: P1)

Platform operators running the `up` command need their BuildKit cache and builder options applied to both Dockerfile builds and feature builds so builds stay fast and aligned with CI settings.

**Why this priority**: Missing or inconsistent build options slows delivery and creates drift between local and automated environments.

**Independent Test**: Run `up` with cache-from, cache-to, and buildx/builder options and verify they appear in both Dockerfile and feature build steps without requiring other feature controls.

**Acceptance Scenarios**:

1. **Given** a project with a Dockerfile and declared features, **When** `up` runs with cache-from/cache-to/buildx options set, **Then** both Dockerfile and feature builds execute with those options and no defaults override them.
2. **Given** build options are omitted, **When** `up` runs, **Then** builds proceed with current defaults and no unexpected cache or builder settings are introduced.

---

### User Story 2 - Deterministic feature selection (Priority: P2)

Developers want to disable auto-mapped features and enforce lockfile or frozen installs to guarantee the exact feature set and versions.

**Why this priority**: Prevents surprise feature additions or updates that can break builds or compliance.

**Independent Test**: Run `up` with skip-feature-auto-mapping and lockfile/frozen toggles and verify the resolved feature set matches declarations and locks.

**Acceptance Scenarios**:

1. **Given** skip-feature-auto-mapping is enabled, **When** `up` runs, **Then** only explicitly declared features are selected and built with no auto-added features.
2. **Given** a feature lockfile and frozen mode, **When** the resolved features differ from the lockfile or the lockfile is missing, **Then** `up` stops before builds start and surfaces a clear error about the mismatch.

---

### User Story 3 - Metadata available downstream (Priority: P3)

Consumers of mergedConfiguration need feature metadata included after `up` completes so tooling and audits can use it without manual reconstruction.

**Why this priority**: Metadata visibility supports compliance and reporting, though it follows build determinism in criticality.

**Independent Test**: Inspect mergedConfiguration after a build and confirm it contains metadata for each built feature.

**Acceptance Scenarios**:

1. **Given** features that emit metadata, **When** `up` completes, **Then** mergedConfiguration includes each feature's metadata as declared.
2. **Given** multiple features, **When** merged configurations are produced, **Then** metadata from all features is present once each with no omissions.

### Edge Cases

- Unsupported or conflicting build options (e.g., buildx requested when BuildKit is unavailable) must fail fast with an actionable error before builds start.
- Cache-from or cache-to sources that are unreachable should degrade gracefully with clear warnings while allowing the build to proceed without cached layers.
- Skip-feature-auto-mapping must override any defaults or template mappings so no implicit features are added even when mappings exist.
- Missing or corrupted lockfiles under frozen mode must halt execution with messaging that instructs how to repair or regenerate locks.
- Features without metadata must still appear in mergedConfiguration with an empty or minimal metadata placeholder so consumers see a complete list.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Up MUST apply user-specified BuildKit cache-from and cache-to options to both Dockerfile builds and feature builds consistently.
- **FR-002**: Up MUST honor build executor selection (such as buildx or builder choice) for Dockerfile builds and feature builds whenever provided alongside BuildKit settings.
- **FR-003**: Up MUST leave build behavior unchanged when no BuildKit or cache options are provided, avoiding implicit defaults that alter build outputs.
- **FR-004**: Up MUST support a skip-feature-auto-mapping control that prevents adding or modifying features beyond those explicitly requested.
- **FR-005**: Up MUST enforce lockfile and frozen modes so that any deviation from the locked feature set halts execution with a clear explanation before builds run.
- **FR-006**: Up MUST merge feature metadata into mergedConfiguration for every built feature, preserving identifiers and declared metadata fields.
- **FR-007**: Up MUST report status and exit codes that reflect success or failure in applying build options, enforcing skip auto mapping, lockfile/frozen requirements, and metadata merging.

### Key Entities

- **Build options**: User-supplied values for cache-from/cache-to and build executor selection applied to Dockerfile and feature builds.
- **Feature controls**: Flags or settings for skip-feature-auto-mapping, lockfile, and frozen behavior that govern feature resolution.
- **Merged configuration**: Aggregated representation after `up` completes, including resolved features, build decisions, and feature metadata.
- **Feature metadata**: Descriptive data returned by features (such as identifiers, versions, or annotations) that must surface in the merged configuration.

## Dependencies & Assumptions

- BuildKit and buildx support is available in environments where users supply those options; otherwise the command fails fast with actionable guidance.
- Feature definitions publish metadata in a retrievable shape; when absent, mergedConfiguration still lists the feature with an empty metadata placeholder.
- Lockfile and frozen modes rely on an existing lockfile produced by supported workflows; if missing or corrupted, execution stops before any build work.
- Cache-from and cache-to sources may be slow or unreachable; builds proceed without cached layers while warning the user about reduced caching.
- mergedConfiguration remains the authoritative post-build output consumed by downstream tooling for audits and automation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In test runs, 100% of builds executed with provided cache-from/cache-to/buildx options show those options applied in both Dockerfile and feature build steps.
- **SC-002**: With skip-feature-auto-mapping enabled, test environments show zero auto-added features across at least three representative sample projects.
- **SC-003**: Frozen or lockfile runs detect and block 100% of feature resolution drift cases before any build begins, with user-facing messaging recorded.
- **SC-004**: mergedConfiguration outputs expose metadata for every built feature in all test scenarios, verified across multi-feature and single-feature cases.
- **SC-005**: User acceptance tests confirm build-option propagation and deterministic feature handling complete without regression, achieving a pass rate of at least 95% across defined acceptance scenarios.
