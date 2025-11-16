# Features Info Implementation Gap Analysis

**Date:** October 13, 2025  
**Specification Source:** `/workspaces/deacon/docs/subcommand-specs/features-info/`  
**Implementation Source:** `/workspaces/deacon/crates/deacon/src/commands/features.rs`

## Executive Summary

The current implementation of the `features info` subcommand is a **stub/placeholder** that provides basic feature metadata display but **does not implement the core OCI registry query functionality** specified in the official documentation. The implementation requires significant work to match the specification.

**Overall Compliance: ~15%** ❌

---

## 1. Command-Line Interface Gaps

### 1.1 CLI Flags and Options

| Specification | Implementation | Status | Notes |
|--------------|----------------|--------|-------|
| `<mode>` positional arg | ✅ Implemented | ✅ | Accepts: manifest, tags, dependencies, verbose |
| `<feature>` positional arg | ✅ Implemented | ✅ | Accepts local path or registry reference |
| `--log-level <info\|debug\|trace>` | ❌ Missing | ❌ | Not implemented at all |
| `--output-format <text\|json>` | ⚠️ Wrong flag | ⚠️ | Uses `--json` boolean instead of `--output-format` enum |

**Impact:** High  
**Recommendation:** 
1. Add `--log-level` flag to CLI definition in `cli.rs`
2. Replace `--json` with `--output-format` that accepts `text` (default) or `json`

---

## 2. Mode-Specific Implementation Gaps

### 2.1 Manifest Mode

**Specification Requirements:**
- Fetch actual OCI manifest from registry using `fetchOCIManifestIfExists(ref)`
- Calculate and return canonical identifier (format: `registry/namespace/name@sha256:...`)
- **Text output:** Boxed sections with headers "Manifest" and "Canonical Identifier"
- **JSON output:** `{ "manifest": {OCI manifest object}, "canonicalId": "..." }`
- Error handling: Exit 1 with `{}` in JSON mode if manifest not found

**Current Implementation:**
```rust
fn output_manifest_info(metadata: &FeatureMetadata, json: bool)
```

**Issues:**

| Issue | Severity | Description |
|-------|----------|-------------|
| Wrong data source | 🔴 Critical | Outputs `FeatureMetadata` (devcontainer-feature.json) instead of OCI manifest | 
| No canonical ID | 🔴 Critical | Doesn't calculate or output canonical identifier with digest |
| Missing OCI manifest fetch | 🔴 Critical | Doesn't call `get_manifest()` or interact with registry |
| Wrong JSON structure | 🔴 Critical | Outputs feature metadata fields, not `{manifest, canonicalId}` |
| No boxed text formatting | 🟡 Medium | Text output doesn't use boxed sections with headers |
| Missing error handling | 🟡 Medium | Doesn't return `{}` and exit 1 on manifest not found in JSON mode |

**Example of Expected vs Actual Output:**

Expected (JSON):
```json
{
  "manifest": {
    "schemaVersion": 2,
    "mediaType": "application/vnd.oci.image.manifest.v1+json",
    "config": { ... },
    "layers": [ ... ]
  },
  "canonicalId": "ghcr.io/devcontainers/features/node@sha256:abc123..."
}
```

Actual (JSON):
```json
{
  "id": "node",
  "version": "1.0.0",
  "name": "Node.js",
  "description": "...",
  // ... feature metadata fields
}
```

**Compliance: 10%** ❌

---

### 2.2 Tags Mode

**Specification Requirements:**
- Query registry for all published tags using `getPublishedTags(ref)`
- **Text output:** Boxed "Published Tags" section with list
- **JSON output:** `{ "publishedTags": ["1", "1.2", "1.2.3", "latest"] }`
- Error handling: Exit 1 with `{}` in JSON mode if no tags found

**Current Implementation:**
```rust
async fn output_tags_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    json: bool,
)
```

**Issues:**

