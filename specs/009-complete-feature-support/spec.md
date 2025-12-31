# Feature Specification: Complete Feature Support During Up Command

**Feature Branch**: `009-complete-feature-support`
**Created**: 2025-12-28
**Status**: Draft
**Input**: User description: "Implement complete feature support during the `up` command including lifecycle commands, security options, mounts, entrypoints, and non-OCI feature references"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Feature Security Options Applied to Container (Priority: P1)

As a developer using a devcontainer with features that require elevated permissions (e.g., Docker-in-Docker), I need the container to be created with the correct security options (privileged, init, capAdd, securityOpt) so that the feature works correctly.

**Why this priority**: Security options are fundamental to feature functionality. Many popular features like Docker-in-Docker require privileged mode or specific capabilities. Without this, features are installed but non-functional.

**Independent Test**: Can be tested by creating a devcontainer with a feature requiring `privileged: true` and verifying the container has the correct Docker flags applied.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json with a feature declaring `"privileged": true`, **When** `deacon up` runs, **Then** the container is created with `--privileged` flag
2. **Given** a devcontainer.json with a feature declaring `"capAdd": ["SYS_PTRACE"]`, **When** `deacon up` runs, **Then** the container is created with `--cap-add=SYS_PTRACE`
3. **Given** multiple features where one declares `"init": true`, **When** `deacon up` runs, **Then** the container is created with `--init` flag
4. **Given** multiple features declaring different `capAdd` values, **When** `deacon up` runs, **Then** all capabilities are merged (union, deduplicated) and applied

---

### User Story 2 - Feature Lifecycle Commands Execute Before User Commands (Priority: P1)

As a developer, when my devcontainer features define lifecycle commands (like setting up environment variables or installing additional tools), I need those commands to run before my devcontainer.json lifecycle commands, so that my commands can depend on the feature's setup.

**Why this priority**: Lifecycle command ordering is critical for correct feature behavior. Many features rely on their lifecycle commands to complete setup that BuildKit layers cannot perform.

**Independent Test**: Can be tested by creating a devcontainer with a feature that creates a file in `onCreateCommand` and a user command that reads that file.

**Acceptance Scenarios**:

1. **Given** a feature with `onCreateCommand: "echo feature > /tmp/order"` and config with `onCreateCommand: "cat /tmp/order"`, **When** `deacon up` runs, **Then** feature command runs first, config command reads the file successfully
2. **Given** multiple features with `postCreateCommand`, **When** `deacon up` runs, **Then** commands execute in feature installation order, then config command last
3. **Given** a feature with empty/null lifecycle commands, **When** `deacon up` runs, **Then** those empty commands are skipped without error
4. **Given** features with `updateContentCommand`, **When** `deacon up` runs, **Then** feature commands run before config's `updateContentCommand`

---

### User Story 3 - Feature Mounts Applied to Container (Priority: P2)

As a developer using features that require persistent storage or shared volumes, I need the feature-declared mounts to be applied to my container so the feature can access required storage locations.

**Why this priority**: Mounts are essential for features requiring persistent data or shared access. While less common than security options, some features depend on specific mount configurations.

**Independent Test**: Can be tested by creating a devcontainer with a feature declaring a volume mount and verifying the mount exists in the running container.

**Acceptance Scenarios**:

1. **Given** a feature declaring `mounts: ["type=volume,source=mydata,target=/data"]`, **When** `deacon up` runs, **Then** the volume is mounted at `/data` in the container
2. **Given** config and feature both declaring mounts to the same target path, **When** `deacon up` runs, **Then** config mount takes precedence
3. **Given** a feature with structured mount format `{"type": "bind", "source": "/host", "target": "/container"}`, **When** `deacon up` runs, **Then** the mount is normalized and applied correctly
4. **Given** a feature with an invalid mount specification, **When** `deacon up` runs, **Then** a clear error message identifies the problematic feature

---

### User Story 4 - Feature Entrypoints Wrap Container Entry (Priority: P2)

