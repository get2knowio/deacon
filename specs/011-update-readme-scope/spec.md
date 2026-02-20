# Feature Specification: Update README for Consumer-Only Scope

**Feature Branch**: `011-update-readme-scope`
**Created**: 2026-02-20
**Status**: Draft
**Input**: User description: "Update Deacon's README to reflect its new consumer-only scope after removing feature-authoring commands."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - New Visitor Understands Deacon's Purpose (Priority: P1)

A developer visits the Deacon GitHub repository for the first time and reads the README. They immediately understand that Deacon is a focused, consumer-oriented DevContainer CLI for developers who use dev containers and CI pipelines — not a tool for feature authors.

**Why this priority**: The README is the project's front door. If the positioning is wrong or stale, every new visitor gets a misleading impression of what Deacon does.

**Independent Test**: Can be tested by reading the README top-to-bottom and verifying the positioning statement, feature list, and "In Progress" table all consistently reflect a consumer-only tool.

**Acceptance Scenarios**:

1. **Given** a visitor opens the README, **When** they read the project description, **Then** they see positioning that communicates Deacon is "The DevContainer CLI, minus the parts you don't use" — a fast, focused Rust CLI for developers who use dev containers and CI pipelines, not for feature authors.
2. **Given** a visitor reads the README, **When** they look for feature-authoring capabilities (features test, info, plan, package, publish), **Then** they find no references to these commands anywhere in the document.

---

### User Story 2 - Existing User Sees Accurate Command List (Priority: P1)

A developer already using Deacon checks the README to see what commands are available. The feature list accurately reflects the shipped commands: up, down, exec, build, read-configuration, run-user-commands, templates apply, and doctor.

**Why this priority**: An inaccurate command list erodes trust and causes confusion. Users need to know exactly what the tool ships.

**Independent Test**: Can be tested by comparing the README's listed commands against the actual CLI output (`deacon --help`) and verifying parity.

**Acceptance Scenarios**:

1. **Given** a user reads the README, **When** they look at the feature/command list, **Then** they see exactly: up, down, exec, build, read-configuration, run-user-commands, templates apply, and doctor.
2. **Given** a user reads the README, **When** they search for any feature-authoring commands (features test, features info, features plan, features package, features publish), **Then** they find zero references.

---

### User Story 3 - CI/Automation User Sees Current Status (Priority: P2)

A DevOps engineer evaluating Deacon for CI pipeline integration reads the "In Progress" table to understand what's stable and what's coming. The table contains no rows referencing feature-authoring capabilities and accurately reflects the current development status.

**Why this priority**: The "In Progress" table sets expectations for what works today vs. what's coming. Stale rows referencing removed capabilities mislead evaluators.

**Independent Test**: Can be tested by reviewing the "In Progress" table and verifying every listed item corresponds to an actual planned capability (not a removed one).

**Acceptance Scenarios**:

1. **Given** an engineer reads the "In Progress" table, **When** they review each row, **Then** no row references feature-authoring commands or capabilities.
2. **Given** an engineer reads the "In Progress" table, **When** they check the listed items, **Then** each item accurately reflects current development status.

---

### User Story 4 - No Broken Links or Stale References (Priority: P2)

Any visitor navigating the README finds that all badges, CI links, installation instructions, and internal references remain intact and functional after the update.

**Why this priority**: Broken links and stale references signal an unmaintained project and frustrate users trying to install or evaluate the tool.

**Independent Test**: Can be tested by verifying all links in the README resolve correctly and that badge URLs, CI references, and installation instructions are unchanged from the pre-update version.

**Acceptance Scenarios**:

1. **Given** a visitor reads the updated README, **When** they click any badge or link, **Then** all links resolve correctly.
2. **Given** a visitor follows the installation instructions, **When** they compare against the previous version, **Then** all installation content (URLs, commands, environment variables) is identical.

---

### Edge Cases

- What happens if the Roadmap section references feature authoring indirectly (e.g., "Feature system for reusable development environment components")? Update to clarify this refers to feature *consumption* (installing features into dev containers), not authoring.
- What happens if examples references mention feature-authoring workflows? Verify examples content describes feature consumption, not authoring, and update references if needed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: README MUST include the positioning statement: "The DevContainer CLI, minus the parts you don't use." with supporting text describing Deacon as a fast, focused Rust CLI for developers who use dev containers and CI pipelines — not for feature authors.
- **FR-002**: README MUST list exactly these shipped commands: up, down, exec, build, read-configuration, run-user-commands, templates apply, and doctor.
- **FR-003**: README MUST NOT contain any references to feature-authoring commands: features test, features info, features plan, features package, features publish.
- **FR-004**: The "In Progress" table MUST NOT contain any rows related to feature-authoring capabilities.
- **FR-005**: All existing badge URLs, CI references, and installation instructions MUST remain unchanged.
- **FR-006**: The Roadmap section MUST accurately reflect consumer-only scope (feature consumption, not authoring).
- **FR-007**: README MUST preserve its existing structure, tone, and formatting conventions.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of README command references match the actual shipped command set (up, down, exec, build, read-configuration, run-user-commands, templates apply, doctor) with zero references to removed feature-authoring commands.
- **SC-002**: A reader can determine Deacon's consumer-only positioning within the first 3 lines of the README.
- **SC-003**: Zero broken links or stale references exist in the updated README.
- **SC-004**: All badge URLs, CI links, and installation instructions are byte-identical to the pre-update version.
- **SC-005**: The "In Progress" table contains zero rows referencing feature-authoring capabilities.

## Assumptions

- The feature-authoring commands (features test, info, plan, package, publish) have already been fully removed from the codebase in a prior change (branch 010-remove-feature-authoring).
- The existing README structure (sections, ordering, formatting) is considered good and should be preserved.
- "Consumer-only scope" means Deacon still installs/resolves features as part of `up` and `build` commands — it just doesn't provide tools for *authoring* features.
- The Examples section references to the Feature System are about feature *consumption* (installing features into containers) and remain valid.
