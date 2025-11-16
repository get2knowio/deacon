# Build Subcommand Implementation Gap Analysis

**Generated:** October 13, 2025  
**Specification Version:** Based on docs/subcommand-specs/build/  
**Implementation Location:** crates/deacon/src/commands/build.rs, crates/deacon/src/cli.rs

---

## Executive Summary

This document analyzes the current implementation of the `deacon build` subcommand against the official specification in `docs/subcommand-specs/build/`. The analysis identifies missing features, incorrect implementations, and deviations from the specification.

**Overall Assessment:** The current implementation provides basic build functionality but is **missing critical features** required by the specification, particularly around:
- Image tagging with `--image-name` (repeatable)
- Push to registry with `--push` 
- BuildKit output control with `--output`
- Proper Compose configuration handling
- Metadata label injection
- JSON output format on stdout
- Several validation and error handling requirements

---

## 1. Command-Line Interface Gaps

### 1.1 Missing Required Flags

| Flag | Status | Notes |
|------|--------|-------|
| `--image-name <name[:tag]>` | ❌ **MISSING** | Repeatable flag for final image tags. Critical for registry workflows. |
| `--push` | ❌ **MISSING** | BuildKit-only flag to push built image to registry. |
| `--output <spec>` | ❌ **MISSING** | BuildKit-only flag for custom output (e.g., `type=oci,dest=out.tar`). |
| `--label <name=value>` | ❌ **MISSING** | Repeatable flag to add metadata labels to builds. |

**Impact:** Users cannot tag images with custom names, push to registries, or export builds in alternative formats. This prevents prebuild workflows and CI/CD integration.

### 1.2 Missing Validation Rules

| Validation | Status | Implementation Notes |
|------------|--------|---------------------|
| `--config` filename must be `devcontainer.json` or `.devcontainer.json` | ⚠️ **PARTIAL** | ConfigLoader may validate, but spec requires explicit error message format. |
| `--output` cannot be combined with `--push` | ❌ **NOT IMPLEMENTED** | No validation present since neither flag exists. |
| `--platform`, `--push`, `--output`, `--cache-to` require BuildKit | ⚠️ **PARTIAL** | BuildKit detection exists but no explicit error for these flags when BuildKit unavailable. |
| Compose configs cannot use `--platform`, `--push`, `--output`, `--cache-to` | ❌ **NOT IMPLEMENTED** | Compose rejection exists but not specific to these flags. |

**Spec Requirement:** 
```
IF input.output AND input.push == true THEN
    RAISE InputError("--push true cannot be used with --output.")
```

**Current State:** Neither flag exists, so validation is impossible.

### 1.3 Hidden/Experimental Flags

| Flag | Status | Notes |
|------|--------|-------|
| `--skip-feature-auto-mapping` | ❌ **MISSING** | Hidden testing toggle for feature auto-mapping. |
| `--skip-persisting-customizations-from-features` | ❌ **MISSING** | Do not persist customizations from Features into image metadata. |
| `--experimental-lockfile` | ❌ **MISSING** | Write feature lockfile. |
| `--experimental-frozen-lockfile` | ❌ **MISSING** | Fail if lockfile changes would occur. |
| `--omit-syntax-directive` | ❌ **MISSING** | Omit Dockerfile `# syntax=` directive workaround. |

**Impact:** Cannot match reference implementation behavior or support advanced feature workflows.

---

## 2. Configuration Resolution Gaps

### 2.1 Compose Configuration Support

**Spec Requirement:**
```
ELSE IF compose_mode THEN
    ASSERT NOT parsed.platform AND NOT parsed.push AND NOT parsed.output AND NOT parsed.cache_to
    compose_files, env_file, compose_args = resolve_compose_files_and_args(config)
    version_prefix = read_version_prefix(compose_files)
    compose_res = build_and_extend_compose(...)
    original_name = derive_original_service_image(compose_res, config.service)
    IF image_names NOT EMPTY THEN
        tag_all(original_name, image_names)
        final_names = image_names
    ELSE
        final_names = original_name
```

