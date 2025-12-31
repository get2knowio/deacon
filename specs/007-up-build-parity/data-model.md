# Data Model: Up Build Parity and Metadata

## Entities

### BuildOptions
- cache_from: ordered list of cache sources supplied by user; preserved order used when invoking BuildKit/buildx.
- cache_to: ordered list of cache destinations supplied by user.
- builder: optional buildx/builder selection applied to Dockerfile and feature builds.
- buildkit_required: flag indicating BuildKit/buildx availability is mandatory; failures are surfaced before build.
- scope: applies to the entire `up` run and must be threaded to both Dockerfile and feature builds.

### FeatureControl
- skip_feature_auto_mapping: boolean gate that blocks any auto-added features beyond explicit declarations.
- lockfile_path: resolved path to the feature lockfile used to validate resolved features.
- frozen: boolean requiring exact match to the lockfile without updates.
- resolution_status: derived state (matched | missing_lockfile | mismatch) determining whether builds proceed.

### FeatureMetadata
- feature_ref: identifier for the feature as resolved (name/version/source as defined by spec ordering).
- metadata: map/object of metadata fields emitted by the feature; may be empty when none supplied.
- origin: indicates whether metadata came from a feature build or Dockerfile layer annotation when applicable.

### MergedConfiguration
- features: ordered list of resolved features (respecting declaration order) with associated FeatureMetadata entries.
- build_options_applied: record of the BuildOptions actually used for Dockerfile and feature builds.
- enforcement: markers showing whether skip-feature-auto-mapping, lockfile, and frozen were enforced (and their result).
- errors: optional structured errors captured before build (e.g., lockfile mismatch) to support fail-fast messaging.

## Relationships and Rules
- BuildOptions must be applied to both Dockerfile and feature builds; no divergence unless a fail-fast condition is triggered.
- FeatureControl governs which features populate `MergedConfiguration.features`; skip_feature_auto_mapping prevents additions beyond declarations.
- FeatureMetadata entries must exist for every feature in `MergedConfiguration.features`; if a feature emits none, metadata remains an empty map/object but is still present.
- resolution_status == mismatch or missing_lockfile halts progression to build and surfaces errors; successful status allows merged configuration emission.
- Ordering from the user configuration/lockfile must be preserved when serializing mergedConfiguration outputs.
