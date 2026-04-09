---
work-unit: feature-install-timing-impl
flight-plan: consumer-core-completion
sequence: 6
depends-on:
- feature-install-timing-research
parallel-group: beta
---

## Task

Move feature installation from running container to image build phase via generated Dockerfile with deterministic layers, pass feature options as ENV vars, skip feature build step when no features configured, ensure Docker cache works on unchanged configs

## Acceptance Criteria

- Features installed during image build phase (BuildKit), not in running container [SC-013]
- Feature options passed correctly as ENV vars in generated Dockerfile [SC-014]
- Configs without features skip the feature build step entirely [SC-015]
- Rebuilding without config changes uses Docker cache [SC-016]
- Orphaned in-container feature installer code cleaned up or removed [SC-013]
- All changes pass cargo clippy and cargo test [SC-019]
- New integration tests configured in nextest.toml [SC-020]

## File Scope

### Create


### Modify

- crates/deacon/src/commands/up/features_build.rs
- crates/deacon/src/commands/up/container.rs
- crates/deacon/src/commands/up/mod.rs
- crates/core/src/feature_installer.rs

### Protect


## Procedure

### Step 0: Read research findings
- MUST Read .maverick/context/feature-install-timing-research.md from research bead

### Step 1: Verify current build-phase installation
- MUST Read crates/deacon/src/commands/up/features_build.rs to confirm BuildKit-based feature building
- MUST Read crates/deacon/src/commands/up/container.rs lines 180-230
- IF already in build phase, proceed to cleanup and testing

### Step 2: Clean up orphaned in-container installer
- IF feature_installer.rs confirmed unused: SHOULD remove or deprecate FeatureInstaller
- MUST NOT break compilation. Check all imports first
- IF lib.rs exports feature_installer, SHOULD remove if no downstream uses
- IF integration tests reference it, MUST remove ignored tests or update them

### Step 3: Verify feature options as ENV vars
- MUST verify each feature options emitted as ENV KEY=VALUE before RUN install.sh
- IF not properly passed, MUST fix Dockerfile generation

### Step 4: Verify deterministic layers for cache
- MUST verify same config produces same Dockerfile
- MUST verify features ordered by dependency resolution not random
- IF non-deterministic, MUST switch to deterministic ordering

### Step 5: Verify no-features skip path
- MUST confirm container.rs skips feature build when no features
- MUST add test if none exists

### Step 6: Add or improve tests
- MUST add tests: Dockerfile correctness, ENV vars, empty features skip, deterministic ordering
- MUST configure new integration tests in .config/nextest.toml

### Step 7: Build and lint
- MUST run cargo fmt --all and cargo clippy --all-targets -- -D warnings
- MUST run make test-nextest-fast

## Test Specification

#[test]
fn test_no_features_skips_build() {
    let config = DevContainerConfig {
        image: Some("ubuntu:22.04".to_string()),
        features: serde_json::Value::Null,
        ..Default::default()
    };
    let has_features = config.features.as_object().map(|o| !o.is_empty()).unwrap_or(false);
    assert!(!has_features);
}

#[test]
fn test_empty_features_skips_build() {
    let config = DevContainerConfig {
        image: Some("ubuntu:22.04".to_string()),
        features: serde_json::json!({}),
        ..Default::default()
    };
    let has_features = config.features.as_object().map(|o| !o.is_empty()).unwrap_or(false);
    assert!(!has_features);
}

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
- cargo test -p deacon -- features 2>&1 | tail -3
- cargo test -p deacon-core -- feature 2>&1 | tail -3
- make test-nextest-fast 2>&1 | tail -5
