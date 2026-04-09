# Feature Installation Timing Research

## Current State

### Active Code Path: BuildKit-based Image Build (`features_build.rs`)

Features are installed **during image build phase** (before container creation), not in a running container. The active code path is:

1. **`crates/deacon/src/commands/up/container.rs:193-222`** — Entry point. Checks if `config.features` is a non-empty object. If features exist, calls `build_image_with_features()`.
2. **`crates/deacon/src/commands/up/features_build.rs:48-294`** — `build_image_with_features()`:
   - Parses feature references from config (`features_build.rs:70-126`)
   - Downloads features from OCI registries via `default_fetcher()` (`features_build.rs:129-134`)
   - Creates `ResolvedFeature` entries with user options + metadata defaults (`features_build.rs:137-167`)
   - Resolves dependency ordering via `FeatureDependencyResolver` (`features_build.rs:170-178`)
   - Collects `combined_env` from feature metadata in plan order (`features_build.rs:181-188`)
   - Copies feature directories to a temp BuildKit context (`features_build.rs:200-229`)
   - Generates a Dockerfile via `DockerfileGenerator` (`features_build.rs:232-244`)
   - Builds image with `docker buildx build` via `CliDocker::build_image()` (`features_build.rs:279-285`)
   - Returns `FeatureBuildOutput { image_tag, combined_env, resolved_features }` (`features_build.rs:289-293`)
3. **`crates/core/src/dockerfile_generator.rs:53-95`** — Generates Dockerfile:
   - `ARG _DEV_CONTAINERS_BASE_IMAGE=<base>` + `FROM` stage (`dockerfile_generator.rs:63-72`)
   - Per-feature: `RUN --mount=type=bind,from=dev_containers_feature_content_source,...` with ENV vars before `./install.sh` (`dockerfile_generator.rs:78-91`)

After `build_image_with_features()` returns, `container.rs:207-217` merges `combined_env` into `config.container_env` and updates `config.image` to the feature-extended image tag. The container is then created from this already-extended image.

### Orphaned Code Path: In-container Installer (`feature_installer.rs`)

**`crates/core/src/feature_installer.rs`** contains `FeatureInstaller` — an older in-container installation approach that:
- Copies feature files into a running container via `docker exec` + base64-encoded echo (`feature_installer.rs:330-493`)
- Executes `install.sh` inside the container via `docker exec` (`feature_installer.rs:497-566`)
- Applies environment variables by writing `/etc/profile.d/deacon-features.sh` (`feature_installer.rs:569-636`)
- Supports parallel installation within dependency levels via tokio semaphore (`feature_installer.rs:155-272`)

**Usage analysis:**
- `crates/core/src/lib.rs:20` — `pub mod feature_installer;` (publicly exported)
- `crates/core/tests/integration_feature_installation.rs:3` — Only consumer, imports `FeatureInstallationConfig` for tests
- **Zero imports from the CLI binary (`crates/deacon/`)** — `features_build.rs` is the sole active path
- The integration test file has `#[ignore]` on the Docker-dependent test, plus basic struct construction tests

**Verdict: `feature_installer.rs` is orphaned.** It is not called from any production code path. The BuildKit approach in `features_build.rs` completely supersedes it.

## Feature Options ENV Var Handling

### BuildKit Path (Active)

Feature options are passed as environment variables in the generated Dockerfile:

1. **`dockerfile_generator.rs:145-156`** — `build_environment_variables()` iterates feature options, converts keys to UPPERCASE
2. **`dockerfile_generator.rs:159-167`** — `option_value_to_string()` serializes each `OptionValue` variant:
   - `Boolean(b)` → `"true"/"false"`
   - `String(s)` → literal string
   - `Number(n)` → `n.to_string()`
   - `Array(a)` / `Object(o)` → JSON serialization
   - `Null` → empty string
