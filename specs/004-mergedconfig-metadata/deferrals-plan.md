# Plan: Complete Deferred Work for mergedConfiguration Metadata

This plan addresses the two deferrals documented in `research.md` (Decisions 6 and 7).

## Overview of Deferrals

### Deferral 1: Phased Feature Metadata Extraction (Decision 6)
**Current State**: `FeatureMetadataEntry::from_config_entry()` extracts only ID, options, and order.
**Goal**: Use `from_resolved()` to populate full metadata (version, name, description, documentationUrl, installsAfter, dependsOn, mounts, containerEnv).

### Deferral 2: Docker Inspect for Labels (Decision 7)
**Current State**: Infrastructure ready (`LabelSet`, `MergedConfigurationOptions`) but actual Docker inspect calls deferred.
**Goal**: Add `docker image inspect` and `docker container inspect` calls to populate labels.

---

## Implementation Plan

### Phase 1: Add Image Inspection Capability

#### Task 1.1: Add `inspect_image` to Docker trait
**File**: `crates/core/src/docker.rs`

Add new method to `Docker` trait (similar to existing `inspect_container`):
```rust
/// Inspect a specific image by reference and return its labels
async fn inspect_image(&self, image_ref: &str) -> Result<Option<ImageInfo>>;
```

Add `ImageInfo` struct:
```rust
pub struct ImageInfo {
    pub id: String,
    pub labels: HashMap<String, String>,
}
```

#### Task 1.2: Implement `inspect_image` for CliDocker
**File**: `crates/core/src/docker.rs`

Implementation:
```rust
async fn inspect_image(&self, image_ref: &str) -> Result<Option<ImageInfo>> {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", "{{json .Config.Labels}}", image_ref])
        .output()
        .await?;
    // Parse labels from JSON output
}
```

#### Task 1.3: Implement `inspect_image` for MockDocker
**File**: `crates/core/src/docker.rs`

Add mock implementation for testing.

#### Task 1.4: Add unit tests for image inspection
**File**: `crates/core/src/docker.rs`

Test parsing of label JSON, empty labels, and error cases.

---

### Phase 2: Wire Container Labels into mergedConfiguration

#### Task 2.1: Extract labels from ContainerInfo after inspect
**File**: `crates/deacon/src/commands/up.rs`

At the call sites (lines ~1912, ~2302), `ContainerInfo` already contains labels from inspect. Wire them through:

```rust
// Single container flow (line ~2302)
let container_info = docker.inspect_container(&container_result.container_id).await?;
let container_labels = container_info.map(|info| info.labels);

let options = MergedConfigurationOptions {
    image_labels: None, // Phase 3
    image_ref: config.image.clone(),
    container_labels,  // Now populated!
    container_id: Some(container_result.container_id.clone()),
    service_name: None,
};
```

Similar change for compose flow (line ~1912).

#### Task 2.2: Add integration test for container labels
**File**: `crates/deacon/tests/up_merged_configuration.rs`

Test that `containerMetadata.labels` is populated when container has labels.

---

### Phase 3: Wire Image Labels into mergedConfiguration

#### Task 3.1: Call `inspect_image` before building mergedConfiguration
**File**: `crates/deacon/src/commands/up.rs`

For single container flow:
```rust
let image_info = docker.inspect_image(config.image.as_deref().unwrap_or("")).await?;
let image_labels = image_info.and_then(|info| {
    if info.labels.is_empty() { None } else { Some(info.labels) }
});

let options = MergedConfigurationOptions {
    image_labels,  // Now populated!
    image_ref: config.image.clone(),
    container_labels,
    container_id: Some(container_result.container_id.clone()),
    service_name: None,
};
```

For compose flow, use the service's image.

#### Task 3.2: Add integration test for image labels
**File**: `crates/deacon/tests/up_merged_configuration.rs`

Test that `imageMetadata.labels` is populated when image has labels.

---

### Phase 4: Thread Resolved Feature Metadata

#### Task 4.1: Extend `build_merged_configuration_with_options` signature
**File**: `crates/deacon/src/commands/up.rs`

Add optional `resolved_features` parameter:
```rust
fn build_merged_configuration_with_options(
    config: &DevContainerConfig,
    config_path: &Path,
    options: MergedConfigurationOptions,
    resolved_features: Option<&[ResolvedFeature]>,  // New parameter
) -> Result<serde_json::Value>
```

#### Task 4.2: Create new extraction function using `from_resolved()`
**File**: `crates/deacon/src/commands/up.rs`

```rust
fn extract_feature_metadata_from_resolved(
    features: &[ResolvedFeature],
    service: Option<String>,
) -> Vec<FeatureMetadataEntry> {
    features
        .iter()
        .enumerate()
        .map(|(order, f)| {
            let options = serde_json::to_value(&f.options).ok();
            FeatureMetadataEntry::from_resolved(
                f.id.clone(),
                f.source.clone(),
                options,
                &f.metadata,
                order,
                service.clone(),
            )
        })
        .collect()
}
```

