# mergedConfiguration Testing & Fixtures Documentation

## Executive Summary
This document catalogs all tests, fixtures, and testing patterns related to `mergedConfiguration` in the Deacon project. The purpose is to prevent regressions when implementing enriched metadata for mergedConfiguration per specs/004-mergedconfig-metadata.

---

## Test Files Touching mergedConfiguration

### 1. `/workspaces/deacon/crates/deacon/tests/up_validation.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/up_validation.rs`  
**Tests**:
- `test_up_result_success_basic_serialization()` (L118-136)
  - Verifies mergedConfiguration is NOT present in basic success output
  - Assertion: `assert!(json.get("mergedConfiguration").is_none())`
  
- `test_up_result_success_with_merged_configuration()` (L172-191)
  - Verifies mergedConfiguration CAN be added via builder method
  - Uses: `UpResult::success().with_merged_configuration(merged_config.clone())`
  - Assertion: `assert_eq!(json["mergedConfiguration"], merged_config)`
  
- `test_up_result_json_roundtrip_success()` (L266-277)
  - Tests JSON serialization/deserialization including mergedConfiguration
  - Verifies: Success JSON is stable across roundtrip

**Key Assertions**:
- mergedConfiguration field NOT included by default
- mergedConfiguration IS included when explicitly set via builder
- Field name is correctly camelCase: "mergedConfiguration"
- Schema: Accepts any serde_json::Value (flexible structure)

**Type Under Test**: `UpResult` struct and serialization

---

### 2. `/workspaces/deacon/crates/deacon/tests/up_prebuild.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/up_prebuild.rs`  
**Tests**:
- `test_prebuild_with_features_metadata_merge()` (L176-203)
  - Command: `up --workspace-folder <fixture> --config <path> --prebuild --include-merged-configuration`
  - Fixture: `/workspaces/deacon/fixtures/devcontainer-up/feature-and-dotfiles/`
  - Assertions:
    - `stdout(predicate::str::contains("outcome").and(predicate::str::contains("success")))`
    - `stdout(predicate::str::contains("mergedConfiguration"))`
  - **Note**: Test expects mergedConfiguration to be present but does NOT validate internal structure
  - Comment: "The mergedConfiguration should include feature metadata; Detailed verification would require parsing JSON and checking feature provenance"

**Key Observations**:
- Uses `--include-merged-configuration` flag (not yet implemented for `up`)
- Validates presence only, not content
- Prebuild mode prerequisite: Features installed and metadata merged BEFORE updateContent
- GAP: No detailed validation of feature metadata structure, ordering, or provenance

---

### 3. `/workspaces/deacon/crates/deacon/tests/integration_read_configuration.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/integration_read_configuration.rs`  
**Tests**:
- `test_helper_with_flags()` (L131-143)
  - Command: `read-configuration --workspace-folder <temp> --include-merged-configuration`
  - Fixture: Minimal devcontainer.json in temp directory
  - Assertions:
    - `assert!(result.get("configuration").is_some())`
    - `assert!(result.get("mergedConfiguration").is_some())`

- `test_acceptance_merged_configuration_output()` (L167-189)
  - Command: `read-configuration --workspace-folder <temp> --include-merged-configuration`
  - Fixture: Simple devcontainer.json with name="test-merged", image="ubuntu:22.04"
  - Assertions:
    - `assert!(result.get("configuration").is_some())`
    - `assert!(result.get("workspace").is_some())`
    - `assert!(result.get("featuresConfiguration").is_some())` (auto-computed per spec)
    - `assert!(result.get("mergedConfiguration").is_some())`
    - Verifies content preservation: name and image match between base and merged
  - Comment: "Currently returns base config as placeholder"

- `test_acceptance_container_only_mode_empty_configuration()` (L323-362)
  - Tests container-only mode where merged config should be present
  - Fixture: No config files, only --id-label provided
  - Validates configuration field is {} when only container selectors provided