3. **`dockerfile_generator.rs:171-175`** — `format_env_var()` escapes backslashes and quotes
4. Generated Dockerfile line format: `KEY="value" \` before `./install.sh`

**Default option filling:** `features_build.rs:150-159` fills in default values from `downloaded.metadata.options` when the user did not supply a value. This ensures install scripts always receive all expected option ENV vars.

### In-container Path (Orphaned)

`feature_installer.rs:522-533` does a similar UPPERCASE conversion but passes options via `docker exec --env` rather than Dockerfile ENV. Also sets `FEATURE_ID`, `FEATURE_VERSION`, `PROVIDED_OPTIONS` (JSON), `DEACON=1`, and `FEATURE_PATH`.

**Gap in BuildKit path:** The BuildKit-generated Dockerfile does NOT set `FEATURE_ID`, `FEATURE_VERSION`, `PROVIDED_OPTIONS`, `DEACON`, or `FEATURE_PATH` env vars. These are set only in the orphaned in-container path. Some install scripts may rely on these standard vars (particularly `FEATURE_ID` and `VERSION`).

## Cache Behavior

### BuildKit Caching

- **`features_build.rs:260-275`** — Logs cache configuration (`cache_from`, `cache_to`) before build
- **`dockerfile_generator.rs:188-226`** — `generate_build_args()` passes `--cache-from`, `--cache-to`, `--builder` from `BuildOptions` to `docker buildx build`
- **`features_build.rs:249`** — Image tag: `deacon-devcontainer-features:{workspace_hash}` — deterministic per workspace

### Determinism Analysis

- **Dependency ordering:** `FeatureDependencyResolver::resolve()` produces deterministic level-based ordering (`features_build.rs:170-172`)
- **Feature directory names:** `{sanitized_id}_{level_idx}` — deterministic (`features_build.rs:225`)
- **Potential non-determinism:** `features_build.rs:90` iterates `features_obj.iter()` — `serde_json::Map` iteration order depends on JSON object key ordering, which is insertion-order-preserving in `serde_json`. As long as the JSON config file doesn't change, order is deterministic.
- **ENV var ordering in Dockerfile:** `HashMap` iteration in `build_environment_variables()` (`dockerfile_generator.rs:145`) is non-deterministic. Different runs may produce ENV vars in different order within the same RUN command. **This breaks Docker layer caching** — the Dockerfile content hash changes even when nothing changed.

**Gap: Non-deterministic ENV var ordering in generated Dockerfile prevents reliable Docker cache hits.**

## No-Features Skip Path

**`container.rs:193-222`** correctly skips feature building:

```rust
let resolved_features = if config
    .features
    .as_object()
    .map(|o| !o.is_empty())
    .unwrap_or(false)
{
    // ... build features ...
    Some(feature_build.resolved_features)
} else {
    None
};
```

This handles:
- `features: null` → `as_object()` returns `None` → `unwrap_or(false)` → skip ✓
- `features: {}` → `as_object()` returns `Some({})` → `is_empty()` → `false` → skip ✓
- Features not present in config → depends on default value (typically `Value::Null`) → skip ✓

Additionally, `features_build.rs:75-81` has an early return for empty features object, providing a second safety net.

**Verdict: No-features skip path is correct and has defense-in-depth.**

## Gaps Found

### G1: Orphaned `feature_installer.rs` (Medium Priority)
- **Issue:** Dead code in `crates/core/src/feature_installer.rs` — publicly exported, never called from production code
- **Impact:** Maintenance burden, confusion about which path is active, false positives in code searches
- **Fix:** Remove `feature_installer.rs`, remove `pub mod feature_installer;` from `lib.rs`, remove/update `integration_feature_installation.rs` test file

### G2: Missing Standard ENV Vars in BuildKit Path (Low-Medium Priority)
- **Issue:** BuildKit Dockerfile does not set `FEATURE_ID`, `FEATURE_VERSION`, `PROVIDED_OPTIONS`, `DEACON=1`, `FEATURE_PATH` — vars that install scripts may expect
- **Impact:** Some feature install scripts that reference these standard vars may fail silently or behave differently
- **Fix:** Add these standard ENV vars to `generate_feature_install_command()` in `dockerfile_generator.rs`

### G3: Non-deterministic ENV Var Ordering (Low Priority)
- **Issue:** `HashMap` iteration in `build_environment_variables()` produces non-deterministic ordering
- **Impact:** Docker layer cache misses when Dockerfile content hash changes between identical runs
- **Fix:** Use `BTreeMap` or sort keys before generating ENV var lines

## Recommended Changes

### For Implementation Bead (006-feature-install-timing-impl)

1. **Remove orphaned code (G1):**
   - Delete `crates/core/src/feature_installer.rs`
   - Remove `pub mod feature_installer;` from `crates/core/src/lib.rs`
   - Delete or update `crates/core/tests/integration_feature_installation.rs`
   - Verify no downstream crates reference it (confirmed: only test file)

2. **Add standard ENV vars to BuildKit Dockerfile (G2):**
   - In `dockerfile_generator.rs::generate_feature_install_command()`, add before option ENV vars:
     - `_CONTAINER_ID_=<feature_id>` (maps to `FEATURE_ID` in install scripts)
     - `VERSION=<feature_version>` if available
     - `_BUILD_ARG_DEACON=1`
   - Follow reference CLI patterns for exact var names

3. **Fix ENV var ordering for deterministic caching (G3):**
   - Change `build_environment_variables()` return type from `HashMap` to `BTreeMap` (sorted keys)
   - Or collect into `Vec` and sort before emitting

4. **Add tests:**
   - Test no-features skip path (both `null` and `{}`)
   - Test deterministic Dockerfile generation (same input → same output)
   - Test standard ENV vars presence in generated Dockerfile
   - Configure new tests in `.config/nextest.toml` if integration-level