**Current Implementation:**
```rust
// Check if this is a compose-based configuration
if config.uses_compose() {
    return Err(
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: "Docker Compose configurations cannot be built directly. Use 'docker compose build' to build individual services.".to_string(),
        })
        .into(),
    );
}
```

**Gap:** Complete rejection of Compose configs instead of building with constraints as specified.

**Required Actions:**
1. Implement `build_and_extend_compose()` workflow
2. Generate compose override file for features/labels
3. Derive original service image name
4. Apply `--image-name` tags if provided
5. Validate that BuildKit-specific flags are not used

### 2.2 Image Reference Mode

**Spec Requirement:**
```
ELSE // image reference mode (config.image)
    final = extend_image(
        params, config, base_image=config.image,
        additional_image_names=image_names, additional_features=parsed.additional_features,
        can_add_labels=false)
    final_names = image_names OR final.updated_image_name
```

**Current Implementation:**
```rust
} else if config.image.is_some() {
    // If we have an image but no dockerfile, we can't build
    Err(
        DeaconError::Config(deacon_core::errors::ConfigError::Validation {
            message: "Cannot build with 'image' configuration. Use 'dockerFile' for builds."
                .to_string(),
        })
        .into(),
    )
}
```

**Gap:** Image reference mode should **extend** the base image with features, not reject it.

**Required Actions:**
1. Implement `extend_image()` function
2. Pull base image if not present
3. Apply features on top of base image
4. Generate new image with metadata labels
5. Tag with `--image-name` if provided

---

## 3. Build Execution Logic Gaps

### 3.1 Image Tagging

**Spec Requirement:**
- Accept repeatable `--image-name` flag
- Tag built image with all provided names
- Default tag should be derived if `--image-name` is empty

**Current Implementation:**
```rust
// Add deterministic tag with config hash
let tag = format!("deacon-build:{}", &config_hash[..12]);
build_args.push("-t".to_string());
build_args.push(tag.clone());
```

**Gap:** Only uses a single deterministic tag based on config hash. No support for custom user-specified tags.

**Required Actions:**
1. Parse `--image-name` (repeatable) from CLI
2. Add all user-specified tags with `-t` flags
3. Maintain deterministic tag as default if no `--image-name` provided
4. Return all tags in `BuildResult.imageName` output

### 3.2 Metadata Label Injection

**Spec Requirement:**
```rust
// From SPEC.md Section 5:
// Inject devcontainer metadata as image label
// Always label the resulting image with merged devcontainer metadata 
// (and optionally feature customizations) for later discovery.
```

**Current Implementation:**
```rust
// Add label with config hash
let label = format!("org.deacon.configHash={}", config_hash);
build_args.push("--label".to_string());
build_args.push(label);
```

**Gap:** Only adds `org.deacon.configHash` label. Missing:
- Full devcontainer metadata label (JSON)
- Feature customizations metadata
- User-specified labels from `--label` flag

**Required Actions:**
1. Serialize full devcontainer config as JSON label
2. Include feature metadata and customizations
3. Add user-specified labels from `--label` flags
4. Follow devcontainer metadata label schema from spec

### 3.3 BuildKit Output Control

**Spec Requirement:**
```rust
IF params.buildkit_version EXISTS THEN
    APPEND args: ['buildx','build']
    IF params.buildx_push THEN APPEND args: ['--push']
    ELSE IF params.buildx_output THEN APPEND args: ['--output', params.buildx_output]
    ELSE APPEND args: ['--load']
```

**Current Implementation:**
```rust
// Determine if BuildKit should be used
let use_buildkit = should_use_buildkit(args.buildkit.as_ref());
debug!("Using BuildKit: {}", use_buildkit);
```

**Gap:** No handling of `--push` or `--output` flags. No automatic `--load` when neither is specified.

**Required Actions:**
1. Implement `--push` flag handling
2. Implement `--output` flag handling
3. Add `--load` automatically when neither `--push` nor `--output` is specified
4. Detect BuildKit availability via `buildx` command

---

## 4. Output Specification Gaps

### 4.1 Standard Output Format

**Spec Requirement:**
```json
// Success:
{ "outcome": "success", "imageName": string | string[] }

// Error:
{ "outcome": "error", "message": string, "description"?: string }
```