| Issue | Severity | Description |
|-------|----------|-------------|
| No registry querying | 🔴 Critical | Placeholder implementation - just returns current version from metadata |
| Missing OCI tags API | 🔴 Critical | No implementation of OCI Distribution Spec tags listing (`/v2/<name>/tags/list`) |
| Wrong JSON structure | 🔴 Critical | Returns `{id, tags}` instead of `{publishedTags}` |
| No boxed text formatting | 🟡 Medium | Text output doesn't use boxed "Published Tags" section |
| Missing error handling | 🟡 Medium | Doesn't handle empty tags case with exit 1 and `{}` in JSON mode |

**Current Code (Placeholder):**
```rust
// Note: This is a placeholder - in a full implementation, we would
// query the OCI registry for available tags
// For now, we'll just return the current version if available
vec![metadata.version.clone().unwrap_or_else(|| "latest".to_string())]
```

**What's Missing:**
- OCI registry HTTP client method: `GET /v2/<namespace>/<name>/tags/list`
- Parsing OCI tags list response
- Handling pagination for large tag lists
- Authentication for private registries

**Compliance: 5%** ❌

---

### 2.3 Dependencies Mode

**Specification Requirements:**
- Build dependency graph from feature metadata
- Generate Mermaid diagram representation
- **Text output ONLY:** Boxed "Dependency Tree" section with Mermaid syntax
- **JSON mode:** Should NOT output dependency graph (per spec decision)
- Render hint: "Dependency Tree (Render with https://mermaid.live/)"

**Current Implementation:**
```rust
fn output_dependencies_info(metadata: &FeatureMetadata, json: bool)
```

**Issues:**

| Issue | Severity | Description |
|-------|----------|-------------|
| Outputs JSON | 🔴 Critical | Spec explicitly states dependencies mode is text-only |
| No Mermaid generation | 🔴 Critical | Doesn't generate Mermaid diagram syntax |
| No boxed formatting | 🟡 Medium | Missing boxed section with header |
| No render hint | 🟡 Medium | Doesn't include mermaid.live URL hint |
| Simple list vs graph | 🟡 Medium | Just lists dependencies, doesn't show graph structure |

**Current Text Output:**
```
Dependencies for 'node':
  Installs After:
    - common-utils
  Depends On:
    - github-cli: true
```

**Expected Text Output:**
```
╔════════════════════════════════════════════════════════════╗
║ Dependency Tree (Render with https://mermaid.live/)        ║
╚════════════════════════════════════════════════════════════╝

graph TD
    node --> common-utils
    node --> github-cli
    common-utils --> base
```

**What's Missing:**
- Mermaid graph generation from dependency relationships
- Proper graph traversal to show transitive dependencies
- Boxed section formatting utility
- Proper handling of JSON mode (should output nothing or error)

**Compliance: 20%** ⚠️

---

### 2.4 Verbose Mode

**Specification Requirements:**
- Combine output from manifest + tags + dependencies modes
- **Text output:** All three boxed sections (Manifest, Canonical ID, Published Tags, Dependency Tree)
- **JSON output:** Union of manifest and tags data only (no dependency graph)
  ```json
  {
    "manifest": { ... },
    "canonicalId": "...",
    "publishedTags": [ ... ]
  }
  ```

**Current Implementation:**
```rust
fn output_verbose_info(
    metadata: &FeatureMetadata,
    registry_url: Option<&str>,
    namespace: Option<&str>,
    name: Option<&str>,
    tag: Option<&str>,
    digest: Option<&str>,
    json: bool,
)
```

**Issues:**

| Issue | Severity | Description |
|-------|----------|-------------|
| Wrong approach | 🔴 Critical | Outputs all metadata fields instead of combining mode outputs |
| No manifest fetching | 🔴 Critical | Doesn't fetch and include OCI manifest |
| No tags querying | 🔴 Critical | Doesn't query and include published tags |
| No dependency graph | 🔴 Critical | Doesn't include Mermaid dependency tree in text mode |
| Wrong JSON structure | 🔴 Critical | Custom structure instead of manifest+canonicalId+publishedTags |
| No boxed sections | 🟡 Medium | Text output doesn't use boxed sections |

