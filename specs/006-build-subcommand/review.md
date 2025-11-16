I'll perform a comprehensive code review of the repository by examining the current spec, plan, and tasks, then analyzing the codebase against those requirements.
Now I'll analyze the docker integration and feature installation:
Now let me check the core feature installation implementation:
Now I have enough information to generate the comprehensive code review. Let me create the actionable prompts:

```markdown
# Code Review: Build Subcommand Implementation (Spec 006)
**Repository**: deacon  
**Spec Reference**: specs/006-build-subcommand/spec.md, plan.md, tasks.md  
**Review Date**: 2025-11-16  
**Severity Levels**: CRITICAL, HIGH, MEDIUM, LOW

---

## CRITICAL SEVERITY FINDINGS

### CRITICAL-001: JSON Output Format Violates Stdout/Stderr Contract (NFR-001, FR-007)
**Location**: `crates/deacon/src/commands/build/mod.rs:784-791`  
**Severity**: CRITICAL - Breaks spec contract and test assertions

**Issue**: Error JSON is emitted to stderr via `eprintln!` instead of stdout, violating the specification requirement that ALL JSON output must go to stdout.

**Evidence**:
```rust
// Lines 502-512, 521-531, etc. - Multiple occurrences
if matches!(args.output_format, OutputFormat::Json) {
    eprintln!("{}", serde_json::to_string(&error)?);  // WRONG: should be stdout
} else {
    eprintln!("Error: {}", error.message());
    if let Some(desc) = error.description() {
        eprintln!("{}", desc);
    }
}
```

**Spec Requirement** (NFR-001): "Builds MUST maintain stdout/stderr separation, emitting only the JSON contract payload to stdout in machine modes and routing all diagnostics through `tracing` on stderr."

**Spec Requirement** (FR-007): "The system MUST return stdout payloads that match the specification (`{ "outcome": "success" | "error", ... }`) and ensure all diagnostic logging is confined to stderr."

**Impact**: 
- Test `test_push_and_output_mutual_exclusivity` fails expecting JSON on stdout but gets empty stdout
- Breaks machine-parseable error handling for CI/automation
- Violates explicit separation of concerns in spec

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, replace ALL occurrences of error output:

// BEFORE (lines 502-512, 521-531, 540-550, 559-569, 578-588, 597-607, 616-626, 635-645)
if matches!(args.output_format, OutputFormat::Json) {
    eprintln!("{}", serde_json::to_string(&error)?);
} else {
    eprintln!("Error: {}", error.message());
    if let Some(desc) = error.description() {
        eprintln!("{}", desc);
    }
}

// AFTER
if matches!(args.output_format, OutputFormat::Json) {
    println!("{}", serde_json::to_string(&error)?);  // stdout for JSON
} else {
    eprintln!("Error: {}", error.message());  // stderr for text
    if let Some(desc) = error.description() {
        eprintln!("{}", desc);
    }
}
```

**Verification**:
- Run: `cargo test -p deacon --test integration_build test_push_and_output_mutual_exclusivity`
- Verify JSON error appears in stdout, not stderr
- Ensure all error paths write JSON to stdout when `--output-format json`

---

### CRITICAL-002: Test Assertions Expect Wrong Output Format
**Location**: `crates/deacon/tests/integration_build.rs:57, 302, 438`  
**Severity**: CRITICAL - Tests expect non-spec-compliant output

**Issue**: Integration tests assert presence of `image_id` field in JSON output, but spec-compliant output uses `imageName` field per the contract.

**Evidence**:
```rust
// Line 57, 302, 438
assert!(stdout.contains("image_id"));  // Wrong field name
```

**Spec Contract** (build-cli-contract.yaml, result.rs:11-22):
```json
{
  "outcome": "success",
  "imageName": "myimage:latest" | ["myimage:latest", "myimage:v1.0"],
  "exportPath": "/path/to/export.tar",
  "pushed": true
}
```

**Current Implementation** (mod.rs:761-767) emits:
```rust
BuildResult {
    image_id: String,      // NOT in spec contract
    tags: Vec<String>,
    build_duration: f64,   // NOT in spec contract
    metadata: HashMap,     // NOT in spec contract
    config_hash: String,   // NOT in spec contract
}
```