**Current Implementation:**
```rust
match format {
    OutputFormat::Json => {
        let json = serde_json::to_string_pretty(result)?;
        writer.write_line(&json)?;
    }
    OutputFormat::Text => {
        writer.write_line("Build completed successfully!")?;
        writer.write_line(&format!("Image ID: {}", result.image_id))?;
        // ...
    }
}
```

**Gap:** JSON output uses full `BuildResult` structure instead of spec-compliant format with `outcome` and `imageName` fields.

**Required Actions:**
1. Create spec-compliant output structures
2. Ensure stdout contains only the result JSON (logs go to stderr)
3. Use `imageName` (not `image_id`) in output
4. Support both single string and string array for multiple tags
5. Implement error output format with `outcome: "error"`

### 4.2 Error Message Format

**Spec Requirement:**
```
Config not found → exit code 1, description "Dev container config (...) not found."
Invalid --config filename → exit code 1, message: "Filename must be devcontainer.json or .devcontainer.json (...)"
--output with --push → exit code 1, message: "--push true cannot be used with --output."
```

**Current Implementation:**
Error messages exist but may not match exact spec format. Need verification of:
- Exact message text
- Error structure in JSON output
- Exit codes

---

## 5. Docker/Container Runtime Integration Gaps

### 5.1 Push to Registry

**Spec Requirement:**
```
IF params.buildx_push THEN APPEND args: ['--push']
```

**Current Implementation:** Not implemented.

**Required Actions:**
1. Add `--push` CLI flag
2. Validate BuildKit is enabled when `--push` is used
3. Add `--push` to docker buildx build arguments
4. Validate mutually exclusive with `--output`

### 5.2 Build Context Management

**Spec Requirement:**
```rust
FOR (name, path) IN feature_build_info.buildKitContexts DO 
    APPEND args: ['--build-context', name + '=' + path]
FOR opt IN feature_build_info.securityOpts DO 
    APPEND args: ['--security-opt', opt]
```

**Current Implementation:** Not implemented.

**Gap:** No support for BuildKit build contexts or security options needed for advanced feature installations.

**Required Actions:**
1. Extract build contexts from feature installation metadata
2. Add `--build-context` arguments for each context
3. Add `--security-opt` arguments from feature requirements

---

## 6. Feature System Integration Gaps

### 6.1 Feature Installation Workflow

**Spec Requirement:**
- Apply features during build
- Generate feature installation scripts
- Inject feature metadata into image labels
- Support `--skip-persisting-customizations-from-features`
- Support feature lockfiles

**Current Implementation:**
```rust
// Apply feature merging if CLI features are provided
if args.additional_features.is_some() || args.feature_install_order.is_some() {
    let merge_config = FeatureMergeConfig::new(
        args.additional_features.clone(),
        args.prefer_cli_features,
        args.feature_install_order.clone(),
    );
    config.features = FeatureMerger::merge_features(&config.features, &merge_config)?;
    // ...
}
```

**Gap:** Features are merged but not installed during build. No generation of feature installation Dockerfile layers.

**Required Actions:**
1. Generate feature installation Dockerfile content
2. Create temporary build directory with feature scripts
3. Add feature layers to build
4. Inject feature metadata into image labels
5. Support `--skip-persisting-customizations-from-features` flag
6. Implement lockfile generation/validation

---

## 7. Testing Coverage Gaps

### 7.1 Required Test Cases from Spec

| Test Case | Status | Location |
|-----------|--------|----------|
| Labels applied from `--label` flags | ❌ **MISSING** | N/A |
| Local disallowed feature error | ❌ **MISSING** | N/A |
| Mutually exclusive `--push`/`--output` error | ❌ **MISSING** | N/A |
| BuildKit cache and platform build | ❌ **MISSING** | N/A |
| Compose with unsupported flags error | ❌ **MISSING** | N/A |
| Config in subfolder | ⚠️ **PARTIAL** | May exist in integration tests |

**From SPEC.md Section 15:**
```pseudocode
TEST "labels applied":
    GIVEN example config
    WHEN run with --label name=label-test --label type=multiple-labels
    THEN image has both labels

TEST "mutually exclusive push/output":
    WHEN build with --push true and --output type=oci,dest=out.tar
    THEN error about mutual exclusion
```

