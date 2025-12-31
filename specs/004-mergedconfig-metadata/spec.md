# Feature Specification: Enriched mergedConfiguration metadata for up

**Feature Branch**: `001-mergedconfig-metadata`  
**Created**: 2025-11-26  
**Status**: Draft  
**Input**: User description: "In our journey to compliance with docs/repomix-output-devcontainers-cli.xml we need to implement: Spec enriched mergedConfiguration for up: include feature metadata and image/container metadata (labels) with provenance, ordering, and null semantics per spec. Reuse read_configuration merge logic (feature metadata generation + image/container label merge) for compose/single paths. Acceptance: mergedConfiguration differs from base; includes feature metadata keys; includes image labels when available; JSON schema compliance."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Verify feature metadata presence (Priority: P1)

Devcontainer users run `up` and inspect mergedConfiguration to confirm every applied feature shows metadata, provenance, and ordering so they can trust what will be built.

**Why this priority**: This is the core compliance and trust story; without visible feature metadata the merged output cannot be validated.

**Independent Test**: Run `up` on a devcontainer with multiple features and confirm mergedConfiguration lists feature metadata with provenance and order.

**Acceptance Scenarios**:

1. **Given** a devcontainer with multiple features, **When** `up` is executed, **Then** mergedConfiguration lists each feature's metadata fields defined by the spec in the resolved order.
2. **Given** a feature missing optional metadata, **When** `up` is executed, **Then** mergedConfiguration includes the feature entry with nulls for missing fields instead of dropping the entry.

---

### User Story 2 - Capture image and container labels (Priority: P2)

Platform auditors run `up` for single and compose projects and need mergedConfiguration to include available image/container labels with source information to verify compliance tagging.

**Why this priority**: Compliance checks depend on seeing labels and their provenance across deployment types.

**Independent Test**: Execute `up` on single and compose configs with known labels and verify mergedConfiguration surfaces those labels with provenance.

**Acceptance Scenarios**:

1. **Given** an image with labels, **When** `up` is executed, **Then** mergedConfiguration includes those labels with source noted.
2. **Given** a compose service without labels, **When** `up` is executed, **Then** mergedConfiguration includes the labels section with null/empty values as defined by the spec.

---

### User Story 3 - Compare base vs merged configuration (Priority: P3)

Tooling integrators compare base configuration to mergedConfiguration to confirm enrichment steps and schema correctness across flows.

**Why this priority**: Downstream tools need consistent, schema-compliant output that clearly shows enrichment.

**Independent Test**: Generate base and merged configurations for single and compose setups and verify differences reflect added metadata while remaining schema-valid.

**Acceptance Scenarios**:

1. **Given** a base configuration without metadata, **When** mergedConfiguration is produced, **Then** the output differs by including feature and label metadata while retaining schema validity.
2. **Given** identical inputs across single and compose runs, **When** mergedConfiguration is produced, **Then** metadata handling (ordering, nulls, provenance) is consistent across both paths.

### Edge Cases

- Devcontainers with no features still emit a mergedConfiguration with empty or null metadata fields per spec.
- Images or containers lacking labels keep metadata sections present with null/empty placeholders instead of being omitted.
- Conflicting or duplicate labels across services follow spec-defined ordering/prioritization to remain deterministic.
- Compose files with multiple services maintain per-service provenance and ordering when merging metadata.
- Base configuration already containing metadata is reconciled so mergedConfiguration shows the resolved values and provenance.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mergedConfiguration MUST include feature metadata entries for every resolved feature, using the spec-defined fields, provenance, and deterministic ordering.
- **FR-002**: mergedConfiguration MUST surface available image and container labels for single and compose flows, annotated with their source.
- **FR-003**: When metadata is absent or partially available, mergedConfiguration MUST retain required fields with null or empty values according to the spec rather than omitting them.
- **FR-004**: The merge behavior for metadata MUST be identical across compose and single workflows by reusing the same configuration merge rules.
- **FR-005**: mergedConfiguration MUST differ from the base configuration whenever metadata is added or reconciled, making enrichment observable.
- **FR-006**: mergedConfiguration output MUST remain compliant with the documented JSON schema, including required keys and ordering expectations.

### Key Entities

- **Merged Configuration**: The resolved configuration produced by `up`, including base settings plus enriched metadata and labels.
- **Feature Metadata**: Structured details about each applied feature (e.g., identifiers, provenance, ordering, optional fields).
- **Image and Container Labels**: Key-value metadata derived from images and containers, with information about their source within single or compose setups.

### Assumptions

- docs/repomix-output-devcontainers-cli.xml defines the authoritative JSON schema, metadata fields, and ordering rules.
- Existing configuration loading and merge logic can be reused without altering upstream inputs.
- Images, containers, or features may omit optional metadata; null or empty placeholders are acceptable per the spec.
- Compose setups may include multiple services; each service's metadata and labels are captured independently with provenance.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In 100% of `up` runs with features, mergedConfiguration lists feature metadata entries with required keys, provenance, and ordering per spec.
- **SC-002**: For images or containers that expose labels, 100% of mergedConfiguration outputs include those labels with source annotations; when labels are missing, fields remain present with null/empty values per spec.
- **SC-003**: mergedConfiguration passes JSON schema validation for the documented spec across 100% of covered single and compose fixtures.
- **SC-004**: When comparing base vs merged outputs in tests, 100% of cases with metadata or labels show observable differences attributable to enrichment, with no regression in ordering or null-handling rules.