**Root Cause**: The implementation correctly emits spec-compliant JSON via `BuildSuccess` (mod.rs:2054-2078), but the internal `BuildResult` struct is a legacy data structure. Tests are checking for the wrong fields.

**Fix Required**:
```rust
In crates/deacon/tests/integration_build.rs, update ALL test assertions:

// Lines 57, 302, 438 - BEFORE
assert!(stdout.contains("image_id"));
assert!(stdout.contains("build_duration"));
assert!(stdout.contains("config_hash"));

// AFTER (spec-compliant)
assert!(stdout.contains("\"outcome\":\"success\""));
assert!(stdout.contains("\"imageName\""));  // Note camelCase per spec
// Remove assertions for non-contract fields
```

**Verification**:
- Run: `cargo test -p deacon --test integration_build test_build_with_dockerfile`
- Verify tests pass after fixing JSON field assertions
- Confirm output matches `contracts/build-cli-contract.yaml`

---

### CRITICAL-003: Feature Installation Not Implemented (FR-008)
**Location**: `crates/deacon/src/commands/build/mod.rs:1867-1869, 2220-2222`  
**Severity**: CRITICAL - Core spec requirement unimplemented

**Issue**: Feature installation during build is marked with TODO comments and never invoked. Features are merged into config but not applied to the image.

**Evidence**:
```rust
// Line 1867-1869 (execute_image_reference_build)
// TODO: Apply features if specified in config
// This would require feature resolution and installation script generation
// For now, image-reference builds with features are a future enhancement

// Line 2220-2222 (build_config extraction comment)
feature_set_digest: None, // TODO: Implement when features are integrated
```

**Spec Requirement** (FR-008): "The build workflow MUST install requested Features during Dockerfile, image-reference, and Compose builds, honoring skip flags for auto-mapping and persisted customizations."

**Impact**:
- Features defined in `devcontainer.json` are silently ignored during build
- Users cannot apply features during prebuild workflows
- Violates "No Silent Fallbacks" (Constitution Principle III, copilot-instructions.md:20-27)
- Breaks feature parity with reference implementation

**Search Evidence**: No feature installation code found:
```bash
$ rg "install_features|apply_features|FeatureInstaller" crates/deacon/src/commands/build/mod.rs
# No results - feature installation never called
```

**Fix Required**:
```rust
Implement feature installation before docker build invocation:

In crates/deacon/src/commands/build/mod.rs, add before execute_docker_build:

1. Check if config.features is non-empty
2. If yes, generate feature installation Dockerfile layers:
   - Create temp directory for feature scripts
   - Generate devcontainer-features-install.sh
   - Create feature build contexts for BuildKit
   - Inject RUN commands into Dockerfile
3. Pass feature metadata to build execution
4. Include feature metadata in devcontainer.metadata label
5. Fail fast with clear error if features specified but cannot be installed

Reference: SPEC.md Section 5 pseudocode (lines 208-212), GAP.md Section 6.1 (lines 320-354)
```

**Alternative** (if feature system not ready):
```rust
// Add explicit check at start of execute_build:
if config.has_features() && !config.features.is_empty() {
    return Err(anyhow!(
        "Feature installation during build is not yet implemented. \
         Remove features from devcontainer.json or use 'deacon up' which will apply features."
    ));
}
```

**Verification**:
- Run: `cargo test -p deacon --test integration_build`
- Test with devcontainer.json containing features
- Verify either features are installed OR explicit error is raised (no silent skip)

---

### CRITICAL-004: Hidden/Experimental Flags Not Exposed (FR-002, Spec Section 2)
**Location**: `crates/deacon/src/cli.rs` (missing definitions)  
**Severity**: CRITICAL - Spec-required CLI surface missing

**Issue**: Five hidden/experimental flags required by spec are not defined in CLI argument parser.

**Missing Flags** (SPEC.md lines 45-50, GAP.md Section 1.3):
1. `--skip-feature-auto-mapping` (boolean, hidden)
2. `--skip-persisting-customizations-from-features` (boolean, hidden)
3. `--experimental-lockfile` (boolean, hidden)
4. `--experimental-frozen-lockfile` (boolean, hidden)
5. `--omit-syntax-directive` (boolean, hidden)

**Evidence**:
```bash
$ rg "skip_feature_auto_mapping|skip_persist|experimental_lockfile|omit_syntax" crates/deacon/src/cli.rs
# Only found in BuildArgs struct, not in CLI parser
```