As a developer using features that require entrypoint wrappers (e.g., for environment setup or process supervision), I need the feature entrypoints to be properly chained so that my container starts with the correct initialization sequence.

**Why this priority**: Entrypoints enable features to set up the container environment before the main process starts. This is important for features requiring shell initialization or environment configuration.

**Independent Test**: Can be tested by creating a devcontainer with a feature declaring an entrypoint wrapper and verifying the wrapper script executes before the main command.

**Acceptance Scenarios**:

1. **Given** a feature declaring `"entrypoint": "/usr/local/bin/wrapper.sh"`, **When** `deacon up` runs, **Then** the container uses the wrapper as its entrypoint
2. **Given** multiple features with entrypoints, **When** `deacon up` runs, **Then** entrypoints are chained in feature installation order
3. **Given** a feature entrypoint and user-specified command, **When** `deacon up` runs, **Then** the command executes through the entrypoint chain

---

### User Story 5 - Local Feature References Work (Priority: P2)

As a developer with custom features in my repository, I need to reference them via relative paths (e.g., `./my-feature`) so I can use local features without publishing to a registry.

**Why this priority**: Local features enable rapid iteration and custom tooling without registry overhead. This is essential for teams with internal features or during feature development.

**Independent Test**: Can be tested by creating a `.devcontainer/my-feature/devcontainer-feature.json` and referencing it as `./my-feature` in devcontainer.json.

**Acceptance Scenarios**:

1. **Given** a feature reference `"./local-feature"` and directory `.devcontainer/local-feature/devcontainer-feature.json`, **When** `deacon up` runs, **Then** the local feature is installed correctly
2. **Given** a feature reference `"../shared-feature"` resolving outside the .devcontainer directory, **When** `deacon up` runs, **Then** the path is resolved correctly relative to devcontainer.json
3. **Given** a local feature reference to a non-existent path, **When** `deacon up` runs, **Then** a clear error message indicates the missing path

---

### User Story 6 - HTTPS Tarball Feature References Work (Priority: P3)

As a developer referencing features via direct HTTPS URLs to tarballs, I need the feature to be downloaded and installed so I can use features from any HTTP-accessible location.

**Why this priority**: HTTPS references provide flexibility for features hosted outside OCI registries. This is a less common use case but required for full spec compliance.

**Independent Test**: Can be tested by referencing an HTTPS URL to a feature tarball and verifying installation.

**Acceptance Scenarios**:

1. **Given** a feature reference `"https://example.com/feature.tgz"`, **When** `deacon up` runs, **Then** the tarball is downloaded and feature metadata parsed from `devcontainer-feature.json`
2. **Given** an HTTPS feature URL that returns 404, **When** `deacon up` runs, **Then** a clear error message indicates the download failure
3. **Given** an HTTPS feature URL returning invalid tarball, **When** `deacon up` runs, **Then** a clear error message indicates the parsing failure

---

### Edge Cases