**Key Assertions**:
- mergedConfiguration present when --include-merged-configuration flag set
- Not present when flag absent
- Currently mirrors base configuration (placeholder implementation)
- featuresConfiguration auto-included when --include-merged-configuration requested

---

### 4. `/workspaces/deacon/crates/deacon/tests/integration_read_configuration_output.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/integration_read_configuration_output.rs`  
**Tests**:
- `test_merged_configuration_with_flag()` (L198-251)
  - Command without flag: `read-configuration --workspace-folder <temp>`
    - Assertion: `assert!(parsed.get("mergedConfiguration").is_none())`
  - Command with flag: `read-configuration --workspace-folder <temp> --include-merged-configuration`
    - Assertion: `assert!(parsed.get("mergedConfiguration").is_some())`

- `test_features_configuration_included_with_merged()` (L253-294)
  - Validates automatic inclusion of featuresConfiguration when merged is requested
  - Per spec: "features are needed to derive metadata when no container is available"
  - Fixture: Simple devcontainer.json
  - Assertions:
    - `assert!(parsed.get("featuresConfiguration").is_some())`
    - `assert!(parsed.get("mergedConfiguration").is_some())`

- `test_complete_output_structure()` (L296-360)
  - Command: `read-configuration --include-features-configuration --include-merged-configuration`
  - Validates complete output structure with all optional fields
  - Assertions:
    - All expected fields present: configuration, workspace, featuresConfiguration, mergedConfiguration
    - No unexpected fields
    - Validates ordering/structure compliance

**Key Assertions**:
- Presence controlled by --include-merged-configuration flag
- featuresConfiguration automatically included when merged config requested
- Top-level structure validation (presence, absence, no unexpected fields)
- Schema compliance: field names, types, required vs optional

---

### 5. `/workspaces/deacon/crates/deacon/tests/up_config_resolution.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/up_config_resolution.rs`  
**Status**: Placeholder tests - mostly unimplemented
**Tests**:
- `test_image_metadata_merges_into_configuration()` (L50-62)
  - Comment: "Complex integration test" - placeholder
  - Requires:
    1. Test fixture with image
    2. Running `up --include-merged-configuration`
    3. Inspecting JSON to verify merged metadata
  - GAP: Not yet implemented

---