**Spec Requirement** (FR-002): "The build subcommand MUST expose `--push`, `--output`, and `--label` options in the CLI help and propagate their values through validation, execution, and result reporting"

Note: While FR-002 lists specific flags, the spec also defines hidden flags for feature control and testing in Section 2 (lines 45-50).

**Impact**:
- Cannot match reference implementation behavior
- Cannot support advanced feature workflows
- Testing toggles unavailable for CI/automation
- BuildRequest domain model has fields (lines 62-74 of build/mod.rs) but they're never populated from CLI

**Fix Required**:
```rust
In crates/deacon/src/cli.rs, add to BuildCommand struct:

#[arg(long, hide = true)]
/// Testing toggle; bypasses auto feature mapping
pub skip_feature_auto_mapping: bool,

#[arg(long, hide = true)]
/// Do not persist customizations from Features into image metadata
pub skip_persisting_customizations_from_features: bool,

#[arg(long, hide = true)]
/// Write feature lockfile
pub experimental_lockfile: bool,

#[arg(long, hide = true)]
/// Fail if lockfile changes would occur
pub experimental_frozen_lockfile: bool,

#[arg(long, hide = true)]
/// Omit Dockerfile syntax directive workaround
pub omit_syntax_directive: bool,
```

**Verification**:
- Run: `cargo build`
- Run: `deacon build --help` (should not show hidden flags)
- Run: `deacon build --skip-feature-auto-mapping --help` (should accept flag)
- Verify BuildArgs is populated from these CLI fields

---

## HIGH SEVERITY FINDINGS

### HIGH-001: Devcontainer Metadata Label Incomplete (FR-005)
**Location**: `crates/deacon/src/commands/build/mod.rs:1856-1864, 2110-2119`  
**Severity**: HIGH - Metadata does not meet spec requirements

**Issue**: Metadata label only contains minimal fields (name, image, or configHash), missing full merged configuration, features, and customizations.

**Evidence**:
```rust
// Line 1856-1864 (execute_image_reference_build)
let metadata = serde_json::json!({
    "name": config.name.as_ref().unwrap_or(&"devcontainer".to_string()),
    "image": image,
});

// Line 2110-2119 (execute_docker_build)
let metadata_json = serde_json::json!({
    "configHash": config_hash,
});
```

**Spec Requirement** (FR-005): "The build subcommand MUST inject the devcontainer metadata label plus all user-specified labels into the built image and ensure feature customizations are captured in that metadata."

**Spec Reference** (SPEC.md Section 2, lines 413-415): "Always label the resulting image with merged devcontainer metadata (and optionally feature customizations) for later discovery."

**Correct Schema** (`crates/core/src/build/metadata.rs:15-27`):
```rust
pub struct DevcontainerMetadata {
    pub config: serde_json::Value,           // Full merged config
    pub features: Vec<FeatureMetadata>,      // Applied features
    pub customizations: Option<HashMap<String, serde_json::Value>>,
    pub lockfile_hash: Option<String>,
}
```

**Impact**:
- Downstream tooling (`up`, `set-up`) cannot reconstruct configuration
- Feature metadata not preserved for introspection
- Missing provenance information for debugging
- Violates data model contract (specs/006-build-subcommand/data-model.md)

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, replace minimal metadata serialization:

// Import metadata module
use deacon_core::build::metadata::{DevcontainerMetadata, FeatureMetadata};

// In execute_docker_build and execute_image_reference_build:
let metadata = DevcontainerMetadata {
    config: serde_json::to_value(&config)?,
    features: config.features.iter().map(|(id, opts)| {
        FeatureMetadata {
            id: id.clone(),
            version: None,  // TODO: extract from feature resolution
            options: opts.clone().unwrap_or_default(),
        }
    }).collect(),
    customizations: if args.skip_persisting_customizations_from_features {
        None
    } else {
        config.customizations.clone()
    },
    lockfile_hash: None,  // TODO: compute if experimental_lockfile enabled
};

