# Research: Enriched mergedConfiguration metadata for up

## Decision 1: Reuse read_configuration merge logic for up (single and compose)
- **Decision**: Use the existing read_configuration metadata merge path (feature metadata generation and image/container label merge) for both single and compose `up` flows.
- **Rationale**: Ensures spec-parity, consistent provenance/order/null handling, and avoids divergence between commands; minimizes new code and leverages tested logic.
- **Alternatives considered**: 
  - Duplicate merge logic inside `up` for compose-only paths (rejected: high drift risk, violates shared helper principle).
  - Implement a new merge pipeline specialized for `up` (rejected: unnecessary complexity and harder to keep schema aligned).

## Decision 2: Preserve declaration ordering for metadata and labels
- **Decision**: Retain spec-defined ordering (feature resolution order, compose service order) when serializing mergedConfiguration metadata and labels.
- **Rationale**: Ordering is part of the JSON contract and enables deterministic diffs between base and merged configurations.
- **Alternatives considered**:
  - Sort keys alphabetically (rejected: violates spec ordering and would break schema compliance).
  - Leave ordering to map default iteration (rejected: non-deterministic across runtimes/implementations).

## Decision 3: Null/empty semantics for absent metadata or labels
- **Decision**: When metadata or labels are missing, keep fields present with null/empty values per spec instead of omitting entries.
- **Rationale**: Matches spec and user expectations for visibility; allows consumers to detect absence explicitly without brittle key checks.
- **Alternatives considered**:
  - Drop fields when missing (rejected: violates spec, hides provenance and breaks acceptance).
  - Inject default strings like "" (rejected: changes schema types and can mask true absence).

## Decision 4: Provenance annotations
- **Decision**: Carry source/provenance for feature metadata and labels (e.g., registry ref, compose service) through mergedConfiguration.
- **Rationale**: Compliance and auditing scenarios require knowing which source produced each metadata item; prevents ambiguity across services/features.
- **Alternatives considered**:
  - Omit provenance or only track IDs (rejected: insufficient for auditors and may make conflicts untraceable).

## Decision 5: Test coverage focus
- **Decision**: Add/adjust tests that assert mergedConfiguration differs from base when enrichment applies, includes feature metadata keys, carries image/container labels when available, and remains JSON-schema valid for both single and compose paths.
- **Rationale**: Acceptance hinges on schema correctness, ordering, and null handling; tests must guard against regressions.
- **Alternatives considered**:
  - Rely solely on existing read_configuration tests (rejected: up-specific paths and compose handling need direct coverage).

## Decision 6: Phased feature metadata extraction
- **Decision**: Use `from_config_entry()` for MVP, extracting feature ID, options, and order from config. Reserve `from_resolved()` for future integration when resolved `FeatureMetadata` is available in the merged configuration build path.
- **Rationale**: At the point where `build_merged_configuration_with_options` is called, we only have `DevContainerConfig` with `features` as JSON value, not the resolved feature metadata (version, name, description, etc.). Wiring resolved features through the entire up flow requires architectural changes beyond MVP scope.
- **Implementation**:
  - `FeatureMetadataEntry::from_config_entry()` - Extracts ID, options, provenance order from config
  - `FeatureMetadataEntry::from_resolved()` - Full extraction when `FeatureMetadata` is available (future)
- **Future work**: Thread resolved `FeatureMetadata` through compose/single flows to populate version, name, description, documentationUrl, installsAfter, dependsOn, mounts, containerEnv fields.

## Decision 7: Deferred Docker inspect for labels
- **Decision**: Define label metadata structures and integration points now; defer actual Docker inspect calls to future iteration.
- **Rationale**: The data structures (`LabelSet`, `imageMetadata`, `containerMetadata`) and builder pattern are in place. Actual Docker inspection adds runtime complexity and potential failure modes that should be addressed separately with proper error handling and caching.
- **Implementation**:
  - `MergedConfigurationOptions` accepts optional `image_labels` and `container_labels`
  - `LabelSet::from_image()`, `LabelSet::from_container()`, `LabelSet::from_service()` handle construction
  - TODO comments mark integration points for Docker inspect
- **Future work**: Add Docker image/container inspect calls in compose and single-container flows; consider caching label results to avoid repeated inspection.
