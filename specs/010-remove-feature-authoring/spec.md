# Feature Specification: Remove Feature Authoring Commands

**Feature Branch**: `010-remove-feature-authoring`
**Created**: 2026-02-19
**Status**: Draft
**Input**: User description: "Remove all DevContainer Feature authoring commands from Deacon. The CLI is narrowing its scope to consumer-only commands — developers who USE dev containers, not developers who AUTHOR features for them."

## Clarifications

### Session 2026-02-19

- Q: Should template authoring commands (`templates publish`, `templates metadata`, `templates generate-docs`) and the unimplemented `features pull` stub be included in this removal scope? → A: Yes (Option C). Remove all features subcommands (including the unimplemented `features pull` stub, eliminating the entire `features` group), remove template authoring commands (`publish`, `metadata`, `generate-docs`), and retain only consumer template commands (`templates pull`, `templates apply`).
- Q: Should spec documentation under `docs/subcommand-specs/completed-specs/` for removed commands be deleted or archived? → A: Delete all spec docs for removed commands (`features-test/`, `features-info/`, `features-plan/`, `features-package/`, `features-publish/`). Update references in `CLI_PARITY.md` and `ARCHITECTURE.md`. Git history preserves the content.
- Q: Which core library modules should be deleted vs preserved? → A: Delete `features_info.rs` and `features_test/` (authoring-only). Preserve `features/` module (shared with consumer FeatureInstaller path).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - CLI No Longer Offers Authoring Commands (Priority: P1)

A developer using Deacon to manage their dev containers invokes the CLI help or attempts to run an authoring command. The CLI no longer lists or accepts the removed authoring commands: all six `features` subcommands (`test`, `info`, `plan`, `package`, `publish`, `pull`) and three `templates` authoring subcommands (`publish`, `metadata`, `generate-docs`). The entire `features` subcommand group is removed. The `templates` group retains only consumer commands (`pull`, `apply`). The developer sees a clean, focused command set oriented around consuming dev containers.

**Why this priority**: This is the core deliverable — removing the authoring commands is the primary scope change that simplifies the CLI and communicates Deacon's consumer-only focus.

**Independent Test**: Can be fully tested by running `deacon --help` and verifying the `features` group no longer appears, running `deacon templates --help` and verifying only `pull` and `apply` appear, and by attempting each removed command and confirming it is unrecognized.

**Acceptance Scenarios**:

1. **Given** a built Deacon binary with authoring commands removed, **When** a developer runs `deacon --help`, **Then** the `features` subcommand group does not appear.
2. **Given** a built Deacon binary, **When** a developer runs `deacon features test`, **Then** the CLI returns an unrecognized command error.
3. **Given** a built Deacon binary, **When** a developer runs `deacon features publish`, **Then** the CLI returns an unrecognized command error.
4. **Given** a built Deacon binary, **When** a developer runs `deacon templates --help`, **Then** only `pull` and `apply` subcommands appear.
5. **Given** a built Deacon binary, **When** a developer runs `deacon templates publish`, **Then** the CLI returns an unrecognized command error.

---

### User Story 2 - Feature Installation During Container Startup Remains Functional (Priority: P1)

A developer runs `deacon up` with a devcontainer configuration that references OCI-hosted features. The feature installer fetches and installs those features exactly as before. The removal of authoring commands has no impact on the consumer-side feature installation workflow.

**Why this priority**: Equally critical to the removal — preserving consumer functionality is a hard constraint. Breaking feature installation would be a regression affecting all users.

**Independent Test**: Can be fully tested by running `deacon up` against a devcontainer.json that specifies features from an OCI registry and verifying features are installed successfully.

**Acceptance Scenarios**:

1. **Given** a devcontainer.json that references one or more OCI-hosted features, **When** a developer runs `deacon up`, **Then** features are fetched from the OCI registry and installed into the container successfully.
2. **Given** a devcontainer.json with no features specified, **When** a developer runs `deacon up`, **Then** the container starts normally without errors related to feature handling.

---

### User Story 3 - Dead Code Fully Removed (Priority: P2)

After the authoring commands are removed, no orphaned modules, unused imports, or dead test code remains in the codebase. The project compiles cleanly without warnings, and the test suite passes completely.

**Why this priority**: Code hygiene ensures maintainability and prevents confusion. Dead code can mislead future contributors and adds unnecessary compilation overhead.

**Independent Test**: Can be tested by running the linter and full test suite, verifying zero warnings and zero test failures.

**Acceptance Scenarios**:

1. **Given** the codebase after command removal, **When** the linter is run with warnings-as-errors, **Then** zero warnings are produced.
2. **Given** the codebase after command removal, **When** the full test suite is run, **Then** all tests pass with no failures.
3. **Given** the codebase after command removal, **When** a developer searches for references to removed commands, **Then** no orphaned code referencing them exists in production code.

---

### User Story 4 - License Alignment (Priority: P2)

The project's license metadata is consistent across all files. The LICENSE file states MIT, and the Cargo.toml license field is updated to match, eliminating the mismatch.

**Why this priority**: License consistency is important for legal clarity and open-source compliance, but is a straightforward metadata fix with no behavioral impact.