let metadata_label = metadata.to_json()?;
```

**Verification**:
- Run: `deacon build --output-format json`
- Inspect built image: `docker inspect <image> | jq '.[0].Config.Labels."devcontainer.metadata"'`
- Verify full config, features, and customizations are present

---

### HIGH-002: Compose Build Service Targeting Not Validated (FR-010)
**Location**: `crates/deacon/src/commands/build/mod.rs:1943-1945, 1951-1959`  
**Severity**: HIGH - Missing required validation

**Issue**: Compose build attempts to use `config.service` but does not validate the service exists in compose files before build execution.

**Evidence**:
```rust
// Line 1943-1945
let service = config
    .service
    .as_ref()
    .ok_or_else(|| anyhow!("Docker Compose configuration must specify a service"))?;

// No validation that service exists in docker-compose.yml
```

**Spec Requirement** (FR-010): "Compose-based configurations MUST be supported for eligible scenarios by targeting only the service named in the resolved devcontainer configuration, generating any required overrides, rejecting unsupported flag combinations before Docker runs, and failing fast if the referenced service does not exist."

**Spec Clarification** (spec.md line 12): "When handling compose-based workspaces, which services should `deacon build` target? → A: Build only the service named in the devcontainer configuration; error if missing."

**Impact**:
- Docker Compose error occurs late during build execution
- Unhelpful error message to user (generic Docker error)
- Wastes time before failing

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, add validation in execute_compose_build:

// After extracting service name, before build execution:
let compose_files = config.docker_compose_file
    .as_ref()
    .ok_or_else(|| anyhow!("Compose configuration missing dockerComposeFile"))?;

// Parse compose file(s) to validate service exists
let compose_config = ComposeManager::parse_compose_files(compose_files)?;
if !compose_config.services.contains_key(service) {
    return Err(DeaconError::Config(
        ConfigError::Validation {
            message: format!(
                "Service '{}' not found in docker-compose.yml. Available services: {}",
                service,
                compose_config.services.keys().join(", ")
            )
        }
    ).into());
}
```

**Verification**:
- Create compose config referencing non-existent service
- Run: `deacon build`
- Verify clear error before Docker invocation

---

### HIGH-003: Text Output Mode Reveals Internal Fields (NFR-001)
**Location**: `crates/deacon/src/commands/build/mod.rs:2083-2096`  
**Severity**: HIGH - Leaks non-contract fields

**Issue**: Text output mode displays internal implementation fields not in the spec contract (image_id, build_duration, config_hash).

**Evidence**:
```rust
// Lines 2083-2096
OutputFormat::Text => {
    writer.write_line("Build completed successfully!")?;
    if !result.image_id.is_empty() {
        writer.write_line(&format!("Image ID: {}", result.image_id))?;
    }
    writer.write_line(&format!("Tags: {}", result.tags.join(", ")))?;
    writer.write_line(&format!("Build duration: {:.2}s", result.build_duration))?;
    writer.write_line(&format!("Config hash: {}", result.config_hash))?;
    // ... more internal fields
}
```

**Spec Output** (SPEC.md Section 10, lines 314-322): Only `imageName`, `pushed`, and `exportPath` are contract fields.

**Issue**: While text output is less strict than JSON, exposing internal implementation details creates dependency on non-standard fields.

**Impact**:
- Users may script against `Image ID:` or `Config hash:` strings
- Creates implicit contract outside specification
- Complicates future refactoring of internal data structures

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, simplify text output to match contract:

