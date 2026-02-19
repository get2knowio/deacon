# Research: Complete Feature Support During Up Command

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28
**Status**: Complete

## Overview

This document consolidates research findings for implementing complete feature support during the `deacon up` command. All NEEDS CLARIFICATION items from the technical context have been resolved.

---

## Decision 1: Security Options Merging Strategy

**Question**: How should security options from multiple features and config be merged?

**Decision**: Use OR logic for booleans, union with deduplication for arrays.

**Rationale**:
- Matches existing config `extends` chain merging pattern in `crates/core/src/config.rs:1171-1174`
- OR logic for `privileged`/`init` ensures any feature requiring elevated permissions gets them
- Union for arrays ensures all capabilities are available without duplicates

**Alternatives Considered**:
1. **Last-writer-wins**: Rejected - would silently disable security options from earlier features
2. **AND logic for booleans**: Rejected - would prevent features from enabling needed permissions
3. **Ordered concatenation without dedup**: Rejected - would pass duplicate capabilities to Docker

**Implementation**:
```rust
fn merge_security_options(
    config: &DevContainerConfig,
    features: &[ResolvedFeature],
) -> MergedSecurityOptions {
    MergedSecurityOptions {
        privileged: features.iter().any(|f| f.metadata.privileged == Some(true))
            || config.privileged == Some(true),
        init: features.iter().any(|f| f.metadata.init == Some(true))
            || config.init == Some(true),
        cap_add: deduplicate_uppercase(
            config.cap_add.iter()
                .chain(features.iter().flat_map(|f| f.metadata.cap_add.iter()))
        ),
        security_opt: deduplicate(
            config.security_opt.iter()
                .chain(features.iter().flat_map(|f| f.metadata.security_opt.iter()))
        ),
    }
}
```

---

## Decision 2: Feature Lifecycle Command Ordering

**Question**: In what order should feature lifecycle commands execute relative to config commands?

**Decision**: Feature commands execute first, in installation order, then config commands.

**Rationale**:
- Features set up prerequisites (environment, tools) that config commands may depend on
- Installation order is already deterministic via `FeatureDependencyResolver`
- Matches reference devcontainer CLI behavior

**Alternatives Considered**:
1. **Config commands first**: Rejected - config commands often depend on feature setup
2. **Interleaved (feature1, config, feature2)**: Rejected - no clear use case, adds complexity
3. **Parallel execution**: Rejected - dependencies between commands not guaranteed safe

**Implementation**:
```rust
fn aggregate_lifecycle_commands(
    phase: LifecyclePhase,
    features: &[ResolvedFeature],  // Already in installation order
    config: &DevContainerConfig,
) -> Vec<LifecycleCommand> {
    let mut commands = Vec::new();

    // Feature commands first, in installation order
    for feature in features {
        if let Some(cmd) = feature.metadata.get_lifecycle_command(phase) {
            if !is_empty_command(&cmd) {
                commands.push(LifecycleCommand::from_feature(&feature.id, cmd));
            }
        }
    }

    // Config command last
    if let Some(cmd) = config.get_lifecycle_command(phase) {
        if !is_empty_command(&cmd) {
            commands.push(LifecycleCommand::from_config(cmd));
        }
    }

    commands
}
```

---

## Decision 3: Feature Reference Type Detection

**Question**: How should different feature reference types (OCI, local, HTTPS) be distinguished?

**Decision**: Prefix-based detection at parse time with explicit enum representation.

**Rationale**:
- Clear, unambiguous: `./` and `../` are local, `https://` is HTTPS, everything else is OCI
- Early validation prevents downstream confusion
- Enum type ensures all code paths handle all reference types

**Alternatives Considered**:
1. **URL parsing for all**: Rejected - `./feature` is not a valid URL
2. **Heuristic detection**: Rejected - ambiguous cases lead to silent wrong behavior
3. **Explicit prefix requirement for OCI**: Rejected - breaks backward compatibility

**Implementation**:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureRefType {
    Oci(OciFeatureRef),           // ghcr.io/devcontainers/features/node:18
    LocalPath(PathBuf),           // ./local-feature, ../shared
    HttpsTarball(Url),            // https://example.com/feature.tgz
}

