# Feature Specification: Compose mount & env injection

**Feature Branch**: `005-compose-mount-env`  
**Created**: 2025-11-26  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec native compose mount/env injection for up subcommand: apply CLI mounts and remote env to compose services without temp override; align mountWorkspaceGitRoot in compose mounts. Extend ComposeProject/ComposeCommand to inject env/volumes for primary service, handle external volumes, respect profiles/env-files/project naming. Acceptance: mounts/env visible inside service; external volumes honored; git-root applied; profiles/env-files still respected."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Mounts and env applied to primary service (Priority: P1)

Developers starting the primary compose service via the up subcommand need CLI-provided mounts and remote environment entries to appear inside the service container without managing extra compose override files.

**Why this priority**: Without reliable mount/env injection, the up workflow cannot surface remote assets or workspace paths, blocking devcontainer parity.

**Independent Test**: Start a project with a defined primary service using the up command while passing CLI mounts and remote env; verify inside the running service that mounts and env vars are present without additional user steps.

**Acceptance Scenarios**:

1. **Given** a compose project with a primary service, **When** the user runs the up subcommand with CLI mounts and remote env values, **Then** the primary service starts with those mounts at the requested paths and environment variables visible before application startup.
2. **Given** the project uses the default compose definitions only, **When** the up command applies mounts/env, **Then** the user does not need to manage temporary compose override files and the compose project naming remains consistent across runs.

---

### User Story 2 - External volumes preserved and git root aligned (Priority: P2)

Developers using external volumes and the workspace Git root option need the up workflow to honor external volume references while also mounting the Git root consistently alongside other CLI mounts.

**Why this priority**: Losing external volumes or misaligned Git root mounts risks data loss and breaks reproducible dev environments.

**Independent Test**: Run up on a project declaring an external volume and with mountWorkspaceGitRoot enabled; confirm data persists through the external volume and the Git root path is mounted in the service where expected.

**Acceptance Scenarios**:

1. **Given** a compose project declaring an external volume, **When** the user runs the up subcommand with CLI mounts/env, **Then** the service uses the external volume unchanged and data stored prior to the run remains available inside the service.
2. **Given** mountWorkspaceGitRoot is enabled, **When** the up command runs, **Then** the workspace Git root is mounted inside the primary service using the same conventions as other CLI mounts.

---

### User Story 3 - Profiles, env-files, and project naming respected (Priority: P3)

Developers relying on compose profiles, env-files, and custom project names need the up workflow to keep those settings intact while injecting CLI mounts and remote env.

**Why this priority**: Profile and naming drift causes unintended services or resource names, reducing confidence in the CLI.

**Independent Test**: Execute up with a selected profile and env-file while providing CLI mounts/env; verify the started services match the selected profile, env-file values load, and project/resource names align with the configured project name.

**Acceptance Scenarios**:

1. **Given** compose configurations include profiles and env-files, **When** the user runs up with a chosen profile and remote env, **Then** only the profiled services start and their environment includes both env-file values and the CLI-provided entries.
2. **Given** a custom compose project name is configured, **When** the user runs up with CLI mount/env injection, **Then** resources (services, networks, volumes) use the custom name prefix while still reflecting the injected mounts/env.

### Edge Cases

- Running the up command without CLI mounts or remote env inputs leaves compose behavior unchanged beyond starting services.
- When CLI-supplied env keys conflict with env-file or service defaults, the CLI-provided values take precedence while other compose-defined values remain intact.
- If an external volume referenced by the project is absent, the user is notified by the compose workflow without substituting a bind mount; injection does not mask the missing volume.
- In multi-service projects where only the primary service should receive injection, other services start with their compose-defined mounts/env unaffected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The up subcommand must apply user-specified CLI mounts directly to the primary compose service so that requested host paths appear at the intended container paths on startup without requiring users to manage temporary compose override files.
- **FR-002**: The up subcommand must apply user-specified remote environment entries to the primary service so they are available in the container environment before the service process starts.
- **FR-003**: When mountWorkspaceGitRoot is enabled, the workspace Git root path must be mounted into the primary service using the same rules as other CLI mounts.
- **FR-004**: Compose projects declaring external volumes must retain those external references when the up subcommand injects mounts/env, ensuring data stored in the external volumes is preserved across runs.
- **FR-005**: The up workflow must respect compose profiles, env-files, and project naming so that the selected profile services start and resource naming conventions remain consistent while CLI mounts/env are injected.
- **FR-006**: Injection of CLI mounts and remote env must target the intended primary service without altering mounts or environment of non-target services unless explicitly requested by the user.

### Key Entities *(include if feature involves data)*

- **Compose project**: The collection of compose files, profiles, env-files, and project naming that define services and external resources for the up workflow.
- **Primary service**: The service selected by the CLI for mount and remote env injection during the up command.
- **CLI mount request**: User-supplied mount definitions, including mountWorkspaceGitRoot, that should appear inside the primary service container.
- **Remote environment set**: Key-value environment entries supplied through the CLI to augment the primary service at startup.
- **External volume**: A volume declared as external in compose configuration that must remain referenced without conversion during up.

## Assumptions

- CLI remote environment values override conflicting entries from env-files or service defaults, while non-conflicting compose-provided values remain in effect.
- The primary service is identifiable through existing compose configuration or CLI selection, and only that service receives injection by default.
- External volumes referenced by the compose project are pre-created or available to the runtime; missing volumes surface as errors rather than being replaced.
- Users run the up subcommand from within the intended workspace so compose profiles, env-files, and project naming resolve as configured.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In 100% of test runs using the up subcommand with CLI mounts/env, the primary service exposes the requested mounts and environment variables inside the running container without extra user steps.
- **SC-002**: For compose projects with external volumes, data written before the run remains accessible after up completes in 100% of validation runs that include CLI mount/env injection.
- **SC-003**: When mountWorkspaceGitRoot is enabled, the workspace Git root is mounted at the expected path inside the primary service in 100% of runs across supported host platforms.
- **SC-004**: Across representative projects using profiles, env-files, and custom project names, the services started and resource names match the configured profiles and naming while including CLI-provided mounts/env in 100% of validation attempts.