**Independent Test**: Can be tested by inspecting the Cargo.toml license field and comparing it to the LICENSE file content.

**Acceptance Scenarios**:

1. **Given** the project repository, **When** a developer inspects Cargo.toml, **Then** the license field reads `MIT`.
2. **Given** the project repository, **When** a developer compares the LICENSE file and Cargo.toml license field, **Then** both indicate MIT.

---

### Edge Cases

- What happens if a user has scripts or automation that call the removed commands? They receive a clear "unrecognized command" error, consistent with standard CLI behavior for unknown subcommands.
- What happens if a user runs `deacon features` (the group itself)? The CLI returns an unrecognized command error since the entire group is removed.
- What happens if documentation or help text still references removed commands? All CLI help text and command registration must be updated to exclude removed commands.
- What happens if internal code paths reference removed command types in match statements or enums? All such references must be removed to prevent compilation errors or dead branches.
- What happens to `templates pull` and `templates apply`? They are retained as consumer-facing commands and continue to function normally.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The CLI MUST NOT register or accept the `features test` subcommand.
- **FR-002**: The CLI MUST NOT register or accept the `features info` subcommand.
- **FR-003**: The CLI MUST NOT register or accept the `features plan` subcommand.
- **FR-004**: The CLI MUST NOT register or accept the `features package` subcommand.
- **FR-005**: The CLI MUST NOT register or accept the `features publish` subcommand.
- **FR-006**: The CLI MUST NOT register or accept the `features pull` subcommand (unimplemented stub).
- **FR-007**: The entire `features` subcommand group MUST be removed from the CLI (no subcommands remain).
- **FR-008**: The CLI MUST NOT register or accept the `templates publish` subcommand.
- **FR-009**: The CLI MUST NOT register or accept the `templates metadata` subcommand.
- **FR-010**: The CLI MUST NOT register or accept the `templates generate-docs` subcommand.
- **FR-011**: The `templates` subcommand group MUST retain only `pull` and `apply` subcommands.
- **FR-012**: The CLI MUST continue to install features from OCI registries during `deacon up` without degradation.
- **FR-013**: All modules, types, functions, and tests exclusively supporting the removed commands MUST be deleted.
- **FR-014**: No orphaned imports, unused variables, or dead code referencing removed commands MUST remain.
- **FR-015**: The Cargo.toml license field MUST read `MIT`, matching the LICENSE file.
- **FR-016**: The project MUST compile with zero linter warnings under the existing strict policy.
- **FR-017**: All existing tests for retained commands and functionality MUST continue to pass.
- **FR-018**: Spec documentation directories under `docs/subcommand-specs/completed-specs/` for removed commands MUST be deleted.
- **FR-019**: References to removed commands in `docs/CLI_PARITY.md` and `docs/ARCHITECTURE.md` MUST be updated or removed.
- **FR-020**: The core library modules `features_info.rs` and `features_test/` MUST be deleted (exclusively serve removed commands).
- **FR-021**: The core library `features/` module MUST be preserved (shared with consumer-side `FeatureInstaller`).

### Scope Boundary

The following items are explicitly **in scope**:
- Removal of all six `features` subcommands (`test`, `info`, `plan`, `package`, `publish`, `pull`) and the entire `features` group
- Removal of three `templates` authoring subcommands (`publish`, `metadata`, `generate-docs`)
- Cleanup of dead code, orphaned modules, and unused test fixtures across both features and templates
- Deletion of spec documentation under `docs/subcommand-specs/completed-specs/` for removed commands (`features-test/`, `features-info/`, `features-plan/`, `features-package/`, `features-publish/`)
- Updating references to removed commands in `docs/CLI_PARITY.md` and `docs/ARCHITECTURE.md`
- CLI help text and command registration updates
- License field alignment in Cargo.toml

The following items are explicitly **out of scope**:
- Changes to the FeatureInstaller or OCI feature installation workflow
- Changes to consumer-facing commands (`up`, `exec`, `read-configuration`, `templates pull`, `templates apply`, etc.)
- Removal of OCI registry client code used by the feature installer or `templates pull`
- Changes to the LICENSE file itself (it already says MIT)
- Adding new commands or features

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The `features` subcommand group does not appear in CLI help output; none of its former subcommands are accepted.
- **SC-002**: The `templates` subcommand group shows only `pull` and `apply` in help output; `publish`, `metadata`, and `generate-docs` are not accepted.
- **SC-003**: Feature installation during container startup completes successfully for configurations referencing OCI-hosted features.
- **SC-004**: The full test suite passes with zero failures after all changes are applied.
- **SC-005**: The linter produces zero warnings across the entire codebase.
- **SC-006**: The Cargo.toml license field reads `MIT`.
- **SC-007**: No modules, functions, or types exclusively serving the removed commands remain in the codebase.

## Assumptions

- The LICENSE file already contains the correct MIT license text and does not need modification.
- The entire `features` subcommand group is removed since no subcommands remain after removal of all six variants.
- Existing CI pipelines and quality gates will validate the changes against the same standards applied to all other contributions.
- No downstream tools or integrations depend on the removed authoring commands in a way that would require migration support.
- The OCI registry client code (used for fetching features) is shared with consumer functionality and is not considered dead code even if authoring commands also used it.