---

## 8. Missing Data Structures

### 8.1 ParsedInput Structure

**Spec Requirement (from DATA-STRUCTURES.md):**
```pseudocode
STRUCT ParsedInput:
    image_names: string[]       // repeatable --image-name
    push: boolean               // buildx only
    output: string?             // buildx only, mutually exclusive with push
    labels: string[]            // repeatable --label
    skip_feature_auto_mapping: boolean
    skip_persist_customizations: boolean
    experimental_lockfile: boolean
    experimental_frozen_lockfile: boolean
    omit_syntax_directive: boolean
```

**Current Implementation (BuildArgs):**
Missing fields:
- `image_names`
- `push`
- `output`
- `labels`
- `skip_feature_auto_mapping`
- `skip_persist_customizations`
- `experimental_lockfile`
- `experimental_frozen_lockfile`
- `omit_syntax_directive`

### 8.2 Build Result Structure

**Spec Requirement:**
```pseudocode
STRUCT BuildSuccessResult:
    outcome: 'success'
    imageName: string | string[]

STRUCT BuildErrorResult:
    outcome: 'error'
    message: string
    description?: string
```

**Current Implementation:**
```rust
pub struct BuildResult {
    pub image_id: String,
    pub tags: Vec<String>,
    pub build_duration: f64,
    pub metadata: HashMap<String, String>,
    pub config_hash: String,
}
```

**Gap:** Structure doesn't match spec output format.

---

## 9. Priority Matrix

### Critical (Blocking Basic Workflows)

1. **Implement `--image-name` flag** - Users cannot tag images for use
2. **Fix JSON output format** - Must match spec for tool integration
3. **Implement image reference mode** - Cannot build from `"image"` config
4. **Implement Compose mode** - Compose configs completely blocked

### High (Blocking Advanced Workflows)

5. **Implement `--push` flag** - Cannot push to registries
6. **Implement `--output` flag** - Cannot export builds
7. **Implement `--label` flag** - Cannot add custom metadata
8. **Metadata label injection** - Missing devcontainer metadata in images
9. **Feature installation during build** - Features not applied to images

### Medium (Nice to Have)

10. **Hidden/experimental flags** - Testing and advanced features
11. **BuildKit build contexts** - Advanced feature installations
12. **Validation improvements** - Better error messages
13. **Test coverage** - Match spec test suite

### Low (Future Enhancements)

14. **Documentation updates** - Align with implementation
15. **Performance optimizations** - Already has basic caching

---

## 10. Recommendations

### Immediate Actions (Sprint 1)

1. **Add Core Flags:**
   ```rust
   --image-name <name[:tag]>  (repeatable)
   --push                     (boolean, BuildKit only)
   --output <spec>            (string, BuildKit only)
   --label <name=value>       (repeatable)
   ```

2. **Fix Output Format:**
   - Implement spec-compliant JSON output on stdout
   - Move logs to stderr
   - Use `outcome` and `imageName` fields

3. **Add Flag Validation:**
   - `--push` and `--output` mutually exclusive
   - BuildKit-only flags require BuildKit

### Near-Term Actions (Sprint 2-3)

4. **Implement Image Reference Mode:**
   - Extend base images with features
   - Support `config.image` builds

5. **Implement Compose Mode:**
   - Generate compose override
   - Build and tag service images
   - Validate unsupported flags

6. **Metadata Label Injection:**
   - Serialize full devcontainer config
   - Include feature metadata
   - Support user labels from `--label`

### Long-Term Actions (Future Sprints)

7. **Feature Integration:**
   - Generate feature installation layers
   - Apply features during build
   - Support lockfiles

8. **Hidden Flags:**
   - Implement experimental feature flags
   - Add testing toggles

9. **Test Coverage:**
   - Implement spec test cases
   - Add integration tests
   - Validate error messages

---

## 11. Breaking Changes Required

### Output Format Change

**Current:** Full `BuildResult` JSON structure  
**Required:** Spec-compliant `{ outcome, imageName }` structure

**Migration:** This is a breaking change for any tools parsing the JSON output.

### Compose Configuration Handling

