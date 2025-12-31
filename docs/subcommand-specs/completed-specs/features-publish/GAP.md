# Features Publish Implementation Gap Analysis

**Date:** October 13, 2025  
**Specification Source:** `/workspaces/deacon/docs/subcommand-specs/features-publish/`  
**Implementation Files:**
- `/workspaces/deacon/crates/deacon/src/cli.rs` (CLI arguments)
- `/workspaces/deacon/crates/deacon/src/commands/features.rs` (execution logic)
- `/workspaces/deacon/crates/core/src/oci.rs` (OCI client)

---

## Executive Summary

The current implementation of `features publish` has significant gaps compared to the official specification. While basic publish functionality exists, **critical features like semantic version tagging, collection metadata publishing, and idempotency checks are completely missing**. The CLI interface also deviates from the spec's expected arguments.

**Status:** ⚠️ **PARTIALLY IMPLEMENTED** - Core functionality exists but missing 60-70% of required features

---

## 1. Command-Line Interface Gaps

### ❌ MISSING: `--namespace` Flag (CRITICAL)

**Specification:**
```
devcontainer features publish [target] --registry <host> --namespace <owner/repo>
```
- `--namespace, -n <owner/repo>`: Collection namespace (required)
- `--registry, -r <host>`: Registry hostname (default `ghcr.io`)

**Current Implementation:**
```rust
Publish {
    path: String,                    // Positional argument
    #[arg(long)]
    registry: String,                // Required flag (registry URL)
    // MISSING: --namespace flag
}
```

**Impact:** HIGH
- Users cannot separate registry host from namespace/repository path
- Current `--registry` flag conflates both concepts into single string
- Breaking change from spec's intended interface

**Fix Required:**
```rust
Publish {
    path: String,
    #[arg(long, short = 'r', default_value = "ghcr.io")]
    registry: String,
    #[arg(long, short = 'n')]
    namespace: String,  // NEW: Required namespace parameter
    // ... rest
}
```

---

### ❌ MISSING: `--log-level` Flag

**Specification:**
- `--log-level <info|debug|trace>`: Logging level (default `info`)

**Current Implementation:**
- Not present; relies on global `RUST_LOG` environment variable

**Impact:** MEDIUM
- Less user-friendly than spec's explicit flag
- Inconsistent with other devcontainer CLI implementations

---

### ✅ IMPLEMENTED: Basic Flags

These flags are correctly implemented:
- `--dry-run`: Dry run mode ✓
- `--json`: JSON output format ✓
- `--username`: Registry authentication ✓ (marked as TODO but parameter exists)
- `--password-stdin`: Password from stdin ✓ (marked as TODO but parameter exists)

---

## 2. Core Execution Logic Gaps

### ❌ MISSING: Semantic Version Tagging (CRITICAL)

**Specification Requirement:**
```
For version `X.Y.Z`, publish tags `[X, X.Y, X.Y.Z, latest]` if not already published
```

**Expected Behavior (from SPEC.md §5):**
```pseudocode
for feature in packaged.features:
    version = feature.version
    published_tags = fetch_published_tags(oci_ref)
    tags_to_publish = compute_semantic_tags(version, published_tags)
    if tags_to_publish:
        digest = push_feature_tarball(oci_ref, out_dir, feature, tags_to_publish)
```

**Current Implementation:**
```rust
// Only publishes single tag from the registry reference
let feature_ref = FeatureRef::new(
    registry_url.clone(),
    namespace.clone(),
    name.clone(),
    tag.clone(),  // Single tag only
);
// ... publish_feature() called once
```

**Impact:** CRITICAL
- Major discoverability issue for consumers
- Cannot use `myfeature:1` to get latest `1.x.x` version
- Non-compliance with container.dev community practices
- Missing consumer ergonomics (npm-style semver ranges)