#### Task 4.3: Update build function to prefer resolved features
**File**: `crates/deacon/src/commands/up.rs`

```rust
let feature_metadata = if let Some(resolved) = resolved_features {
    extract_feature_metadata_from_resolved(resolved, options.service_name.clone())
} else {
    extract_feature_metadata_from_config(&config.features)
};
```

#### Task 4.4: Pass InstallationPlan through to mergedConfiguration
**File**: `crates/deacon/src/commands/up.rs`

At BuildKit feature installation (line ~2685), the `installation_plan` is available:
```rust
// After feature installation completes, pass resolved features
let merged_configuration = if args.include_merged_configuration {
    let options = MergedConfigurationOptions { ... };
    Some(build_merged_configuration_with_options(
        &config,
        config_path,
        options,
        Some(&installation_plan.features),  // Pass resolved features
    )?)
} else {
    None
};
```

#### Task 4.5: Handle compose flow
**File**: `crates/deacon/src/commands/up.rs`

For compose flow, features may be resolved separately. Pass them when available.

#### Task 4.6: Add tests for full feature metadata
**File**: `crates/deacon/tests/up_merged_configuration.rs`

Test that `featureMetadata` entries include:
- `version` from resolved metadata
- `name` from resolved metadata
- `description` from resolved metadata
- `documentationUrl` from resolved metadata
- `installsAfter` array
- `dependsOn` array
- `mounts` array
- `containerEnv` map
- `provenance.source` (registry reference)

---

### Phase 5: Fix Reconnect/Expect-Existing Flow

#### Task 5.1: Update placeholder in reconnect flow
**File**: `crates/deacon/src/commands/up.rs` (line ~1759)

The TODO comment notes this needs proper feature metadata. Since we won't have resolved features on reconnect (no installation happens), use `from_config_entry()` path but ensure proper label collection:

```rust
let merged_configuration = if args.include_merged_configuration {
    let container_info = docker.inspect_container(&container_id).await?;
    let container_labels = container_info.map(|info| info.labels);

    let options = MergedConfigurationOptions {
        image_labels: None, // Could inspect image if available
        image_ref: config.image.clone(),
        container_labels,
        container_id: Some(container_id.clone()),
        service_name: None,
    };
    Some(build_merged_configuration_with_options(&config, config_path, options, None)?)
} else {
    None
};
```

---

## Task Summary

| # | Task | Files | Complexity |
|---|------|-------|------------|
| 1.1 | Add `inspect_image` to Docker trait | docker.rs | Medium |
| 1.2 | Implement for CliDocker | docker.rs | Medium |
| 1.3 | Implement for MockDocker | docker.rs | Low |
| 1.4 | Unit tests for image inspection | docker.rs | Low |
| 2.1 | Wire container labels | up.rs | Low |
| 2.2 | Integration test for container labels | up_merged_configuration.rs | Low |
| 3.1 | Wire image labels | up.rs | Medium |
| 3.2 | Integration test for image labels | up_merged_configuration.rs | Low |
| 4.1 | Extend builder signature | up.rs | Low |
| 4.2 | Create `extract_feature_metadata_from_resolved` | up.rs | Medium |
| 4.3 | Update build function logic | up.rs | Low |
| 4.4 | Pass InstallationPlan to builder | up.rs | Medium |
| 4.5 | Handle compose flow | up.rs | Medium |
| 4.6 | Tests for full feature metadata | up_merged_configuration.rs | Medium |
| 5.1 | Fix reconnect flow | up.rs | Low |

---

## Dependencies

```
Phase 1 (Image Inspection)
    └── Phase 3 (Image Labels)

Phase 2 (Container Labels) - Independent

Phase 4 (Resolved Features) - Independent
    └── Phase 5 (Reconnect Flow uses patterns from Phase 4)
```

Phases 1-3 and Phase 4-5 can proceed in parallel.

---

## Acceptance Criteria

1. **Container Labels**: `mergedConfiguration.containerMetadata.labels` populated from `docker container inspect`
2. **Image Labels**: `mergedConfiguration.imageMetadata.labels` populated from `docker image inspect`
3. **Full Feature Metadata**: When features are resolved, `featureMetadata` entries include:
   - `version`, `name`, `description`, `documentationUrl`
   - `installsAfter`, `dependsOn`, `mounts`, `containerEnv`
   - `provenance.source` with registry reference
4. **All existing tests pass**
5. **Schema compliance** verified
6. **No regressions** in existing `--include-merged-configuration` output

---

## Risk Mitigation

1. **Docker inspect failures**: Use best-effort approach - if inspect fails, continue with `null` labels
2. **Performance**: Cache image inspection results since they don't change during `up`
3. **Compose complexity**: May not have resolved features for all services; fall back to config extraction
4. **Breaking changes**: Ensure additional fields are additive (existing consumers ignore unknown fields)