impl FeatureRefType {
    pub fn parse(reference: &str) -> Result<Self> {
        if reference.starts_with("./") || reference.starts_with("../") {
            Ok(Self::LocalPath(PathBuf::from(reference)))
        } else if reference.starts_with("https://") {
            let url = Url::parse(reference)
                .context("Invalid HTTPS URL")?;
            Ok(Self::HttpsTarball(url))
        } else {
            let oci_ref = parse_oci_reference(reference)?;
            Ok(Self::Oci(oci_ref))
        }
    }
}
```

---

## Decision 4: Local Feature Path Resolution

**Question**: How should local feature paths be resolved?

**Decision**: Resolve relative to the directory containing devcontainer.json.

**Rationale**:
- Matches user mental model - paths written relative to where config is
- Consistent with how other relative paths in devcontainer.json work
- Allows features outside `.devcontainer/` directory

**Alternatives Considered**:
1. **Relative to workspace root**: Rejected - inconsistent with other config paths
2. **Relative to current working directory**: Rejected - non-deterministic based on where command runs
3. **Only allow paths within .devcontainer**: Rejected - unnecessarily restrictive

**Implementation**:
```rust
fn resolve_local_feature(
    reference: &str,
    config_dir: &Path,  // Directory containing devcontainer.json
) -> Result<PathBuf> {
    let relative_path = Path::new(reference);
    let absolute_path = config_dir.join(relative_path).canonicalize()
        .with_context(|| format!("Local feature not found: {}", reference))?;

    // Verify devcontainer-feature.json exists
    let metadata_path = absolute_path.join("devcontainer-feature.json");
    if !metadata_path.exists() {
        bail!("Missing devcontainer-feature.json in local feature: {}", reference);
    }

    Ok(absolute_path)
}
```

---

## Decision 5: HTTPS Feature Download Strategy

**Question**: How should HTTPS tarball downloads be handled for reliability?

**Decision**: 30-second timeout with single retry on transient errors.

**Rationale**:
- 30 seconds sufficient for reasonably-sized feature tarballs (<10MB typical)
- Single retry handles transient network issues without excessive delays
- Matches spec requirements (FR-023)

**Alternatives Considered**:
1. **No timeout**: Rejected - could hang indefinitely on network issues
2. **Exponential backoff with multiple retries**: Rejected - overkill for feature downloads
3. **Configurable timeout**: Rejected - adds complexity, 30s is reasonable default

**Transient Error Detection**:
- Connection timeout
- HTTP 5xx responses
- Network unreachable errors

**Non-Transient Errors** (no retry):
- HTTP 4xx responses (404, 401, etc.)
- Invalid URL
- TLS certificate errors

**Implementation**:
```rust
const HTTPS_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 1;