**Expected Structure:**
The verbose mode should essentially call the three individual mode functions and combine their outputs.

**Compliance: 15%** ❌

---

## 3. Infrastructure and Support Code Gaps

### 3.1 OCI Registry Tag Listing

**Status:** ❌ Not Implemented

**Required Functionality:**
```rust
// Needed in OCI module
pub async fn list_tags(&self, feature_ref: &FeatureRef) -> Result<Vec<String>>
```

**Implementation Requirements:**
- HTTP GET to `/v2/<namespace>/<name>/tags/list`
- Handle OCI Distribution Spec v2 tags response:
  ```json
  {
    "name": "<namespace>/<name>",
    "tags": ["1.0.0", "1.1.0", "latest"]
  }
  ```
- Support pagination via `Link` headers
- Handle authentication (Bearer tokens)
- Error handling for 404, 401, 403
- Retry logic with exponential backoff

**Dependencies:**
- Extends `FeatureFetcher` in `crates/core/src/oci.rs`
- Uses existing `HttpClient` trait
- Leverages existing authentication infrastructure

---

### 3.2 Canonical Identifier Calculation

**Status:** ❌ Not Implemented

**Required Functionality:**
```rust
pub fn canonical_id(manifest: &Manifest, feature_ref: &FeatureRef) -> String
```

**Implementation Requirements:**
- Extract digest from manifest fetch response (Docker-Content-Digest header)
- Format: `{registry}/{namespace}/{name}@{digest}`
- Example: `ghcr.io/devcontainers/features/node@sha256:abc123...`
- Handle local features (may not have canonical ID)

---

### 3.3 Mermaid Diagram Generation

**Status:** ❌ Not Implemented

**Required Functionality:**
```rust
pub fn generate_mermaid_graph(metadata: &FeatureMetadata, all_features: &[FeatureMetadata]) -> String
```

**Implementation Requirements:**
- Parse `installsAfter` and `dependsOn` from feature metadata
- Build directed graph representation
- Generate Mermaid syntax (graph TD or graph LR)
- Handle cycles (though resolver should catch these)
- Format as:
  ```
  graph TD
      feature-a --> feature-b
      feature-a --> feature-c
      feature-b --> feature-d
  ```

**Considerations:**
- May need to fetch dependency feature metadata to show full graph
- Or only show immediate dependencies from current feature

---

### 3.4 Boxed Text Output Formatting

**Status:** ❌ Not Implemented

**Required Functionality:**
```rust
pub fn print_boxed_section(title: &str, content: &str)
```

**Implementation Requirements:**
- Unicode box drawing characters
- Dynamic width based on content or terminal width
- Example:
  ```
  ╔═══════════════════════════════════╗
  ║ Manifest                          ║
  ╚═══════════════════════════════════╝
  ```

**Alternative:** Use existing crate like `term-table`, `cli-table`, or `prettytable-rs`

---

### 3.5 Error Handling for JSON Mode

**Status:** ❌ Not Implemented

**Required Behavior:**
- On any error in JSON mode, output `{}` and exit with code 1
- Text mode can output error messages normally
- Examples:
  - Feature not found: `{}` (JSON) or "Feature not found: ..." (text)
  - No manifest: `{}` (JSON) or "No manifest found! You may need to log in." (text)
  - No tags: `{}` (JSON) or "No published versions found ..." (text)

**Implementation Pattern:**
```rust
if json {
    println!("{{}}");
    std::process::exit(1);
} else {
    eprintln!("Error: {}", error_message);
    std::process::exit(1);
}
```

---

## 4. Data Structure Mismatches

### 4.1 JSON Output Structure

**Specification (DATA-STRUCTURES.md):**
```json
{
  "manifest": { /* OCIManifest object */ },
  "canonicalId": "registry/namespace/name@sha256:...",
  "publishedTags": ["1", "1.2", "1.2.3", "latest"]
}
```