**What's Missing:**
1. Tag parsing/extraction functionality
2. `fetch_published_tags()` - Query registry for existing tags
3. `compute_semantic_tags()` - Algorithm to derive `[major, major.minor, major.minor.patch, latest]`
4. Loop to publish multiple tags per feature
5. Logic to skip already-published tags (idempotency)

**Required Implementation:**
```rust
// 1. Parse version from metadata
let version = semver::Version::parse(metadata.version)?;

// 2. Fetch existing tags from registry
let existing_tags = oci_client.list_tags(&feature_ref).await?;

// 3. Compute semantic tags to publish
let desired_tags = vec![
    version.major.to_string(),                           // "1"
    format!("{}.{}", version.major, version.minor),      // "1.2"
    format!("{}.{}.{}", version.major, version.minor, version.patch), // "1.2.3"
    "latest".to_string(),
];

// 4. Filter out already-published tags
let tags_to_publish: Vec<String> = desired_tags
    .into_iter()
    .filter(|tag| !existing_tags.contains(tag))
    .collect();

// 5. Publish each new tag
for tag in tags_to_publish {
    let tagged_ref = feature_ref.with_tag(&tag);
    oci_client.publish_feature(&tagged_ref, tar_data.clone(), metadata).await?;
}
```

---

### ❌ MISSING: Collection Metadata Publishing (CRITICAL)

**Specification Requirement (SPEC.md §5):**
```pseudocode
// Publish collection metadata
collection_ref = make_collection_ref(input.registry, input.namespace)
push_collection_metadata(collection_ref, join(out_dir, 'devcontainer-collection.json'))
```

**Current Implementation:**
- No collection metadata handling at all
- No `devcontainer-collection.json` reading/generation
- No collection-level OCI artifact publishing

**Impact:** CRITICAL
- Features are published as isolated artifacts
- Missing collection-level discovery mechanism
- Cannot query all features in a namespace
- Non-compliance with Features Distribution spec

**What's Missing:**
1. Collection metadata file reading (`devcontainer-collection.json`)
2. Collection OCI reference construction
3. Collection artifact upload to registry
4. Collection manifest format implementation

**Example Collection Metadata:**
```json
{
  "sourceInformation": {
    "source": "ghcr.io/owner/repo"
  },
  "features": [
    {
      "id": "feature-a",
      "version": "1.2.3",
      "name": "Feature A"
    },
    {
      "id": "feature-b", 
      "version": "2.0.1",
      "name": "Feature B"
    }
  ]
}
```

---

### ❌ MISSING: Idempotency and Version Existence Checks

**Specification Requirement (SPEC.md §6):**
```
Idempotency: Safe to re-run; existing tags are detected and skipped.
```

**Specification Behavior (SPEC.md §5):**
```pseudocode
published_tags = fetch_published_tags(oci_ref)
tags_to_publish = compute_semantic_tags(version, published_tags)
if tags_to_publish:
    // Only publish if new tags needed
else:
    log_warn('Version already exists; skipping')
```

**Current Implementation:**
```rust
// No existence check - always attempts to publish
let publish_result = fetcher
    .publish_feature(&feature_ref, tar_data.into(), &metadata)
    .await
    .map_err(|e| anyhow::anyhow!("Failed to publish feature: {}", e))?;
```

**Impact:** HIGH
- Re-running publish for same version causes errors instead of graceful skip
- CI/CD pipelines cannot be idempotent
- Wastes time/bandwidth on redundant uploads

**Required:**
```rust
// Check if version already published
if let Some(existing_digest) = oci_client.get_manifest_digest(&feature_ref).await? {
    warn!("Version {} already published (digest: {}), skipping", 
          feature_ref.tag(), existing_digest);
    return Ok(/* skip result */);
}
```

---

### ⚠️ INCOMPLETE: Automatic Packaging Integration

**Specification Requirement (SPEC.md §5):**
```pseudocode
// Ensure artifacts exist
packaged = do_features_package(target=input.target, output_dir=out_dir)
ASSERT packaged.features NOT EMPTY
```