### 6. `/workspaces/deacon/crates/deacon/tests/up_reconnect.rs`
**Location**: `/workspaces/deacon/crates/deacon/tests/up_reconnect.rs`  
**Status**: Mostly ignored (#[ignore] attribute)
**Relevant Tests**:
- `test_expect_existing_fails_fast_with_id_label()` (L19-56)
  - Status: #[ignore] - TODO: Enable when T023 implemented
  - Would validate error JSON structure with container selection

---

## Fixtures for mergedConfiguration Testing

### Primary Test Fixtures

#### 1. `/workspaces/deacon/fixtures/devcontainer-up/feature-and-dotfiles/`
**Purpose**: Tests lifecycle hooks with features (used in prebuild tests)  
**Files**:
- `devcontainer.json` - Configuration with features and lifecycle hooks
  ```json
  {
    "name": "Feature and Dotfiles Test",
    "image": "ubuntu:22.04",
    "remoteUser": "root",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
      "FIXTURE_TYPE": "feature-and-dotfiles"
    },
    "updateContentCommand": "...",
    "postCreateCommand": "..."
  }
  ```
- `README.md` - Documentation

**Used In**:
- `up_prebuild.rs`: `test_prebuild_with_features_metadata_merge()`
- Multiple other prebuild tests

**Test Patterns**:
- Validates features are installed before updateContent runs
- Checks metadata merging in prebuild mode
- Currently only validates mergedConfiguration presence, not structure

#### 2. `/workspaces/deacon/fixtures/devcontainer-up/single-container/`
**Purpose**: Basic single-container setup (no features)  
**Files**:
- `devcontainer.json` - Minimal single container config
  ```json
  {
    "name": "Single Container Test",
    "image": "ubuntu:22.04",
    "remoteUser": "root",
    "workspaceFolder": "/workspace",
    "remoteEnv": {
      "TEST_ENV": "single-container"
    },
    "postCreateCommand": "...",
    "updateContentCommand": "..."
  }
  ```

**Used In**:
- `up_prebuild.rs`: `test_prebuild_without_update_content_command()`

**Test Patterns**:
- Tests prebuild mode without updateContentCommand (no-op lifecycle)
- Tests mergedConfiguration output structure

#### 3. `/workspaces/deacon/fixtures/devcontainer-up/compose-with-profiles/`
**Purpose**: Docker Compose with service profiles  
**Files**:
- `devcontainer.json` - Compose-based config
- `docker-compose.yml` - Compose file
- `.env` - Environment variables

**Used In**:
- Compose integration tests (mentioned but not deeply used for merged config)

---

### Ad-Hoc Test Fixtures

#### Temporary Fixtures Created in Tests
Tests create minimal fixtures on-the-fly using `tempfile::TempDir`:

**Pattern** (from `integration_read_configuration.rs`):
```rust
let helper = ReadConfigurationTestHelper::new()?;
helper.create_config(r#"{"name": "test", "image": "ubuntu:22.04"}"#)?;
let result = helper.run_with_workspace(&["--include-merged-configuration"])?;
assert!(result.get("mergedConfiguration").is_some());
```

**Advantages**:
- Isolated, hermetic tests
- No shared state
- Easy to parametrize

**Disadvantages**:
- No real features (mocking or real OCI registry calls needed)
- No image metadata simulation
- No compose multi-service scenarios

---

## Example Directory with mergedConfiguration

### `/workspaces/deacon/examples/up/configuration-output/`
**Purpose**: Real-world example demonstrating --include-merged-configuration  
**Documentation**: `/workspaces/deacon/examples/up/configuration-output/README.md`

**Key Sections**:
1. **Configuration Output Types**:
   - Base Configuration: Original devcontainer.json
   - Merged Configuration: After features, metadata, overrides, substitution

2. **Usage Examples**:
   - `deacon up --workspace-folder . --include-configuration`
   - `deacon up --workspace-folder . --include-merged-configuration`
   - `deacon up --workspace-folder . --include-configuration --include-merged-configuration`

3. **Use Cases**:
   - Configuration debugging
   - Feature impact analysis
   - CI/CD validation
   - Documentation generation
   - Configuration diffing

4. **Expected Output**:
   ```json
   {
     "outcome": "success",
     "containerId": "<container-id>",
     "remoteUser": "root",
     "remoteWorkspaceFolder": "/workspace",
     "configuration": { "...": "original content" },
     "mergedConfiguration": { "...": "final after processing" }
   }
   ```

**Note**: This is documentation only; no automated tests yet validate this example's output.

---

## Current Test Coverage Analysis

### What IS Tested
1. **Field Presence**: mergedConfiguration is/isn't present based on flags
2. **Serialization**: JSON roundtrip without errors
3. **Flag Behavior**: --include-merged-configuration flag parsing
4. **Output Structure**: Top-level field names and types
5. **Placeholder Content**: Basic config reflection (no metadata enrichment)

### What IS NOT Tested (Gaps)

#### A. Feature Metadata Enrichment
- **Missing**: Validation that mergedConfiguration contains feature metadata entries
- **Missing**: Ordering of features in metadata array
- **Missing**: Provenance information in feature entries
- **Missing**: Optional field null-handling (per spec)
- **Spec Requirement**: FR-001, SC-001, SC-003

#### B. Image/Container Label Metadata
- **Missing**: Validation that mergedConfiguration includes image labels
- **Missing**: Validation that mergedConfiguration includes container labels
- **Missing**: Label source provenance tracking
- **Missing**: Single vs compose label aggregation
- **Spec Requirement**: FR-002, SC-002

#### C. Schema Compliance
- **Missing**: JSON schema validation against spec
- **Missing**: Field nullability validation
- **Missing**: Type checking for all nested fields
- **Missing**: additionalProperties handling
- **Spec Requirement**: FR-006, SC-003

#### D. Compose-Specific Scenarios
- **Missing**: Multi-service label merging and ordering
- **Missing**: Per-service provenance capture
- **Missing**: Service order preservation
- **Spec Requirement**: FR-004, SC-002

#### E. Edge Cases
- **Missing**: Devcontainers with NO features (empty metadata arrays)
- **Missing**: Images with NO labels (null vs empty)
- **Missing**: Missing optional feature metadata fields (null handling)
- **Missing**: Conflicting labels across services
- **Spec Requirement**: FR-003, Spec Edge Cases

#### F. Integration Tests
- **Missing**: Real Docker container inspection (labels from running container)
- **Missing**: Real feature installation with metadata extraction
- **Missing**: Real image metadata merging
- **Missing**: End-to-end prebuild flow with mergedConfiguration output

#### G. `up` Command Implementation
- **Status**: --include-merged-configuration flag NOT yet implemented for `up`
- **Current**: Only `read-configuration` implements the flag
- **Gap**: `up_prebuild.rs` tests use flag but command doesn't support it yet
- **Task**: Implementation pending (specs/004-mergedconfig-metadata)

---

## Data Model & Schema Reference

### MergedConfiguration Structure (from spec)
```typescript
MergedConfiguration {
  features?: object;
  featureMetadata?: FeatureMetadataEntry[];
  imageMetadata?: LabelSet | LabelSet[];
  containerMetadata?: LabelSet | LabelSet[];
  // ... plus all other resolved config fields
}

FeatureMetadataEntry {
  id: string;
  version?: string;
  name?: string;
  description?: string;
  documentationURL?: string;
  options?: object;
  installsAfter?: string[];
  dependsOn?: string[];
  mounts?: object[];
  containerEnv?: object;
  customizations?: object;
  provenance?: object;
}

LabelSet {
  source: string;        // e.g., "image", "container", service name
  labels?: object;       // map<string, string>
  provenance?: object;   // collection source details
}
```

### Serialization Names (camelCase per spec)
- `featureMetadata` - feature metadata array
- `imageMetadata` - image label metadata
- `containerMetadata` - container/compose metadata
- Feature fields: id, version, name, description, documentationURL, options, etc.
- Label fields: source, labels, provenance

---

## Test Execution Patterns

### Current Test Groups (from .config/nextest.toml)
- `integration_read_configuration.rs`: Part of default test suite
- `integration_read_configuration_output.rs`: Part of default test suite
- `up_prebuild.rs`: Part of docker-shared group (can run in parallel)
- `up_validation.rs`: Part of unit tests (fast, no docker)
- `up_config_resolution.rs`: Part of default suite

### Recommended Test Group for New mergedConfiguration Tests
- **Unit Tests** (no docker): Schema validation, serialization
- **docker-shared**: read-configuration + up commands (parallel-4 safe)
- **smoke** (serial): End-to-end scenarios with real features

---

## Commands & Flags Involved

### read-configuration
- `--include-merged-configuration` - IMPLEMENTED
  - Automatically includes --include-features-configuration
  - Provides full configuration after merge
- `--workspace-folder` - IMPLEMENTED
- `--container-id` / `--id-label` - PARTIALLY IMPLEMENTED

### up
- `--include-merged-configuration` - NOT YET IMPLEMENTED (pending T###)
  - Listed in tests but command doesn't support yet
  - Expectation: Same behavior as read-configuration
  - Should trigger feature metadata + label metadata collection

---

## Key Implementation Files

### Commands
- `/workspaces/deacon/crates/deacon/src/commands/read_configuration.rs` - Implements mergedConfiguration for read-configuration
- `/workspaces/deacon/crates/deacon/src/commands/up.rs` - UpResult struct with builder methods

### Core Logic
- `/workspaces/deacon/crates/core/src/config.rs` - Config resolution
- `/workspaces/deacon/crates/core/src/features/` - Feature installation and metadata
- `/workspaces/deacon/crates/core/src/container_env_probe.rs` - Environment/label collection

### Specifications
- `/workspaces/deacon/specs/004-mergedconfig-metadata/spec.md` - Feature specification
- `/workspaces/deacon/specs/004-mergedconfig-metadata/data-model.md` - Data model
- `/workspaces/deacon/specs/004-mergedconfig-metadata/contracts/up-mergedconfiguration.yaml` - JSON schema

---

## Regression Prevention Checklist

When implementing mergedConfiguration enrichment:

### Before Implementation
- [ ] Read full spec: specs/004-mergedconfig-metadata/spec.md
- [ ] Review data model: specs/004-mergedconfig-metadata/data-model.md
- [ ] Check schema: specs/004-mergedconfig-metadata/contracts/up-mergedconfiguration.yaml
- [ ] Verify all test files mentioned above
- [ ] Check placeholder tests that need enabling

### Feature Metadata Tests
- [ ] Test feature ordering matches resolution order
- [ ] Test provenance capture (registry, version, service if any)
- [ ] Test null-handling for optional fields
- [ ] Test empty features list (returns empty array, not omitted)
- [ ] Test feature without optional metadata fields

### Label Metadata Tests
- [ ] Test image labels included when available
- [ ] Test container labels included when available
- [ ] Test label source field populated correctly
- [ ] Test label provenance captured
- [ ] Test null labels field when no labels present
- [ ] Test compose: labels per service with service order preserved

### Integration Tests
- [ ] Test with real feature from OCI registry
- [ ] Test with real image labels
- [ ] Test compose with multiple services
- [ ] Test prebuild mode with metadata output
- [ ] Test up command with --include-merged-configuration

### Schema Compliance Tests
- [ ] JSON schema validation
- [ ] Field name camelCase validation
- [ ] Required field presence
- [ ] Optional field null/omission handling
- [ ] Type validation (arrays vs objects, etc.)
- [ ] Ordering preservation

### Backward Compatibility
- [ ] Ensure existing tests still pass
- [ ] Ensure mergedConfiguration absent when flag not set
- [ ] Ensure base configuration unchanged
- [ ] Ensure JSON output still valid without metadata fields

---

## References

### Specification Files
- `/workspaces/deacon/specs/004-mergedconfig-metadata/spec.md` - User scenarios, requirements, success criteria
- `/workspaces/deacon/specs/004-mergedconfig-metadata/data-model.md` - Entity definitions, ordering, null semantics
- `/workspaces/deacon/specs/004-mergedconfig-metadata/contracts/up-mergedconfiguration.yaml` - OpenAPI schema

### Test Files
- `/workspaces/deacon/crates/deacon/tests/up_validation.rs` - UpResult serialization tests
- `/workspaces/deacon/crates/deacon/tests/integration_read_configuration.rs` - read-configuration tests with merged config
- `/workspaces/deacon/crates/deacon/tests/integration_read_configuration_output.rs` - Output structure tests
- `/workspaces/deacon/crates/deacon/tests/up_prebuild.rs` - Prebuild with merged config tests

### Fixture Directories
- `/workspaces/deacon/fixtures/devcontainer-up/feature-and-dotfiles/` - Features + lifecycle
- `/workspaces/deacon/fixtures/devcontainer-up/single-container/` - Basic single container
- `/workspaces/deacon/fixtures/devcontainer-up/compose-with-profiles/` - Docker Compose

### Examples
- `/workspaces/deacon/examples/up/configuration-output/` - Documentation and usage examples