**Current Implementation:**
- Manifest mode: Outputs flattened feature metadata
- Tags mode: Outputs `{id, tags}` instead of `{publishedTags}`
- Verbose mode: Outputs custom structure with feature metadata fields

**Required Changes:**
- Define proper JSON schema structures
- Use OCI Manifest type directly (already exists as `Manifest` struct)
- Standardize on `publishedTags` key name
- Ensure canonical ID is always included where applicable

---

## 5. Test Coverage Gaps

**Current State:** No tests exist for `features info` command

**Required Test Coverage:**

1. **Manifest Mode Tests:**
   - ✅ Test with local feature path
   - ✅ Test with registry reference (public)
   - ✅ Test with registry reference (private, authenticated)
   - ✅ Test manifest not found (exit 1, `{}` in JSON)
   - ✅ Test text vs JSON output formats

2. **Tags Mode Tests:**
   - ✅ Test with registry reference returning multiple tags
   - ✅ Test with local feature (no tags available)
   - ✅ Test with empty tags list (exit 1, `{}` in JSON)
   - ✅ Test pagination for many tags

3. **Dependencies Mode Tests:**
   - ✅ Test simple dependency tree
   - ✅ Test transitive dependencies
   - ✅ Test with no dependencies
   - ✅ Test Mermaid output format
   - ✅ Verify no JSON output in dependencies mode

4. **Verbose Mode Tests:**
   - ✅ Test combined output (manifest + tags + dependencies)
   - ✅ Test JSON structure (manifest + canonicalId + publishedTags only)
   - ✅ Test text output (all boxed sections)

5. **Error Handling Tests:**
   - ✅ Test invalid feature reference
   - ✅ Test network errors
   - ✅ Test authentication failures
   - ✅ Test JSON mode error output (`{}`)

6. **Integration Tests:**
   - ✅ Test against real public registry (ghcr.io)
   - ✅ Test against mock OCI registry
   - ✅ Test with various authentication methods

---

## 6. Priority Ranking for Implementation

### P0 - Critical (Blocking Basic Functionality)
1. **Implement OCI manifest fetching in manifest mode** - Currently shows wrong data
2. **Implement canonical ID calculation** - Required by spec
3. **Fix JSON output structure for all modes** - Current structure doesn't match spec
4. **Add `--output-format` flag** - Current `--json` flag doesn't match spec

### P1 - High (Core Features)
5. **Implement OCI tag listing functionality** - Tags mode is a placeholder
6. **Add error handling with `{}` output in JSON mode** - Spec requirement
7. **Implement Mermaid diagram generation** - Dependencies mode incomplete
8. **Add boxed text formatting** - Visual output requirement

### P2 - Medium (Polish & Completeness)
9. **Add `--log-level` flag support** - Spec requirement but lower impact
10. **Remove JSON output from dependencies mode** - Spec says text-only
11. **Implement verbose mode as combination of other modes** - Should delegate to mode functions
12. **Add comprehensive test suite** - Ensure reliability

### P3 - Low (Future Enhancements)
13. **Support for local features in all modes** - Currently partially supported
14. **Tag pagination support** - For features with many versions
15. **Enhanced error messages** - More helpful diagnostics

---

## 7. Code Refactoring Recommendations

### 7.1 Separate Concerns

**Current Issue:** Single large function with inline output formatting

**Recommendation:**
```rust
// Separate data fetching from output formatting
async fn fetch_manifest_data(feature_ref: &FeatureRef) -> Result<ManifestData>;
async fn fetch_tags_data(feature_ref: &FeatureRef) -> Result<Vec<String>>;
async fn fetch_dependencies_data(metadata: &FeatureMetadata) -> Result<String>; // Mermaid

// Separate output formatting
fn format_manifest_text(data: &ManifestData) -> String;
fn format_manifest_json(data: &ManifestData) -> serde_json::Value;
fn format_tags_text(tags: &[String]) -> String;
fn format_tags_json(tags: &[String]) -> serde_json::Value;
```

### 7.2 Use Type-Safe Output Format

**Current Issue:** Boolean `json` parameter everywhere