- What happens when a feature declares both `privileged: true` and the config declares `privileged: false`? (OR logic: feature wins)
- How does the system handle a feature lifecycle command that fails? (Fail-fast: immediately stop `up` with exit code 1, skip all remaining lifecycle commands)
- What happens when a local feature path contains spaces or special characters? (Paths should be properly escaped)
- How does the system handle circular feature dependencies in local features? (Should be detected during resolution)
- What happens when an HTTPS feature URL requires authentication? (Should fail with clear auth error, not silent fallback)
- How does the system handle features with conflicting entrypoints? (Chain in installation order per spec)
- What happens when a feature mount source path doesn't exist? (Docker handles this; should propagate Docker's error)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST merge feature security options (`privileged`, `init`, `capAdd`, `securityOpt`) with config security options using OR logic for booleans and union for arrays
- **FR-002**: System MUST apply `--privileged` flag to container creation if ANY source (config or feature) declares `privileged: true`
- **FR-003**: System MUST apply `--init` flag to container creation if ANY source (config or feature) declares `init: true`
- **FR-004**: System MUST apply deduplicated, uppercase-normalized `--cap-add` flags for all capabilities declared across config and features
- **FR-005**: System MUST apply deduplicated `--security-opt` flags for all security options declared across config and features
- **FR-006**: System MUST collect lifecycle commands (`onCreateCommand`, `updateContentCommand`, `postCreateCommand`, `postStartCommand`, `postAttachCommand`) from all resolved features
- **FR-007**: System MUST execute feature lifecycle commands BEFORE config lifecycle commands, in feature installation order
- **FR-008**: System MUST filter out empty/null lifecycle commands before execution
- **FR-009**: System MUST merge feature mounts with config mounts, with config mounts taking precedence for same target path
- **FR-010**: System MUST normalize feature mounts to canonical Docker CLI mount string format
- **FR-011**: System MUST report mount parsing errors with clear attribution to the source feature
- **FR-012**: System MUST extract and chain feature entrypoints in feature installation order
- **FR-013**: System MUST ensure user commands execute through the entrypoint chain
- **FR-014**: System MUST detect feature references starting with `./` or `../` as local path references
- **FR-015**: System MUST resolve local feature paths relative to devcontainer.json location
- **FR-016**: System MUST parse `devcontainer-feature.json` from local feature directories
- **FR-017**: System MUST report clear errors for missing local feature paths
- **FR-018**: System MUST detect feature references starting with `https://` as direct tarball URLs
- **FR-019**: System MUST download HTTPS feature tarballs to a temporary location
- **FR-020**: System MUST parse `devcontainer-feature.json` from within downloaded tarballs
- **FR-021**: System MUST report clear errors for HTTPS download or parsing failures
- **FR-022**: System MUST exit with code 1 immediately when any lifecycle command fails, skipping all remaining lifecycle commands
- **FR-023**: System MUST use a 30-second timeout for HTTPS feature downloads with a single retry on transient network errors

### Key Entities

- **ResolvedFeature**: A feature that has been fetched (from OCI, HTTPS, or local path) with its metadata parsed and dependencies resolved
- **FeatureMetadata**: The parsed `devcontainer-feature.json` containing lifecycle commands, security options, mounts, and entrypoint configuration
- **MergedSecurityOptions**: The combined security options from config and all features, with boolean OR and array union applied
- **LifecycleCommandList**: An ordered list of lifecycle commands from features (in installation order) followed by config commands

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: All 7 functional areas (lifecycle commands, privileged, init, capAdd, securityOpt, mounts, entrypoints) have working implementations verified by tests
- **SC-002**: Feature security options correctly enable features requiring elevated permissions (e.g., Docker-in-Docker works in privileged mode)
- **SC-003**: Feature lifecycle commands execute in correct order, with feature commands completing before user commands
- **SC-004**: Local feature references work for paths within and outside the `.devcontainer` directory
- **SC-005**: HTTPS feature references download and install without manual intervention
- **SC-006**: Existing feature functionality (OCI registry, environment variables, BuildKit layers) continues to work unchanged
- **SC-007**: Error messages clearly identify the source (feature name/path) when validation or execution fails

## Clarifications

### Session 2025-12-28

- Q: How should lifecycle command failures be handled? → A: Fail-fast - any lifecycle command failure stops `up` immediately with exit code 1
- Q: What timeout/retry behavior for HTTPS feature downloads? → A: 30-second timeout, single retry on transient errors

## Assumptions

1. Features are resolved in dependency order by the existing feature resolver before this functionality applies
2. The existing BuildKit-based feature layer installation remains the mechanism for installing feature content
3. Feature entrypoint chaining follows a simple sequential model (each wraps the next)
4. HTTPS feature tarballs follow the same internal structure as OCI feature tarballs
5. Local features have the same metadata schema as OCI features
6. The `FeatureMetadata` struct already contains all necessary fields for this implementation