**Current:** Hard error rejecting compose configs  
**Required:** Build compose service images with constraints

**Migration:** Workflows using compose will start succeeding (previously failed).

### Error Message Changes

**Current:** Custom error messages  
**Required:** Spec-compliant error message format

**Migration:** Tools parsing error messages may need updates.

---

## 12. Appendix: Specification References

- **Main Spec:** `docs/subcommand-specs/build/SPEC.md`
- **Data Structures:** `docs/subcommand-specs/build/DATA-STRUCTURES.md`
- **Diagrams:** `docs/subcommand-specs/build/DIAGRAMS.md`
- **Overview:** `docs/subcommand-specs/build/README.md`

---

## 13. Implementation Parity Targets (2025-11-15)

### Phase 1: Tagged Build Deliverable (MVP - Priority P1)
**Target:** Enable `deacon build` to apply requested tags, devcontainer metadata, and user labels while emitting spec-compliant success payloads.

**Key Deliverables:**
- ✅ Repeatable `--image-name` flag for custom tagging
- ✅ Repeatable `--label` flag for user-specified metadata labels
- ✅ Devcontainer metadata label injection (JSON-serialized config, features, customizations)
- ✅ Spec-compliant JSON output: `{ "outcome": "success", "imageName": [...] }`
- ✅ Multi-tag support with deterministic fallback tag
- ✅ CLI parsing and validation for new flags

**Validation:**
- Run `deacon build` with multiple `--image-name` and `--label` inputs
- Verify tags exist locally via `docker images`
- Verify metadata label contains merged configuration
- Verify stdout returns spec-compliant success payload

### Phase 2: Registry and Artifact Distribution (Priority P2)
**Target:** Support pushing images to registries or exporting archives with explicit BuildKit gating and structured error handling.

**Key Deliverables:**
- ✅ `--push` flag for registry push (BuildKit-only)
- ✅ `--output` flag for custom export specs (BuildKit-only)
- ✅ Mutual exclusivity validation (`--push` and `--output` cannot be combined)
- ✅ BuildKit requirement checks with fail-fast errors
- ✅ Success payload includes `pushed` and `exportPath` fields
- ✅ Contract-compliant error messages for validation failures

**Validation:**
- Execute builds with `--push` on BuildKit-enabled hosts
- Execute builds with `--output` for archive exports
- Confirm gating errors when BuildKit unavailable
- Verify artifacts are published/exported as expected

### Phase 3: Multi-source Configuration Coverage (Priority P3)
**Target:** Enable builds for Compose and image-reference configurations with parity validation and feature application.

**Key Deliverables:**
- ✅ Compose service targeting (build only the configured service)
- ✅ Image-reference mode (extend base image with features)
- ✅ Compose override generation for features/labels
- ✅ Unsupported flag preflight validation for Compose mode
- ✅ Feature application and tagging across all modes
- ✅ Supporting fixtures for Compose and image-reference tests

**Validation:**
- Build Compose workspace targeting configured service
- Build image-reference workspace with features applied
- Ensure features, labels, and tagging match Dockerfile mode
- Verify unsupported flags are rejected in Compose mode

### Cross-Cutting Concerns
- ✅ BuildKit capability detection helpers (`crates/core/src/build/buildkit.rs`)
- ✅ Shared label and image-tag validation helpers (`crates/core/src/docker.rs`)
- ✅ Feature metadata serialization (`crates/core/src/build/metadata.rs`)
- ✅ Domain models: `BuildRequest`, `ImageArtifact`, `FeatureManifest`, `ValidationEvent`
- ✅ Result structs: `BuildSuccess`, `BuildError` aligned with contracts

### Testing Strategy
- Integration tests for CLI flag parsing and validation
- JSON output purity checks for spec-compliant payloads
- BuildKit gating and mutual exclusivity error coverage
- Compose service targeting and image-reference build acceptance
- Push/export workflow validation with contract compliance
- Regression coverage for BuildKit-only feature contexts

---

## 14. Version History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2025-10-13 | 1.0 | AI Analysis | Initial gap analysis |
| 2025-11-15 | 1.1 | Implementation | Added parity targets for build subcommand closure (Phases 1-3) |