**Recommendation:**
```rust
enum OutputFormat {
    Text,
    Json,
}

// Cleaner function signatures
fn output_manifest_info(data: &ManifestData, format: OutputFormat) -> Result<()>
```

### 7.3 Create Dedicated Data Types

**Recommendation:**
```rust
#[derive(Debug, Serialize)]
struct ManifestData {
    manifest: Manifest,
    canonical_id: String,
}

#[derive(Debug, Serialize)]
struct InfoOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<Manifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_tags: Option<Vec<String>>,
}
```

---

## 8. Specification Compliance Checklist

### Command-Line Interface
- [ ] `<mode>` argument accepts: manifest, tags, dependencies, verbose
- [ ] `<feature>` argument accepts local path or registry reference  
- [ ] `--log-level <info|debug|trace>` flag implemented
- [ ] `--output-format <text|json>` flag implemented (not `--json`)
- [ ] Default output format is `text`

### Manifest Mode
- [ ] Fetches actual OCI manifest from registry
- [ ] Calculates canonical identifier with digest
- [ ] Text output: Boxed "Manifest" section with JSON
- [ ] Text output: Boxed "Canonical Identifier" section
- [ ] JSON output: `{manifest, canonicalId}` structure
- [ ] Error handling: Exit 1 with `{}` in JSON mode on failure

### Tags Mode  
- [ ] Queries registry `/v2/<name>/tags/list` endpoint
- [ ] Returns array of published tags
- [ ] Text output: Boxed "Published Tags" section with list
- [ ] JSON output: `{publishedTags: [...]}` structure
- [ ] Error handling: Exit 1 with `{}` if no tags in JSON mode
- [ ] Handles pagination for large tag lists

### Dependencies Mode
- [ ] Builds dependency graph from feature metadata
- [ ] Generates Mermaid diagram syntax
- [ ] Text output: Boxed "Dependency Tree" section
- [ ] Text output: Includes mermaid.live render hint
- [ ] Does NOT output JSON in dependencies mode
- [ ] Shows transitive dependencies (optional enhancement)

### Verbose Mode
- [ ] Combines manifest + tags + dependencies
- [ ] Text output: All three boxed sections
- [ ] JSON output: `{manifest, canonicalId, publishedTags}` only (no graph)
- [ ] Delegates to individual mode functions
- [ ] Handles errors from any sub-mode appropriately

### Error Handling
- [ ] Invalid feature reference: Exit 1, `{}` in JSON mode
- [ ] Manifest not found: Exit 1, error message or `{}`
- [ ] No tags found: Exit 1, error message or `{}`
- [ ] Auth required: Exit 1, helpful message
- [ ] Network errors: Exit 1, error message or `{}`

### Output Formatting
- [ ] Boxed sections in text mode using Unicode characters
- [ ] JSON output is properly formatted with `serde_json`
- [ ] Consistent error output between modes
- [ ] Exit codes: 0 = success, 1 = error

---

## 9. Example Reference Implementation Pseudocode

Based on the specification's section 5 (Core Execution Logic):