**Current Implementation:**
```rust
// Creates package in temp dir, but doesn't reuse existing package output
let temp_dir = tempfile::tempdir()?;
let (_digest, _size) =
    create_feature_package(feature_path, temp_dir.path(), &metadata.id).await?;
```

**Status:** PARTIAL
- Does create package automatically ✓
- Always creates in temp dir (doesn't check if already packaged) ✓/-
- Could be optimized to reuse existing package artifacts

**Impact:** LOW
- Works but less efficient than spec's intent

---

## 3. OCI Client Gaps

### ❌ MISSING: Tag Listing API

**Required by Spec:**
```pseudocode
published_tags = fetch_published_tags(oci_ref)
```

**Current OCI Client (`crates/core/src/oci.rs`):**
- No `list_tags()` or `get_published_tags()` method
- Cannot query registry for existing tags

**Required Addition:**
```rust
impl OciFetcher {
    pub async fn list_tags(&self, feature_ref: &FeatureRef) -> Result<Vec<String>> {
        // GET /v2/<name>/tags/list
        let url = format!("{}/v2/{}/tags/list", 
                         feature_ref.registry, 
                         feature_ref.repository());
        // ... HTTP request and parse JSON response
    }
}
```

---

### ❌ MISSING: Multi-Tag Publishing

**Current Behavior:**
```rust
pub async fn publish_feature(
    &self,
    feature_ref: &FeatureRef,  // Single tag reference
    tar_data: Bytes,
    metadata: &FeatureMetadata,
) -> Result<PublishResult>
```

**Required:**
- Either modify to accept `Vec<String>` for tags
- Or call multiple times with different tagged refs (current approach could work with loop in caller)

---

### ⚠️ INCOMPLETE: Authentication Implementation

**Current Status:**
```rust
// TODO: Implement credential setting in OCI client
debug!("Username provided for authentication: {}", _username);

// TODO: Implement reading password from stdin  
debug!("Password will be read from stdin");
```

**Specification Requirement (SPEC.md §7):**
```
Supports `DOCKER_CONFIG` based auth or `DEVCONTAINERS_OCI_AUTH` 
(host|user|pass) environment for tests
```

**Impact:** HIGH
- Users cannot authenticate to private registries
- Blocks real-world usage outside public ghcr.io
- Only dry-run mode works currently

---

## 4. Output Format Gaps

### ⚠️ INCOMPLETE: JSON Output Structure

**Specification (DATA-STRUCTURES.md):**
```json
{
  "featureId": "<id>",
  "digest": "sha256:...",
  "publishedTags": ["1", "1.2", "1.2.3", "latest"]
}
```

**Current Implementation:**
```rust
FeaturesResult {
    command: "publish".to_string(),
    status: "success".to_string(),
    digest: Some(publish_result.digest),
    size: Some(publish_result.size),
    message: Some(format!("Successfully published {} to {}", ...)),
    cache_path: None,
}
```

**Missing Fields:**
- ❌ `featureId` - Not included
- ❌ `publishedTags` - Critical missing field (should show all tags published)
- ✅ `digest` - Present
- ⚠️ `size` - Present but not in spec (extra field, harmless)
- ⚠️ `message` - Present but not in spec (extra field, useful)

**Impact:** MEDIUM
- Consumers cannot determine which tags were published
- JSON output not fully machine-parseable for downstream tools

---

### ✅ IMPLEMENTED: Text Mode Logging

**Current:**
```rust
info!("Publishing feature: {} ({})", metadata.id, metadata.name);
info!("Publishing to OCI registry: {}", feature_ref.reference());
info!("Successfully published {} with digest {}", ...);
```

**Status:** GOOD
- Provides reasonable human-readable output
- Missing some detail about semantic tags (because not implemented)

---

## 5. Error Handling Gaps

### ⚠️ PARTIAL: Authentication Errors

**Specification (SPEC.md §9):**
```
Not authenticated: authentication error with hint
```

**Current:**
```rust
.map_err(|e| anyhow::anyhow!("Failed to publish feature: {}", e))?;
```

**Status:** BASIC
- Generic error propagation exists
- Could provide more helpful hints about auth setup

---

### ❌ MISSING: Semantic Version Validation

**Specification (SPEC.md §9):**
```
Invalid semantic version: exit with error
```

**Current:**
- No version validation before attempting publish
- Would fail later during semantic tag computation (if implemented)

**Required:**
```rust
semver::Version::parse(metadata.version.as_deref().unwrap_or(""))
    .map_err(|e| anyhow::anyhow!("Invalid semantic version: {}", e))?;
```

---

## 6. Missing Features Summary Table

| Feature | Spec Requirement | Current Status | Impact | Priority |
|---------|-----------------|----------------|--------|----------|
| `--namespace` flag | Required CLI arg | ❌ Missing | HIGH | P0 |
| Semantic version tagging | Publish `[X, X.Y, X.Y.Z, latest]` | ❌ Missing | CRITICAL | P0 |
| Collection metadata publish | Push `devcontainer-collection.json` | ❌ Missing | CRITICAL | P0 |
| Idempotency checks | Skip if version exists | ❌ Missing | HIGH | P0 |
| Tag listing API | `fetch_published_tags()` | ❌ Missing | CRITICAL | P0 |
| Registry authentication | `--username`/`--password-stdin` impl | ⚠️ Stubbed | HIGH | P0 |
| `--log-level` flag | CLI logging control | ❌ Missing | MEDIUM | P1 |
| `publishedTags` in JSON output | Show all tags published | ❌ Missing | MEDIUM | P1 |
| Semver validation | Validate before publish | ❌ Missing | MEDIUM | P1 |
| Auth error hints | User-friendly messages | ⚠️ Basic | LOW | P2 |

**Legend:**
- ❌ Missing: Not implemented
- ⚠️ Partial: Started but incomplete
- ✅ Implemented: Matches spec

---

## 7. Compliance Checklist

### Critical (Must Fix for Spec Compliance)
- [ ] Add `--namespace` CLI flag (separate from `--registry`)
- [ ] Implement semantic version tagging algorithm
- [ ] Implement OCI tag listing (`list_tags()`)
- [ ] Compute and filter tags to publish
- [ ] Publish multiple tags per feature in loop
- [ ] Add idempotency checks (skip if version exists)
- [ ] Implement collection metadata publishing
- [ ] Complete registry authentication implementation
- [ ] Add `publishedTags` to JSON output

### Important (Should Fix for Best Practices)
- [ ] Add `--log-level` CLI flag
- [ ] Validate semantic version before publish
- [ ] Add `featureId` to JSON output
- [ ] Improve error messages for auth failures
- [ ] Add retry logic for transient network errors (spec §11)

### Nice to Have (Future Improvements)
- [ ] Parallelize multi-feature publishing (spec §11 note)
- [ ] Add `DEVCONTAINERS_OCI_AUTH` env support for tests
- [ ] Optimize to reuse existing packaged artifacts
- [ ] Add progress indicators for multi-tag publishing

---

## 8. Testing Gaps

**Specification Test Cases (SPEC.md §15):**
```pseudocode
TEST "first publish": expect tags X, X.Y, X.Y.Z, latest
TEST "re-publish same version": expect skip warning, no error
TEST "invalid version": expect error and exit 1
TEST "auth via DEVCONTAINERS_OCI_AUTH": use local registry in tests
```

**Current Test Coverage:**
- ✅ Dry-run mode test exists (`test_features_publish_dry_run`)
- ✅ Basic failure test exists (`test_features_publish_without_dry_run`)
- ❌ No test for semantic tag publishing
- ❌ No test for idempotency (re-publish same version)
- ❌ No test for invalid version rejection
- ❌ No test for authentication mechanisms
- ❌ No test for collection metadata publishing

---

## 9. Recommended Implementation Order

### Phase 1: Core Semantic Tagging (Highest Priority)
1. **Add `--namespace` flag** (breaking CLI change, announce clearly)
   - Separate `--registry` (default: `ghcr.io`) from `--namespace` (required)
   - Update argument parsing and tests

2. **Implement OCI tag listing**
   - Add `list_tags()` to `OciFetcher`
   - Support OCI Distribution API `/v2/<name>/tags/list`

3. **Implement semantic tag computation**
   - Add `semver` dependency
   - Parse version and compute `[major, major.minor, full, latest]`
   - Filter against existing tags

4. **Multi-tag publishing loop**
   - Iterate over tags to publish
   - Call `publish_feature()` for each new tag

5. **Idempotency checks**
   - Check manifest existence before upload
   - Skip with warning if already published

### Phase 2: Collection Metadata
6. **Collection metadata support**
   - Read/generate `devcontainer-collection.json`
   - Implement collection OCI artifact format
   - Upload collection manifest to registry

### Phase 3: Authentication & Polish
7. **Complete authentication**
   - Wire username/password to OCI client
   - Implement password-stdin reading
   - Add `DOCKER_CONFIG` and `DEVCONTAINERS_OCI_AUTH` support

8. **Output format compliance**
   - Add `featureId` and `publishedTags` to JSON output
   - Add `--log-level` flag

9. **Testing & validation**
   - Add tests for all spec test cases
   - Semantic version validation
   - Improved error messages

---

## 10. Breaking Changes Required

### CLI Interface Change
**Current:**
```bash
deacon features publish . --registry ghcr.io/owner/repo/feature:1.2.3
```

**Spec-Compliant:**
```bash
deacon features publish . --registry ghcr.io --namespace owner/repo
# Feature ID and version come from devcontainer-feature.json
```

**Migration Path:**
1. Add `--namespace` as required flag
2. Make `--registry` optional with default `ghcr.io`
3. Parse version from feature metadata instead of registry string
4. Announce breaking change in changelog
5. Update all examples and documentation

---

## 11. Estimated Effort

| Phase | Components | Estimated Effort | Risk |
|-------|-----------|------------------|------|
| Phase 1 (Core) | CLI args, tag listing, semver logic, multi-tag loop | 3-5 days | Medium |
| Phase 2 (Collection) | Collection metadata format & upload | 2-3 days | Low |
| Phase 3 (Polish) | Auth, output format, tests | 2-3 days | Low |
| **Total** | | **7-11 days** | |

**Assumptions:**
- Developer familiar with Rust and OCI specs
- Access to test registry for validation
- May require OCI client library enhancements

---

## 12. References

- **Specification:** `/workspaces/deacon/docs/subcommand-specs/features-publish/SPEC.md`
- **Data Structures:** `/workspaces/deacon/docs/subcommand-specs/features-publish/DATA-STRUCTURES.md`
- **Diagrams:** `/workspaces/deacon/docs/subcommand-specs/features-publish/DIAGRAMS.md`
- **Implementation:** `/workspaces/deacon/crates/deacon/src/commands/features.rs`
- **OCI Client:** `/workspaces/deacon/crates/core/src/oci.rs`
- **Related Spec:** [containers.dev Feature Distribution Spec](https://containers.dev/implementors/features-distribution/)

---

## Conclusion

The `features publish` implementation is **approximately 30-40% complete** compared to the specification. The basic publish flow exists, but critical functionality like semantic version tagging (which is the primary value-add of the command) and collection metadata publishing are entirely missing.

**Immediate Actions Required:**
1. Implement semantic version tagging (P0 - CRITICAL)
2. Add `--namespace` CLI flag (P0 - breaking change)
3. Implement idempotency checks (P0 - HIGH)
4. Complete authentication support (P0 - HIGH)
5. Add collection metadata publishing (P0 - CRITICAL)

Without these changes, the command cannot fulfill its specified purpose and provides limited value compared to manual OCI artifact uploads.