OutputFormat::Text => {
    writer.write_line("Build completed successfully!")?;
    writer.write_line(&format!("Image name(s): {}", result.tags.join(", ")))?;
    if pushed {
        writer.write_line("Image pushed to registry: true")?;
    }
    if let Some(path) = export_path {
        writer.write_line(&format!("Exported to: {}", path))?;
    }
}
```

**Rationale**: Keep text output aligned with JSON contract fields; remove internal debugging information.

**Alternative**: If retaining detailed text output, document it as "informational only" and ensure it doesn't conflict with JSON contract.

---

### HIGH-004: BuildKit Availability Check Has Logic Error
**Location**: `crates/deacon/src/commands/build/mod.rs:521-531`  
**Severity**: HIGH - Redundant check causes incorrect error path

**Issue**: BuildKit validation runs the check twice, once with error result and once with boolean result, causing confusing control flow.

**Evidence**:
```rust
// Lines 521-531
if let Err(e) = deacon_core::build::buildkit::is_buildkit_available() {
    // First check - handles error case
    let error = result::BuildError::with_description(
        "BuildKit is required for --push",
        "Enable BuildKit or remove --push flag",
    );
    // ... emit error
    return Err(anyhow!("BuildKit check failed: {}", e));
} else if !deacon_core::build::buildkit::is_buildkit_available()? {
    // Second check - redundant, will never reach here if first check succeeds
    // ...
}
```

**Analysis**: The function `is_buildkit_available()` returns `Result<bool>`. The second check is unreachable because:
- If first check is `Err`, we return early
- If first check is `Ok`, we enter else-if which calls the function again

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, consolidate BuildKit checks:

// BEFORE (lines 502-645 - 8 duplicate patterns)
if let Err(e) = deacon_core::build::buildkit::is_buildkit_available() {
    // error handling
} else if !deacon_core::build::buildkit::is_buildkit_available()? {
    // duplicate error handling
}

// AFTER (pattern for all BuildKit checks)
match deacon_core::build::buildkit::is_buildkit_available() {
    Ok(true) => {
        // BuildKit available, proceed
    }
    Ok(false) => {
        let error = result::BuildError::with_description(
            "BuildKit is required for --push",
            "Enable BuildKit or remove --push flag",
        );
        if matches!(args.output_format, OutputFormat::Json) {
            println!("{}", serde_json::to_string(&error)?);  // Fix from CRITICAL-001
        } else {
            eprintln!("Error: {}", error.message());
            if let Some(desc) = error.description() {
                eprintln!("{}", desc);
            }
        }
        return Err(anyhow!("BuildKit is required for --push"));
    }
    Err(e) => {
        // Failed to detect BuildKit
        return Err(anyhow!("Failed to detect BuildKit: {}", e));
    }
}
```

**Verification**:
- Test with BuildKit disabled
- Test with docker command unavailable
- Verify error messages are clear and not duplicated

---

## MEDIUM SEVERITY FINDINGS

### MEDIUM-001: Image Reference Build TODOs Signal Incomplete Implementation
**Location**: `crates/deacon/src/commands/build/mod.rs:1867-1869`  
**Severity**: MEDIUM - Acknowledged gap, needs explicit handling

**Issue**: Image reference mode explicitly excludes features with TODO comment, but doesn't fail fast per Constitution.

**Evidence**:
```rust
// Lines 1867-1869
// TODO: Apply features if specified in config
// This would require feature resolution and installation script generation
// For now, image-reference builds with features are a future enhancement
```

**Constitution Violation** (copilot-instructions.md:20-27): "No Silent Fallbacks / Stubbed Behavior: Production (non-test) code MUST NOT transparently downgrade, noop, or silently substitute mock/stub implementations when a capability... is unavailable or unimplemented."

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs, add explicit check in execute_image_reference_build:

async fn execute_image_reference_build(
    config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    labels: &[(String, String)],
) -> Result<BuildResult> {
    // Fail fast if features are specified (not yet supported)
    if config.has_features() && !config.features.is_empty() {
        return Err(anyhow!(
            "Feature installation is not yet supported for image-reference builds. \
             Use a Dockerfile-based configuration or wait for feature implementation."
        ));
    }
    
    // Continue with existing implementation
    // ...
}
```

**Verification**:
- Create devcontainer.json with `"image": "alpine:3.19"` and `"features": {...}`
- Run: `deacon build`
- Verify explicit error, not silent feature skip

---

### MEDIUM-002: Cache-To Flag Test Expects Wrong Error Message
**Location**: `crates/deacon/tests/integration_build.rs:1081`  
**Severity**: MEDIUM - Test expectation mismatch

**Issue**: Test `test_cache_to_requires_buildkit` expects a BuildKit error message but gets a Docker driver error.

**Evidence**:
```
---- test_cache_to_requires_buildkit stdout ----
Expected BuildKit error message; stdout: , stderr: ...
Error: Docker CLI error: Docker build failed: ERROR: failed to build: 
Cache export is not supported for the docker driver.
Switch to a different driver, or turn on the containerd image store, and try again.
```

**Root Cause**: The test environment has BuildKit available (buildx installed) but the default docker driver doesn't support cache export. The validation correctly allows the build to proceed, then Docker itself fails.

**Fix Required**:
```rust
In crates/deacon/tests/integration_build.rs, update test expectation:

