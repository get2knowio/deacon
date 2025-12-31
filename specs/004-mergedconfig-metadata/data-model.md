# Data Model: Enriched mergedConfiguration metadata for up

## Entities

### MergedConfiguration
- **Description**: The resolved configuration returned by `up` when `includeMergedConfig` is requested, combining base devcontainer config with derived feature metadata and image/container label metadata.
- **Key Fields (aligned to spec)**:
  - `features`: Base feature configuration after resolution.
  - `featureMetadata[]`: Ordered list of resolved feature metadata entries (see FeatureMetadataEntry).
  - `customizations` / `remoteUser` / other resolved fields from merge logic (unchanged shape from spec).
  - `imageMetadata`: Label and provenance details derived from image config (see LabelSet; nullable when unavailable).
  - `containerMetadata`: Label and provenance details derived from container/compose services (see LabelSet; nullable when unavailable).
- **Ordering**: Preserve feature resolution order and compose service order as produced by merge logic; do not reorder keys or arrays.
- **Null Handling**: Retain fields with null/empty values when metadata/labels are missing to satisfy schema expectations.

### FeatureMetadataEntry
- **Description**: Metadata for a single resolved feature.
- **Fields**:
  - `id`: Fully qualified feature identifier.
  - `version`: Resolved version string (may be null if absent in source metadata).
  - `name`/`description`/`documentationURL`: Optional descriptive fields from the feature definition.
  - `options`: Map of resolved options (may be empty/null).
  - `installsAfter`: Ordered array of dependency hints (may be empty/null).
  - `dependsOn`: Ordered array of hard dependencies (may be empty/null).
  - `mounts` / `containerEnv` / `customizations` / lifecycle hooks: Optional metadata merged from the feature definition.
  - `provenance`: Source details (e.g., registry reference or local path, compose service if applicable) and resolution order index.
- **Ordering**: List order matches feature resolution; arrays inside retain declaration order from metadata.
- **Null Handling**: Optional fields remain present with null/empty values when absent in the source metadata.

### LabelSet
- **Description**: Labels collected from an image or running container/compose service with provenance.
- **Fields**:
  - `source`: Scope of labels (e.g., `image`, `container`, compose service name).
  - `labels`: Map<string, string> of key/value pairs; null/empty when none are present.
  - `provenance`: Details on how/where labels were collected (image ref, service name, container id).
- **Ordering**: Preserve compose service order; label key ordering follows collected order from merge logic (do not sort).
- **Null Handling**: Keep `labels` field present even when null/empty; retain provenance even when labels are empty if source was inspected.

## Relationships
- `MergedConfiguration.featureMetadata[]` contains FeatureMetadataEntry items in resolved order.
- `MergedConfiguration.imageMetadata` and `MergedConfiguration.containerMetadata` reference LabelSet instances; compose flows may produce one per service, honoring service order.

## Validation Rules
- Schema compliance: field names and nullability must follow the spec; do not omit optional fields when values are absent.
- Provenance: every FeatureMetadataEntry and LabelSet must carry source information when available.
- Determinism: ordering must be stable across runs for identical inputs (features, compose services, labels).***
