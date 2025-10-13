# Outdated Subcommand Implementation Gap Analysis

**Generated**: October 13, 2025  
**Status**: Not Implemented  
**Specification**: `/docs/subcommand-specs/outdated/`

---

## Executive Summary

The `outdated` subcommand, as specified in the official documentation, is **completely missing** from the current implementation. This command is critical for helping developers understand upgrade opportunities for Features declared in devcontainer configurations before running `upgrade` or `build` commands.

**Severity**: High - This is a user-facing feature explicitly mentioned in CLI parity documentation and required for complete DevContainer CLI compatibility.

---

## 1. Missing CLI Interface

### Current State
- **No `Outdated` variant** in `Commands` enum (`crates/deacon/src/cli.rs`)
- No CLI argument parsing for outdated-specific flags
- No command handler or dispatcher entry

### Required Implementation (Per Spec)

#### CLI Enum Addition
```rust
/// Show current and available versions of features
Outdated {
    /// Output format (text or json)
    #[arg(long, value_enum, default_value = "text")]
    output_format: OutputFormat,
    
    /// Terminal columns hint (implies terminal_rows)
    #[arg(long, requires = "terminal_rows")]
    terminal_columns: Option<u32>,
    
    /// Terminal rows hint (implies terminal_columns)
    #[arg(long, requires = "terminal_columns")]
    terminal_rows: Option<u32>,
},
```

#### Required Flags
Per SPEC.md Section 2:
- ✅ `--workspace-folder` (already global)
- ✅ `--config` (already global)
- ❌ `--output-format <text|json>` (default: `text`) - **MISSING**
- ✅ `--log-level` (already global)
- ✅ `--log-format` (already global)
- ❌ `--terminal-columns <n>` - **MISSING**
- ❌ `--terminal-rows <n>` - **MISSING**

### Gap Assessment
- **Complete absence** of the subcommand
- No argument validation for terminal dimension mutual requirements
- No output format handling specific to outdated

---

## 2. Missing Core Execution Logic

### Current State
- No `commands/outdated.rs` module
- No implementation of the core execution pipeline

### Required Implementation Components

#### 2.1 Phase 1: Initialization
Per SPEC.md Section 5:
```pseudocode
- Create CLI host
- Initialize logger (format/level)
- Load workspace from path
- Discover or load config file
- Read adjacent lockfile (may be undefined)
```

**Status**: ❌ Not implemented

#### 2.2 Phase 2: Configuration Resolution
- Load `devcontainer.json` with variable substitution
- Parse features array: `user_features_to_array(config)`
- Read lockfile adjacent to config (`.devcontainer-lock.json` or `devcontainer-lock.json`)

**Status**: ❌ Not implemented  
**Note**: No lockfile reading utilities exist in codebase

#### 2.3 Phase 3: Version Resolution
Per SPEC.md Section 5, for each feature:
```pseudocode
1. Parse feature identifier → ref (tag/digest)
2. List published semver tags (sorted desc)
3. Compute "wanted" version:
   - If tag='latest': highest semver tag
   - If tag=specific: highest matching semver
   - If digest: fetch manifest metadata for version
4. Compute "current": lockfile version OR wanted
5. Compute "latest": highest published semver
6. Calculate major version strings
```

**Status**: ❌ Not implemented

**Missing Dependencies**:
- Lockfile data structures (`Lockfile`, `LockFeature` per DATA-STRUCTURES.md)
- OCI tag listing and filtering to semver-valid tags
- Digest-based metadata resolution
- Semver comparison/sorting utilities

#### 2.4 Phase 4: Output Formatting
- **Text mode**: Table with columns `Feature | Current | Wanted | Latest`
- **JSON mode**: `OutdatedResult` structure per DATA-STRUCTURES.md

**Status**: ❌ Not implemented

---

## 3. Missing Data Structures

### 3.1 Lockfile Support
Per DATA-STRUCTURES.md:

```rust
// ❌ NOT IMPLEMENTED
pub struct Lockfile {
    pub features: HashMap<String, LockFeature>,
}

pub struct LockFeature {
    pub version: String,        // semver
    pub resolved: String,       // canonical ref 'registry/path@sha256:...'
    pub integrity: String,      // digest 'sha256:...'
    pub depends_on: Option<Vec<String>>,
}
```