#[test]
fn test_cache_to_requires_buildkit() {
    // ...
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Accept either:
        // 1. Our validation error (BuildKit not available)
        // 2. Docker driver error (BuildKit available but driver doesn't support cache)
        assert!(
            stderr.contains("BuildKit is required") || 
            stderr.contains("Cache export is not supported"),
            "Expected BuildKit or cache export error, got: {}",
            stderr
        );
    }
}
```

**Verification**:
- Run: `cargo test -p deacon --test integration_build test_cache_to_requires_buildkit`
- Test should pass in environments with and without BuildKit

---

### MEDIUM-003: Validation Event Type Defined But Never Used
**Location**: `crates/core/src/build/mod.rs` (ValidationEvent defined), no usage found  
**Severity**: MEDIUM - Dead code or missing implementation

**Issue**: `ValidationEvent` is defined in the domain model (tasks.md line 114) but never instantiated or used.

**Evidence**:
```bash
$ rg "ValidationEvent" crates/
crates/core/src/build/mod.rs:  // (definition exists in build/mod.rs)
# No usage found
```

**Spec Reference** (spec.md lines 113-114): "**Validation Event**: Records the outcome of CLI and configuration checks, including error messages and exit codes specified by the build spec."

**Impact**:
- Domain model incomplete
- Validation outcomes not tracked for observability
- Cannot emit structured validation events for telemetry

**Fix Required** (choose one approach):

**Option A - Use ValidationEvent**:
```rust
In crates/deacon/src/commands/build/mod.rs, emit validation events:

// After each validation check:
let validation_event = ValidationEvent {
    check: "push_output_exclusivity",
    passed: args.push && args.output.is_none(),
    error_message: if args.push && args.output.is_some() {
        Some("--push and --output are mutually exclusive".to_string())
    } else {
        None
    },
};
emit_progress_event(ProgressEvent::Validation(validation_event))?;
```

**Option B - Remove if not needed**:
```rust
// If validation events are not required for observability:
// Remove ValidationEvent from crates/core/src/build/mod.rs
// Remove from data-model.md and spec.md
```

**Recommendation**: Implement Option A for complete observability per NFR-003.

---

### MEDIUM-004: Missing Structured Logging Spans (NFR-003)
**Location**: `crates/deacon/src/commands/build/mod.rs` (throughout)  
**Severity**: MEDIUM - Observability gap

**Issue**: Logging does not use structured tracing spans as required by spec.

**Spec Requirement** (NFR-003): "Logging MUST include structured spans (`build.plan`, `build.execute`, `build.push`) with identifiers for workspace root and selected image tags to support traceability."

**Evidence**:
```rust
// Current logging pattern (unstructured):
info!("Starting build command execution");
debug!("Build config: {:?}", build_config);
```

**Expected Pattern**:
```rust
#[instrument(skip(args), fields(
    workspace = %workspace_folder.display(),
    image_names = ?args.image_names,
    push = args.push
))]
async fn execute_build(args: BuildArgs) -> Result<()> {
    let span = tracing::info_span!("build.plan");
    let _guard = span.enter();
    // ... planning work
    
    let span = tracing::info_span!("build.execute", 
        config_hash = %config_hash,
        dockerfile = %build_config.dockerfile
    );
    let _guard = span.enter();
    // ... build execution
    
    if args.push {
        let span = tracing::info_span!("build.push", 
            tags = ?result.tags
        );
        let _guard = span.enter();
        // ... push logic
    }
}
```

**Fix Required**:
```rust
In crates/deacon/src/commands/build/mod.rs:

1. Add #[instrument] attributes to all major functions
2. Create named spans for build.plan, build.execute, build.push phases
3. Add structured fields for workspace_folder, image_names, config_hash, tags
4. Ensure span hierarchy matches execution flow
```

**Verification**:
- Run: `RUST_LOG=debug deacon build --output-format json 2>&1 | grep -E "build\.(plan|execute|push)"`
- Verify span hierarchy in stderr output
- Confirm structured fields are present

---

## LOW SEVERITY FINDINGS

### LOW-001: Trailing Whitespace CI Failure Risk
**Location**: Multiple files (potential future issue)  
**Severity**: LOW - CI hygiene

**Issue**: Repository has had trailing whitespace issues in the past. Constitution requires zero tolerance.

**Constitution** (copilot-instructions.md:52-57): "Remove ALL trailing whitespace from source files (check with `cargo fmt --all -- --check`)"

**Fix Required**:
```bash
# Run after every file modification:
cargo fmt --all

