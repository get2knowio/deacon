# Contract: Feature Reference Type Detection

**Feature**: 009-complete-feature-support
**Date**: 2025-12-28

## Purpose

Defines the contract for detecting and parsing different types of feature references in devcontainer.json.

---

## Reference Types

| Type | Pattern | Example |
|------|---------|---------|
| OCI Registry | No special prefix | `ghcr.io/devcontainers/features/node:18` |
| Local Path | Starts with `./` or `../` | `./local-feature`, `./nested/local-feature` |
| HTTPS Tarball | Starts with `https://` | `https://example.com/feature.tgz` |

---

## Input Contract

Feature references are keys in the `features` object of devcontainer.json:

```json
{
    "features": {
        "ghcr.io/devcontainers/features/node:18": {},
        "./local-feature": { "version": "1.0" },
        "https://example.com/feature.tgz": {}
    }
}
```

---

## Output Contract

### FeatureRefType Enum

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureRefType {
    /// OCI registry reference
    Oci(OciFeatureRef),
    /// Local filesystem path
    LocalPath(PathBuf),
    /// HTTPS tarball URL
    HttpsTarball(Url),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OciFeatureRef {
    pub registry: String,
    pub namespace: String,
    pub name: String,
    pub tag: Option<String>,
}
```

---

## Detection Rules

### Rule 1: Local Path Detection

```
IF reference.starts_with("./") OR reference.starts_with("../")
THEN FeatureRefType::LocalPath
```

**Valid Examples**:
- `./local-feature` → `LocalPath("./local-feature")`
- `./deeply/nested/feature` → `LocalPath("./deeply/nested/feature")`
- `../sibling-feature` → `LocalPath("../sibling-feature")` (detected as local; see the
  containment rule below — it only resolves when it still lands inside
  `<workspace>/.devcontainer/`)

**Invalid (NOT local paths)**:
- `/absolute/path` → Error: a local Feature may **not** be referenced by absolute path
- `feature` → OCI reference (no prefix)

### Rule 1a: Containment

Detection classifies the reference; resolution then enforces the two path clauses of
`devcontainer-features-distribution.md` §Locally Referenced Features, in the reference
CLI's order:

1. **No absolute paths.** "A local Feature may **not** be referenced by absolute path."
   Checked unconditionally, before any path resolution.
2. **Must be contained.** "A local Feature's source code **must** be contained within a
   sub-folder of the `.devcontainer/` folder." The containment anchor is
   `<workspace root>/.devcontainer`, independent of where the active config file sits.

Local paths are **config-dir-relative**, not workspace-relative: they resolve against the
directory holding the active `devcontainer.json`. The two spellings below name the same
directory, and both are contained:

| Config location | Reference | Resolves to |
|---|---|---|
| `<workspace>/.devcontainer/devcontainer.json` | `./my-feature` | `<workspace>/.devcontainer/my-feature` |
| `<workspace>/.devcontainer.json` (root) | `./.devcontainer/my-feature` | `<workspace>/.devcontainer/my-feature` |

A `../` that escapes `<workspace>/.devcontainer/` (e.g. `../shared-features/common`) is
rejected.

### Rule 2: HTTPS URL Detection

```
IF reference.starts_with("https://")
THEN FeatureRefType::HttpsTarball
```

**Valid Examples**:
- `https://example.com/feature.tgz` → `HttpsTarball(url)`
- `https://github.com/user/repo/releases/download/v1/feature.tar.gz` → `HttpsTarball(url)`

**Invalid**:
- `http://example.com/feature.tgz` → Error (HTTP not supported)
- `https://` → Error (incomplete URL)

### Rule 3: OCI Reference (Default)

```
IF NOT local_path AND NOT https_url
THEN FeatureRefType::Oci
```

Uses existing `parse_registry_reference()` for OCI parsing.

**Valid Examples**:
- `ghcr.io/devcontainers/features/node:18` → Full OCI ref
- `devcontainers/features/node` → Shorthand (default registry)
- `node:18` → Shortest form (default registry + namespace)

---

## Function Signature

```rust
/// Parse a feature reference string into a typed reference
///
/// # Arguments
/// * `reference` - Feature reference from devcontainer.json features key
///
/// # Returns
/// * `Ok(FeatureRefType)` - Parsed and validated reference
/// * `Err` - Invalid reference format
///
/// # Errors
/// * Local path format invalid
/// * HTTPS URL parse failure
/// * HTTP URL (HTTPS required)
/// * OCI reference parse failure
pub fn parse_feature_reference(reference: &str) -> Result<FeatureRefType>;
```

---

## Fetching Contract

Each reference type has a different fetch mechanism:

### OCI Registry

```rust
/// Fetch feature from OCI registry
async fn fetch_oci_feature(
    oci_ref: &OciFeatureRef,
    oci_client: &OciClient,
) -> Result<DownloadedFeature>;
```

Uses existing OCI client infrastructure.

### Local Path

```rust
/// Load feature from local filesystem
///
/// # Arguments
/// * `path` - Relative path from reference
/// * `config_dir` - Directory containing devcontainer.json
///
/// # Returns
/// Parsed feature metadata from {resolved_path}/devcontainer-feature.json
fn load_local_feature(
    path: &Path,
    config_dir: &Path,
) -> Result<DownloadedFeature>;
```

**Resolution**:
1. Join `config_dir` with relative `path`
2. Canonicalize to resolve `..` and symlinks
3. Read `devcontainer-feature.json` from resolved path
4. Parse as `FeatureMetadata`

**Errors**:
- Path does not exist: `"Local feature not found: {path}"`
- No metadata file: `"Missing devcontainer-feature.json in: {path}"`
- Invalid JSON: `"Failed to parse feature metadata: {path}"`

### HTTPS Tarball

```rust
/// Download and extract feature from HTTPS URL
///
/// # Arguments
/// * `url` - HTTPS URL to tarball
///
/// # Configuration
/// * Timeout: 30 seconds
/// * Retries: 1 on transient errors
///
/// # Returns
/// Extracted feature in temp directory
async fn fetch_https_feature(
    url: &Url,
) -> Result<DownloadedFeature>;
```

**Download Process**:
1. HTTP GET with 30-second timeout
2. On 5xx or timeout: retry once
3. Extract tarball to temp directory
4. Parse `devcontainer-feature.json`

**Errors**:
- HTTP 404: `"Feature not found: {url}"`
- HTTP 401/403: `"Access denied: {url}"`
- Timeout: `"Download timed out after 30s: {url}"`
- Invalid tarball: `"Failed to extract feature: {url}"`
- Missing metadata: `"No devcontainer-feature.json in tarball: {url}"`

---

## Validation Contract

### Local Path Validation

| Check | Error |
|-------|-------|
| Path is relative (starts with ./ or ../) | Enforced by detection |
| Path is not absolute | `"Absolute path to a local Feature is not allowed: {path}"` |
| Resolved path is a child of `<workspace>/.devcontainer/` | `"Local file path parse error. Resolved path must be a child of the .devcontainer/ folder."` |
| Resolved path exists | `"Local feature not found: {path}"` |
| devcontainer-feature.json exists | `"Missing devcontainer-feature.json in: {path}"` |
| JSON is valid | `"Failed to parse feature metadata: {path}: {error}"` |

### HTTPS URL Validation

| Check | Error |
|-------|-------|
| URL parses correctly | `"Invalid URL: {url}"` |
| Scheme is https (not http) | `"HTTP not supported, use HTTPS: {url}"` |
| URL has host | `"Invalid URL (no host): {url}"` |

### OCI Reference Validation

Uses existing validation from `parse_registry_reference()`.

---

## Examples

### Example 1: Mixed Reference Types

**Input** (devcontainer.json):
```json
{
    "features": {
        "ghcr.io/devcontainers/features/node:18": {"nodeVersion": "18"},
        "./my-feature": {},
        "https://releases.example.com/feature-1.0.tgz": {}
    }
}
```

**Parsed References**:
```rust
[
    FeatureRefType::Oci(OciFeatureRef {
        registry: "ghcr.io",
        namespace: "devcontainers/features",
        name: "node",
        tag: Some("18"),
    }),
    FeatureRefType::LocalPath(PathBuf::from("./my-feature")),
    FeatureRefType::HttpsTarball(Url::parse("https://releases.example.com/feature-1.0.tgz")),
]
```

### Example 2: Local Path Resolution

**Config Location**: `/workspace/.devcontainer/devcontainer.json`
**Reference**: `./shared/my-feature`

**Resolution**:
1. Join against the config directory: `/workspace/.devcontainer/./shared/my-feature`
2. Canonicalize: `/workspace/.devcontainer/shared/my-feature`
3. Check containment against `/workspace/.devcontainer` → contained, accepted
4. Read: `/workspace/.devcontainer/shared/my-feature/devcontainer-feature.json`

### Example 2b: Local Path Resolution from a Root-Level Config

**Config Location**: `/workspace/.devcontainer.json`
**Reference**: `./.devcontainer/my-feature`

**Resolution**:
1. Join against the config directory: `/workspace/./.devcontainer/my-feature`
2. Canonicalize: `/workspace/.devcontainer/my-feature`
3. Check containment against `/workspace/.devcontainer` → contained, accepted
4. Read: `/workspace/.devcontainer/my-feature/devcontainer-feature.json`

### Example 2c: Rejected — Escapes Containment

**Config Location**: `/workspace/.devcontainer/devcontainer.json`
**Reference**: `../shared-features/my-feature`

Resolves to `/workspace/shared-features/my-feature`, which is outside
`/workspace/.devcontainer` → rejected with "Local file path parse error. Resolved path
must be a child of the .devcontainer/ folder."

---

## Error Scenarios

| Input | Error |
|-------|-------|
| `http://example.com/f.tgz` | "HTTP not supported, use HTTPS: http://..." |
| `/absolute/path/feature` | "Absolute path to a local Feature is not allowed: /absolute/path/feature" |
| `../outside-feature` (escaping `.devcontainer/`) | "Local file path parse error. Resolved path must be a child of the .devcontainer/ folder." |
| `./missing-feature` | "Local feature not found: ./missing-feature" |
| `./no-metadata-feature` | "Missing devcontainer-feature.json in: ./no-metadata-feature" |
| `https://example.com/404.tgz` | "Feature not found: https://example.com/404.tgz" |
| `invalid:oci:ref` | OCI parse error from existing parser |

---

## Testing Requirements

1. **Unit Tests**: Reference type detection for all patterns
2. **Local Path Tests**: Resolution, missing path, missing metadata
3. **HTTPS Tests**: Mock server for success, 404, timeout, retry
4. **Integration Tests**: Full feature installation from each type