**Current State**: No lockfile reading/writing in `crates/core/`

### 3.2 Outdated Result Schema
Per DATA-STRUCTURES.md:

```rust
// ❌ NOT IMPLEMENTED
pub struct OutdatedResult {
    pub features: HashMap<String, FeatureVersionInfo>,
}

pub struct FeatureVersionInfo {
    pub current: Option<String>,       // lockfile or wanted
    pub wanted: Option<String>,        // derived from tag/digest
    pub wanted_major: Option<String>,  // major(wanted)
    pub latest: Option<String>,        // highest semver
    pub latest_major: Option<String>,  // major(latest)
}
```

### 3.3 Parsed Input Structure
```rust
// ❌ NOT IMPLEMENTED
pub struct OutdatedArgs {
    pub workspace_folder: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub output_format: OutputFormat,
    pub log_level: LogLevel,
    pub log_format: LogFormat,
    pub terminal_columns: Option<u32>,
    pub terminal_rows: Option<u32>,
}
```

---

## 4. Missing OCI/Registry Integration

### Required Capabilities

#### 4.1 Tag Listing & Filtering
Per SPEC.md Section 7:
```
GET https://<registry>/v2/<namespace>/<id>/tags/list
→ Filter to valid semantic versions
→ Sort ascending, then reverse to descending
```

**Status**: ❌ Not implemented  
**Note**: Existing `crates/core/src/oci.rs` may have foundation but needs:
- Tag enumeration endpoint support
- Semver validation filter
- Ascending/descending sort logic

#### 4.2 Digest Metadata Resolution
For digest-based refs without lockfile version:
```
1. Fetch manifest
2. Download blob if needed
3. Extract dev.containers.metadata or feature JSON
4. Return semantic version
```

**Status**: ❌ Not implemented  
**Complexity**: High - requires temporary file extraction

#### 4.3 Authentication
- Uses configured credential helpers (e.g., `docker login`)
- Requests include auth headers

**Status**: Partial - OCI module has auth, but tag listing integration unknown

---

## 5. Missing Helper Functions

### 5.1 Lockfile I/O
```rust
// ❌ NOT IMPLEMENTED
async fn read_lockfile_adjacent_to(config: &DevContainerConfig) -> Option<Lockfile>;
async fn write_lockfile(config: &DevContainerConfig, lockfile: &Lockfile) -> Result<()>;
fn lockfile_path(config_path: &Path) -> PathBuf;
```

**Location**: Should be in `crates/core/src/` (suggested: `lockfile.rs` module)

### 5.2 Version Resolution
```rust
// ❌ NOT IMPLEMENTED
async fn list_published_semver_tags_sorted(ref: &FeatureRef) -> Result<Vec<String>>;
async fn maybe_fetch_manifest_and_metadata(ref: &FeatureRef) -> Result<Option<String>>;
fn compute_wanted_version(ref: &OCIRef, lockfile: Option<&Lockfile>) -> Result<Option<String>>;
```

### 5.3 Output Rendering
```rust
// ❌ NOT IMPLEMENTED
fn render_outdated_table(result: &OutdatedResult) -> String;
fn render_outdated_json(result: &OutdatedResult) -> String;
```

---

## 6. Error Handling Gaps

### Required Behaviors (Per SPEC.md Section 9)

| Error Type | Required Behavior | Current Status |
|------------|-------------------|----------------|
| Config not found | Exit 1, stderr message | ✅ Exists (ConfigError::NotFound) |
| Terminal flags singly provided | CLI parsing error | ❌ Not enforced |
| Registry/network failures | Log error, continue with undefined fields, exit 0 | ❌ Not implemented |
| Invalid feature identifiers | Skip, don't fail | ❌ Not implemented |

**Key Gap**: Graceful degradation on registry errors is critical per spec:
> "do not fail the command; affected features produce `wanted/latest` as undefined. Overall exit remains 0."

---

## 7. Testing Requirements

### Missing Test Suites (Per SPEC.md Section 15)