```rust
async fn execute_features_info(mode: &str, feature: &str, output_format: OutputFormat) -> Result<()> {
    // Parse feature reference (local path or registry)
    let feature_ref = parse_feature_ref(feature)?;
    
    let mut output = InfoOutput::default();
    
    // Manifest or Verbose mode
    if mode == "manifest" || mode == "verbose" {
        match fetch_oci_manifest(&feature_ref).await {
            Ok((manifest, canonical_id)) => {
                if output_format == OutputFormat::Text {
                    print_boxed_section("Manifest", &serde_json::to_string_pretty(&manifest)?);
                    print_boxed_section("Canonical Identifier", &canonical_id);
                } else {
                    output.manifest = Some(manifest);
                    output.canonical_id = Some(canonical_id);
                }
            }
            Err(e) => {
                return handle_error(&e, output_format);
            }
        }
    }
    
    // Tags or Verbose mode
    if mode == "tags" || mode == "verbose" {
        match fetch_published_tags(&feature_ref).await {
            Ok(tags) if !tags.is_empty() => {
                if output_format == OutputFormat::Text {
                    print_boxed_section("Published Tags", &format_tag_list(&tags));
                } else {
                    output.published_tags = Some(tags);
                }
            }
            _ => {
                return handle_error(&anyhow!("No published versions found"), output_format);
            }
        }
    }
    
    // Dependencies or Verbose mode (text only)
    if (mode == "dependencies" || mode == "verbose") && output_format == OutputFormat::Text {
        let metadata = get_feature_metadata(&feature_ref).await?;
        let mermaid = generate_mermaid_graph(&metadata)?;
        print_boxed_section(
            "Dependency Tree (Render with https://mermaid.live/)",
            &mermaid
        );
    }
    
    // Output JSON if applicable
    if output_format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    }
    
    Ok(())
}

fn handle_error(error: &anyhow::Error, format: OutputFormat) -> Result<()> {
    if format == OutputFormat::Json {
        println!("{{}}");
    } else {
        eprintln!("Error: {}", error);
    }
    std::process::exit(1);
}
```

---

## 10. Related Work and Dependencies

### Existing Code to Leverage
- ✅ `FeatureFetcher::get_manifest()` in `crates/core/src/oci.rs` (line ~924)
- ✅ `FeatureFetcher::fetch_feature()` for downloading features
- ✅ OCI authentication infrastructure
- ✅ `HttpClient` trait with GET/HEAD/POST/PUT methods
- ✅ `parse_registry_reference()` for parsing feature refs
- ✅ `FeatureMetadata` and `Manifest` structs already defined

### New Code Required
- ❌ `FeatureFetcher::list_tags()` method
- ❌ Canonical ID calculation from digest
- ❌ Mermaid graph generation
- ❌ Boxed text formatting utility
- ❌ JSON error output standardization

---

## 11. Summary and Recommendations

### Current State
The `features info` implementation is a **minimal stub** that:
- ✅ Accepts the correct command structure
- ✅ Can parse local and registry feature references
- ⚠️ Outputs feature metadata (not OCI manifest data)
- ❌ Does not implement OCI registry queries
- ❌ Does not match specification output formats
- ❌ Missing critical features like tag listing and dependency graphs

### Critical Path to Compliance

**Phase 1: Core OCI Functionality (P0)**
1. Implement OCI manifest fetching in manifest mode
2. Implement canonical ID calculation from manifest digest
3. Fix JSON output structures to match spec
4. Replace `--json` with `--output-format`

**Phase 2: Registry Queries (P1)**
5. Implement OCI tag listing API call
6. Add proper error handling with `{}` output in JSON mode
7. Update tags mode to use real registry data

**Phase 3: Dependencies & Formatting (P1-P2)**
8. Implement Mermaid diagram generation
9. Add boxed text formatting utility
10. Fix dependencies mode (remove JSON, add Mermaid)
11. Implement verbose mode as combination of other modes

**Phase 4: Polish (P2-P3)**
12. Add `--log-level` support
13. Comprehensive test suite
14. Documentation and examples

### Estimated Effort
- **Phase 1:** 3-5 days (critical path)
- **Phase 2:** 2-3 days 
- **Phase 3:** 3-4 days
- **Phase 4:** 2-3 days
- **Total:** ~2-3 weeks for full compliance

### Conclusion

The `features info` subcommand requires **substantial implementation work** to match the specification. The current code is approximately **15% compliant** with the spec and serves primarily as a placeholder. The most critical gaps are:

1. No actual OCI manifest fetching (using wrong data source)
2. No registry tag listing functionality
3. Wrong JSON output structures across all modes
4. No dependency graph generation
5. Missing boxed text formatting

**Recommendation:** Prioritize Phase 1 work to establish core OCI functionality, then proceed through the phases sequentially. The existing OCI infrastructure provides a good foundation, but significant new code is required for tag listing, canonical IDs, and Mermaid generation.
