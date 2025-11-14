# Data Model: Build Subcommand Parity Closure

## Build Request
- **Description**: Aggregates all inputs required to execute `deacon build`.
- **Fields**:
  - `workspace_folder: PathBuf` – normalized absolute workspace path.
  - `config_file: Option<PathBuf>` – explicit devcontainer configuration file.
  - `image_names: Vec<String>` – ordered list of tags derived from `--image-name`.
  - `push: bool` – indicates registry push requested.
  - `output: Option<String>` – BuildKit export specification.
  - `labels: Vec<(String, String)>` – ordered user-provided metadata labels.
  - `additional_features: Map<String, FeatureConfig>` – merged feature overrides from CLI.
  - `buildkit_mode: BuildKitMode` – auto/enable/disable selection.
  - `platform: Option<String>` – multi-arch target.
  - `cache_from: Vec<String>` / `cache_to: Option<String>` – cache settings.
  - `skip_feature_auto_mapping: bool`
  - `skip_persist_customizations: bool`
  - `experimental_lockfile: bool`
  - `experimental_frozen_lockfile: bool`
  - `omit_syntax_directive: bool`
- **Validation Rules**:
  - Config filename must be `devcontainer.json` or `.devcontainer.json` when provided.
  - `push` and `output` are mutually exclusive.
  - BuildKit-only flags require BuildKit availability.
  - Compose mode forbids `--push`, `--output`, `--cache-to`, `--platform`.

## Image Artifact
- **Description**: Represents the outputs produced by the build.
- **Fields**:
  - `tags: Vec<String>` – deterministic fallback plus user-specified tags.
  - `metadata_label: DevcontainerMetadata` – serialized devcontainer metadata JSON.
  - `user_labels: Vec<(String, String)>` – labels supplied by `--label`.
  - `export_path: Option<PathBuf>` – path to exported artifact (`--output`).
  - `pushed: bool` – indicates whether image was pushed to registry.
- **Validation Rules**:
  - All tags must parse as valid image references.
  - Metadata label must serialize to UTF-8 JSON (size capped per Docker limits).

## Feature Manifest
- **Description**: Canonical record of features applied during build.
- **Fields**:
  - `install_order: Vec<FeatureRef>` – resolved in execution order.
  - `customizations: Map<String, serde_json::Value>` – persisted customizations (optional when skip flag set).
  - `build_contexts: Vec<FeatureBuildContext>` – BuildKit contexts required for features.
  - `security_opts: Vec<String>` – container security options appended during build.
  - `lockfile_state: Option<FeatureLockfile>` – generated or validated lockfile payloads.
- **Validation Rules**:
  - Missing or disallowed features emit explicit errors before build.
  - Lockfile mismatches fail when `experimental_frozen_lockfile` is set.

## Validation Event
- **Description**: Captures CLI and configuration validation outcomes.
- **Fields**:
  - `code: ValidationCode` – enum identifying rule breached.
  - `message: String` – human-readable error message matching spec.
  - `description: Option<String>` – supplemental context for JSON error payloads.
  - `category: ValidationCategory` – Input, BuildKit, Compose, Feature, or Runtime.
- **State Transitions**:
  - Events accumulate during preflight; any error transitions execution into failure path emitting spec-compliant JSON.