```rust
// ❌ NOT IMPLEMENTED
#[test]
fn test_outdated_json_output_happy_path() {
    // GIVEN config with features (tagged, digest, untagged)
    // AND lockfile with some versions
    // WHEN outdated --output-format json
    // THEN response includes current/wanted/latest
}

#[test]
fn test_outdated_text_table_output() {
    // THEN stdout includes header and rows
}

#[test]
fn test_outdated_registry_failure_graceful() {
    // THEN exit code 0, undefined fields
}

#[test]
fn test_outdated_no_features() {
    // THEN JSON features={} or empty table
}
```

### Integration Test Fixtures Needed
- Sample configs with various feature types
- Mock lockfile data
- Mock registry responses (tags, manifests)

---

## 8. Cross-Cutting Concerns

### 8.1 Deterministic Ordering
Per SPEC.md Section 6:
> "Reorder to match config declaration order regardless of object key ordering semantics."

**Status**: ❌ Not implemented  
**Impact**: Output must preserve feature order from `devcontainer.json`

### 8.2 Parallel Execution
Per SPEC.md Section 5:
> "PARALLEL_FOR each feature IN features"

**Status**: ❌ Not implemented  
**Requirement**: Tag queries should run concurrently across features

### 8.3 Cache Management
Per SPEC.md Sections 6 & 11:
- Temporary files for digest metadata
- No persistent cache (command is stateless)

**Status**: ❌ Not implemented

---

## 9. Documentation Gaps

### User-Facing Documentation
- ❌ No help text for `deacon outdated --help`
- ❌ No examples in `examples/` directory
- ❌ Not mentioned in README.md
- ✅ Mentioned in CLI_PARITY.md as "not implemented"

### Developer Documentation
- ✅ Comprehensive spec in `/docs/subcommand-specs/outdated/`
- ❌ No implementation guide or API documentation

---

## 10. Implementation Roadmap

### Phase 1: Foundation (Est. 3-5 days)
1. **Create lockfile module** (`crates/core/src/lockfile.rs`)
   - Data structures (Lockfile, LockFeature)
   - Read/write functions
   - Path resolution (`.devcontainer-lock.json` vs `devcontainer-lock.json`)

2. **Extend OCI module** (`crates/core/src/oci.rs`)
   - Tag listing endpoint
   - Semver filter/sort utilities
   - Digest metadata extraction

3. **Add CLI interface** (`crates/deacon/src/cli.rs`)
   - Outdated variant in Commands enum
   - Argument validation (terminal dimensions)

### Phase 2: Core Logic (Est. 5-7 days)
4. **Create outdated command module** (`crates/deacon/src/commands/outdated.rs`)
   - ParsedInput → OutdatedResult pipeline
   - Configuration resolution
   - Feature iteration and version derivation
   - Error handling with graceful degradation

5. **Version resolution logic**
   - Wanted computation (tag/digest semantics)
   - Current from lockfile fallback
   - Latest from registry tags

### Phase 3: Output & Polish (Est. 2-3 days)
6. **Output rendering**
   - Text table formatter
   - JSON serialization
   - Terminal dimension handling

7. **Integration testing**
   - Happy path tests (JSON/text)
   - Registry failure scenarios
   - Empty features cases

### Phase 4: Documentation (Est. 1-2 days)
8. **User documentation**
   - Help text
   - Example workflows
   - README updates

9. **Validation against spec**
   - Cross-reference all requirements
   - Edge case coverage

---

## 11. Dependencies & Blockers

### External Dependencies
- ✅ `serde` / `serde_json` - Already in use
- ✅ `tokio` - Already in use
- ⚠️ `semver` crate - **Required for version parsing/comparison** (needs addition)
- ✅ OCI registry client - Exists but needs enhancement

### Internal Blockers
- **No lockfile infrastructure** - Critical blocker
- **No semver utilities** - Required for tag filtering and comparison
- **Limited OCI tag enumeration** - Needs extension

---

## 12. Specification Compliance Checklist

### Command Interface
- [ ] `--workspace-folder` (required) - validation
- [ ] `--config` (optional)
- [ ] `--output-format <text|json>` (default: text)
- [ ] `--terminal-columns` (implies rows)
- [ ] `--terminal-rows` (implies columns)
- [ ] Log flags (level, format)

### Core Functionality
- [ ] Config discovery and loading
- [ ] Lockfile reading (adjacent to config)
- [ ] Feature array extraction
- [ ] OCI reference parsing
- [ ] Tag listing and semver filtering
- [ ] Wanted version computation
- [ ] Current version from lockfile
- [ ] Latest version resolution
- [ ] Major version calculation
- [ ] Deterministic output ordering