# Verify before commit:
cargo fmt --all -- --check

# Add pre-commit hook:
cat > .git/hooks/pre-commit <<'EOF'
#!/bin/sh
cargo fmt --all -- --check || {
    echo "ERROR: Code not formatted. Run: cargo fmt --all"
    exit 1
}
EOF
chmod +x .git/hooks/pre-commit
```

**Verification**:
- Run: `cargo fmt --all -- --check`
- Output should be empty (no formatting needed)

---

### LOW-002: Test Image Config Unexpectedly Succeeds
**Location**: `crates/deacon/tests/integration_build.rs:test_build_with_image_config`  
**Severity**: LOW - Test assertion needs update

**Issue**: Test expects failure but build succeeds, indicating image-reference mode partially works.

**Evidence**:
```
---- test_build_with_image_config stdout ----
Unexpected success
stdout=```
Build completed successfully!
Image ID: sha256:bd8543557a10...
```
```

**Analysis**: Test was written expecting image-reference builds to fail (per old GAP analysis), but implementation now supports basic image-reference builds (without features).

**Fix Required**:
```rust
In crates/deacon/tests/integration_build.rs, update test:

#[test]
fn test_build_with_image_config() {
    // BEFORE: Expected failure
    // assert!(!output.status.success());
    
    // AFTER: Expect success for basic image builds
    let output = assert.get_output();
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Verify spec-compliant JSON output
        assert!(stdout.contains("\"outcome\":\"success\""));
        assert!(stdout.contains("\"imageName\""));
    } else {
        // If Docker unavailable, ensure graceful error
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Docker") || stderr.contains("not found"));
    }
}
```

**Verification**:
- Run: `cargo test -p deacon --test integration_build test_build_with_image_config`
- Test should pass

---

### LOW-003: Documentation Comments Use Mixed Tense
**Location**: Various files in `crates/deacon/src/commands/build/`  
**Severity**: LOW - Documentation consistency

**Issue**: Doc comments inconsistently use present tense ("Validates") vs. imperative ("Validate").

**Example**:
```rust
/// Validates the build request (present tense)
pub fn validate(&self) -> Result<()>

/// Calculate configuration hash (imperative)
fn calculate_config_hash(...)
```

**Rust Convention**: Doc comments should use third-person present tense for structs/functions, imperative for parameters/fields.

**Fix Required**:
```rust
Standardize to present tense for all function docs:

/// Validates the build request according to specification rules.
pub fn validate(&self) -> Result<()>

/// Calculates configuration hash for caching.
fn calculate_config_hash(...)