async fn download_https_feature(url: &Url) -> Result<TempDir> {
    let client = reqwest::Client::builder()
        .timeout(HTTPS_TIMEOUT)
        .build()?;

    let mut last_error = None;
    for attempt in 0..=MAX_RETRIES {
        match client.get(url.as_str()).send().await {
            Ok(response) if response.status().is_success() => {
                return extract_tarball(response).await;
            }
            Ok(response) if response.status().is_server_error() && attempt < MAX_RETRIES => {
                last_error = Some(anyhow!("Server error: {}", response.status()));
                continue;
            }
            Ok(response) => {
                bail!("HTTPS feature download failed: {}", response.status());
            }
            Err(e) if is_transient_error(&e) && attempt < MAX_RETRIES => {
                last_error = Some(e.into());
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Download failed")))
}
```

---

## Decision 6: Feature Mount Merging Precedence

**Question**: How should feature mounts be merged with config mounts?

**Decision**: Config mounts take precedence for same target path.

**Rationale**:
- User's explicit config should override feature defaults
- Prevents features from overwriting user-specified mount points
- Matches spec requirement (FR-009)

**Alternatives Considered**:
1. **Feature mounts take precedence**: Rejected - user loses control
2. **Error on conflict**: Rejected - too restrictive, common pattern is to override defaults
3. **Append all (allow duplicates)**: Rejected - Docker behavior undefined with duplicate targets

**Implementation**:
```rust
fn merge_mounts(
    config_mounts: &[String],
    feature_mounts: &[ResolvedFeature],
) -> Result<Vec<String>> {
    let mut target_to_mount: IndexMap<String, String> = IndexMap::new();

    // Feature mounts first (will be overwritten by config)
    for feature in feature_mounts {
        for mount_str in &feature.metadata.mounts {
            let parsed = MountParser::parse_mount(mount_str)
                .with_context(|| format!("Invalid mount in feature {}: {}", feature.id, mount_str))?;
            target_to_mount.insert(parsed.target.clone(), mount_str.clone());
        }
    }

    // Config mounts override (last write wins for same target)
    for mount_str in config_mounts {
        let parsed = MountParser::parse_mount(mount_str)?;
        target_to_mount.insert(parsed.target.clone(), mount_str.clone());
    }

    Ok(target_to_mount.into_values().collect())
}
```

---

## Decision 7: Feature Entrypoint Chaining

**Question**: How should multiple feature entrypoints be chained?

**Decision**: Generate wrapper script that chains entrypoints in installation order.

**Rationale**:
- Docker only supports single entrypoint
- Wrapper script allows sequential execution of all entrypoints
- Installation order ensures dependencies are initialized first

**Alternatives Considered**:
1. **Only use last feature's entrypoint**: Rejected - loses earlier feature initialization
2. **Modify each feature's entrypoint in-place**: Rejected - complex, modifies feature content
3. **Use Docker init system**: Rejected - requires additional dependencies

**Implementation**:
```rust
fn generate_entrypoint_wrapper(
    features: &[ResolvedFeature],
    config_entrypoint: Option<&str>,
) -> Option<String> {
    let entrypoints: Vec<&str> = features
        .iter()
        .filter_map(|f| f.metadata.entrypoint.as_deref())
        .chain(config_entrypoint)
        .collect();

    if entrypoints.is_empty() {
        return None;
    }

    if entrypoints.len() == 1 {
        return Some(entrypoints[0].to_string());
    }

    // Generate wrapper script
    let wrapper_content = format!(
        "#!/bin/sh\n{}\nexec \"$@\"\n",
        entrypoints.iter()
            .map(|e| format!("{} || exit $?", e))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Write to container data folder and return path
    Some(write_entrypoint_wrapper(&wrapper_content))
}
```

---

## Decision 8: Lifecycle Command Failure Handling

**Question**: How should lifecycle command failures be handled?

**Decision**: Fail-fast with exit code 1, skip all remaining lifecycle commands.

**Rationale**:
- Matches spec requirement (FR-022)
- Prevents cascading failures from running on broken state
- Clear signal to user that setup failed

**Alternatives Considered**:
1. **Continue on error**: Rejected - could leave container in inconsistent state
2. **Configurable per-command**: Rejected - adds complexity, spec mandates fail-fast
3. **Rollback on failure**: Rejected - complex cleanup, not always possible

**Implementation**:
```rust
async fn execute_lifecycle_phase(
    phase: LifecyclePhase,
    commands: &[LifecycleCommand],
    executor: &ContainerExecutor,
) -> Result<()> {
    for cmd in commands {
        let result = executor.run_command(&cmd.command).await
            .with_context(|| format!(
                "Lifecycle command failed ({}): {}",
                cmd.source, // "feature:node" or "config"
                cmd.command
            ))?;

        if !result.success() {
            // Fail-fast: return error immediately
            bail!(
                "Lifecycle command exited with code {} ({})",
                result.exit_code,
                cmd.source
            );
        }
    }
    Ok(())
}
```

---

## Decision 9: Existing Infrastructure Reuse

**Question**: Which existing infrastructure should be reused vs. implemented fresh?

**Decision**: Reuse all applicable existing infrastructure.

**Components to Reuse**:
| Component | Location | Usage |
|-----------|----------|-------|
| `MountParser` | `crates/core/src/mount.rs` | Parse feature mount strings/objects |
| `ConfigLoader` | `crates/core/src/config.rs` | Load devcontainer.json with extends |
| `ContainerLifecycle` | `crates/core/src/container_lifecycle.rs` | Execute lifecycle commands |
| `FeatureDependencyResolver` | `crates/core/src/features.rs` | Determine installation order |
| `OciClient` | `crates/core/src/oci/client.rs` | Download OCI features |
| `ReqwestClient` | `crates/core/src/http.rs` | HTTP operations for HTTPS features |

**Rationale**:
- Avoids code duplication
- Leverages tested implementations
- Maintains consistency with rest of codebase
- Follows Constitution Principle VII (Shared Abstractions)

---

## Decision 10: Test Strategy

**Question**: How should the new functionality be tested?

**Decision**: Layered testing with unit tests for logic, integration tests for Docker operations.

**Test Categories**:

| Category | Test Group | Tests |
|----------|------------|-------|
| Security option merging | unit | Pure logic, no Docker |
| Feature reference parsing | unit | Pure logic, no Docker |
| Mount merging | unit | Pure logic, no Docker |
| Lifecycle ordering | unit | Pure logic, mock executor |
| Local feature loading | fs-heavy | File system operations |
| HTTPS feature download | unit (mocked) | Mock HTTP responses |
| Full up with features | docker-shared | Real Docker, shared daemon |
| Security options applied | docker-shared | Verify container flags |

**Rationale**:
- Unit tests for fast feedback on logic
- Integration tests for Docker interaction confidence
- `docker-shared` group allows parallel test execution
- Mock HTTP prevents flaky tests from real network

---

## Implementation Order

Based on dependencies and risk:

1. **Feature Reference Types** (Decision 3) - Foundation for all feature sources
2. **Local Feature Support** (Decision 4) - Lower risk than HTTPS, validates reference system
3. **Security Options Merging** (Decision 1) - High value, common need
4. **Lifecycle Command Ordering** (Decision 2, 8) - Depends on resolved features
5. **Mount Merging** (Decision 6) - Moderate complexity
6. **Entrypoint Chaining** (Decision 7) - Complex, lower priority
7. **HTTPS Feature Support** (Decision 5) - Network-dependent, P3 priority

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Security option conflicts | Medium | High | Extensive testing, clear merge rules |
| Entrypoint wrapper complexity | Medium | Medium | Defer to P2, test thoroughly |
| HTTPS download failures | Medium | Low | Retry logic, clear error messages |
| Local path edge cases | Low | Medium | Canonicalize paths, validate early |
| Lifecycle command timeouts | Low | High | Use existing timeout infrastructure |

---

## References

- containers.dev Feature Specification: https://containers.dev/implementors/features/
- Existing feature installer: `crates/core/src/feature_installer.rs`
- Config merging logic: `crates/core/src/config.rs:1171-1174`
- Mount parser: `crates/core/src/mount.rs`