### Output Formats
- [ ] JSON: `OutdatedResult` schema compliance
- [ ] Text: `Feature | Current | Wanted | Latest` table
- [ ] Undefined values as `-` in text mode
- [ ] Feature IDs without version suffix in output

### Error Handling
- [ ] Config not found → exit 1
- [ ] Registry failures → continue with undefined, exit 0
- [ ] Invalid identifiers → skip gracefully
- [ ] Terminal flags XOR validation

### Performance & Security
- [ ] Parallel tag queries
- [ ] Temporary file cleanup
- [ ] Auth credential handling
- [ ] No token logging

---

## 13. Related Work

### Similar Existing Commands (for reference patterns)
- ✅ `read-configuration` - Config loading pattern
- ✅ `features info` - OCI registry interaction
- ✅ `build` - Output format handling (text/json)
- ✅ `doctor` - System validation and reporting

### Reusable Components
- `ConfigLoader` - Configuration resolution
- `OciClient` - Registry communication
- `FeatureRef` - Feature identifier parsing
- `OutputFormat` enum - Already defined

---

## 14. Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Lockfile format divergence from spec | High | Validate against upstream TypeScript CLI examples |
| Registry API rate limits | Medium | Implement retry with backoff (already exists) |
| Semver parsing edge cases | Medium | Comprehensive unit tests, use battle-tested `semver` crate |
| Network timeouts affecting UX | Low | Clear progress indicators, fast-fail per feature |
| Lockfile read/write conflicts | Low | Read-only for outdated (no writes) |

---

## 15. Acceptance Criteria

### Must Have
1. ✅ Command accepts all required flags per spec
2. ✅ Produces valid JSON output matching schema
3. ✅ Produces readable text table output
4. ✅ Handles missing lockfile gracefully
5. ✅ Continues on registry failures with partial data
6. ✅ Preserves config feature order in output
7. ✅ All spec test cases pass
8. ✅ Zero clippy warnings
9. ✅ Formatted with `cargo fmt`
10. ✅ Integration tests cover happy path and error cases

### Should Have
1. Parallel tag queries for performance
2. Clear progress indicators for long operations
3. Example workflows in `examples/` directory
4. Comprehensive error messages

### Nice to Have
1. Caching of tag lists for repeated queries
2. Rich terminal output (colors, symbols)
3. Support for `--filter` to check specific features

---

## 16. Conclusion

The `outdated` subcommand is **entirely absent** from the current implementation. This represents a significant functionality gap requiring:

- **~2-3 weeks development effort** (including testing)
- **New lockfile subsystem** (reusable for future `upgrade` command)
- **OCI module enhancements** (tag enumeration)
- **Semver dependency addition**

**Recommended Priority**: High - This is user-facing functionality explicitly required for CLI parity with the upstream TypeScript implementation and is a prerequisite for the `upgrade` command.

**Next Steps**:
1. Add `semver` crate to workspace dependencies
2. Implement lockfile module with basic I/O
3. Extend OCI client with tag listing
4. Create outdated command module with stub implementation
5. Iterate through phases with continuous testing

---

## Appendix A: Specification References

- **Main Spec**: `/docs/subcommand-specs/outdated/SPEC.md`
- **Data Structures**: `/docs/subcommand-specs/outdated/DATA-STRUCTURES.md`
- **Diagrams**: `/docs/subcommand-specs/outdated/DIAGRAMS.md`
- **README**: `/docs/subcommand-specs/outdated/README.md`

## Appendix B: Code Location Plan

```
crates/
├── core/
│   └── src/
│       ├── lockfile.rs        # NEW: Lockfile I/O and data structures
│       ├── oci.rs            # EXTEND: Tag listing, semver filtering
│       └── version.rs        # NEW: Version resolution utilities
└── deacon/
    └── src/
        ├── cli.rs            # EXTEND: Add Outdated variant
        └── commands/
            └── outdated.rs    # NEW: Command implementation
```

## Appendix C: External Resources

- **Semver Crate**: https://crates.io/crates/semver
- **OCI Distribution Spec**: https://github.com/opencontainers/distribution-spec
- **DevContainer Spec**: https://containers.dev/implementors/spec/