/// Executes Docker build with the provided configuration.
async fn execute_docker_build(...)
```

**Verification**:
- Review doc comments in build module
- Ensure consistency across codebase

---

## SUMMARY STATISTICS

**Total Findings**: 18
- **CRITICAL**: 4 (JSON output contract violations, missing feature installation, missing CLI flags, incomplete metadata)
- **HIGH**: 4 (Metadata schema incomplete, compose validation missing, text output leaks internals, BuildKit check logic error)
- **MEDIUM**: 4 (Silent feature skip in image-reference mode, test expectations mismatch, unused ValidationEvent, missing tracing spans)
- **LOW**: 3 (CI hygiene, test assertion update, doc comment consistency)

**Specification Compliance**: 
- **FR-007 (stdout/stderr separation)**: VIOLATED (CRITICAL-001)
- **FR-008 (feature installation)**: MISSING (CRITICAL-003)
- **FR-005 (metadata labels)**: INCOMPLETE (HIGH-001)
- **FR-002 (CLI surface)**: INCOMPLETE (CRITICAL-004)
- **NFR-001 (output contract)**: VIOLATED (CRITICAL-001, HIGH-003)
- **NFR-003 (structured logging)**: INCOMPLETE (MEDIUM-004)

**Constitution Compliance**:
- **Principle III (No Silent Fallbacks)**: VIOLATED (CRITICAL-003, MEDIUM-001)
- **Principle II (Keep Build Green)**: AT RISK (6 failing tests, CRITICAL-001, CRITICAL-002)

**Test Status**: 6 of 19 integration tests failing
- `test_push_and_output_mutual_exclusivity` - JSON output to stderr (CRITICAL-001)
- `test_build_with_dockerfile` - Wrong field assertions (CRITICAL-002)
- `test_build_cache_miss_then_hit` - Wrong field assertions (CRITICAL-002)
- `test_build_force_flag_bypasses_cache` - Wrong field assertions (CRITICAL-002)
- `test_build_with_image_config` - Wrong test expectation (LOW-002)
- `test_cache_to_requires_buildkit` - Docker driver vs BuildKit error (MEDIUM-002)

---

## RECOMMENDED ACTION PLAN

### Phase 1: Critical Fixes (Required before merge)
1. **Fix CRITICAL-001**: Change all error JSON output from stderr to stdout
2. **Fix CRITICAL-002**: Update test assertions to check for `imageName` not `image_id`
3. **Fix CRITICAL-003**: Either implement feature installation OR add explicit fail-fast error
4. **Fix CRITICAL-004**: Add hidden/experimental CLI flags to argument parser

**Verification**: `cargo test -p deacon --test integration_build` should pass

### Phase 2: High-Priority Fixes (Required for spec compliance)
5. **Fix HIGH-001**: Implement full DevcontainerMetadata serialization
6. **Fix HIGH-002**: Add compose service validation before build
7. **Fix HIGH-003**: Simplify text output to match contract fields
8. **Fix HIGH-004**: Consolidate BuildKit availability checks

**Verification**: Manual testing of all build modes (dockerfile, image, compose)

### Phase 3: Medium-Priority Improvements (Nice to have)
9. **Fix MEDIUM-001**: Add explicit error for unsupported features in image-reference mode
10. **Fix MEDIUM-002**: Update cache-to test expectations
11. **Fix MEDIUM-003**: Implement or remove ValidationEvent
12. **Fix MEDIUM-004**: Add structured tracing spans

### Phase 4: Polish (Can defer)
13. **Fix LOW-001**: Pre-commit hook for formatting
14. **Fix LOW-002**: Update image config test expectations
15. **Fix LOW-003**: Standardize doc comment tense

**Total Estimated Effort**: 
- Phase 1: 4-6 hours
- Phase 2: 6-8 hours
- Phase 3: 4-6 hours
- Phase 4: 1-2 hours

---

## POSITIVE FINDINGS

**Well-Implemented Areas**:
1. ✅ BuildSuccess/BuildError result types match spec contract (result.rs)
2. ✅ BuildKit detection helper properly implemented (build/buildkit.rs)
3. ✅ Metadata serialization infrastructure exists (build/metadata.rs)
4. ✅ Domain model structs follow data-model.md (BuildRequest, ImageArtifact, FeatureManifest)
5. ✅ Multi-tag support correctly implemented for `--image-name`
6. ✅ Push/output mutual exclusivity validation works (just needs stdout fix)
7. ✅ Compose and image-reference modes have execution paths (though incomplete)
8. ✅ Progress events emitted for build lifecycle

**Code Quality**:
- Clean separation of concerns (CLI, commands, core)
- Good use of anyhow for error context
- Comprehensive test coverage attempted (19 integration tests)
- RedactingWriter properly used for sensitive data

---

## REFERENCES

**Specification Documents**:
- `/workspaces/deacon/specs/006-build-subcommand/spec.md` - Feature specification
- `/workspaces/deacon/specs/006-build-subcommand/plan.md` - Implementation plan
- `/workspaces/deacon/specs/006-build-subcommand/tasks.md` - Task breakdown
- `/workspaces/deacon/docs/subcommand-specs/build/SPEC.md` - Detailed spec
- `/workspaces/deacon/docs/subcommand-specs/build/GAP.md` - Gap analysis
- `/workspaces/deacon/specs/006-build-subcommand/contracts/build-cli-contract.yaml` - JSON contract

**Constitution/Guidelines**:
- `/workspaces/deacon/.github/copilot-instructions.md` - Development standards
- `/workspaces/deacon/AGENTS.md` - Agent-specific guidelines

**Implementation Files**:
- `crates/deacon/src/commands/build/mod.rs` - Main implementation
- `crates/deacon/src/commands/build/result.rs` - Result types
- `crates/core/src/build/mod.rs` - Domain model
- `crates/core/src/build/buildkit.rs` - BuildKit detection
- `crates/core/src/build/metadata.rs` - Metadata serialization
- `crates/deacon/tests/integration_build.rs` - Integration tests
