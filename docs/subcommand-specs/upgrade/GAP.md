# Upgrade Subcommand — Implementation Gap Analysis

**Report Date:** October 13, 2025  
**Specification Version:** Based on `/workspaces/deacon/docs/subcommand-specs/upgrade/` documents  
**Current Implementation:** Deacon v0.x (main branch)

---

## Executive Summary

The `upgrade` subcommand is **completely unimplemented** in the current codebase. This report documents all missing components and provides a roadmap for implementation based on the authoritative specification documents.

**Status:** ❌ **NOT IMPLEMENTED**

---

## 1. Command-Line Interface — Missing Implementation

### 1.1 Subcommand Registration
**Status:** ❌ Missing

**Required:** Add `Upgrade` variant to `Commands` enum in `crates/deacon/src/cli.rs`

**Specification Requirements:**
```rust
// Required enum variant based on SPEC.md Section 2
Upgrade {
    // Required flags
    #[arg(long)]
    dry_run: bool,
    
    // Hidden flags for Dependabot integration
    #[arg(long, short = 'f', hide = true)]
    feature: Option<String>,
    
    #[arg(long, short = 'v', hide = true)]
    target_version: Option<String>,
}
```

**Global flags (already available):**
- ✅ `--workspace-folder <PATH>` (required)
- ✅ `--config <PATH>` (optional)
- ✅ `--log-level <LEVEL>` (optional)
- ⚠️ `--docker-path <PATH>` and `--docker-compose-path <PATH>` not exposed globally

### 1.2 Argument Validation
**Status:** ❌ Missing

**Required:** Implement validation rules per SPEC.md Section 2:
- Mutual constraint: `--feature` and `--target-version` must be used together or neither
- Format validation: `--target-version` must match regex `^\d+(\.\d+(\.\d+)?)?$`
- Error messages must match specification exactly:
  - "The '--target-version' and '--feature' flag must be used together."
  - "Invalid version '<value>'. Must be in the form of 'x', 'x.y', or 'x.y.z'"

---

## 2. Core Execution Logic — Missing Implementation

### 2.1 Command Handler
**Status:** ❌ Missing

**Required:** Create `crates/deacon/src/commands/upgrade.rs` with:
- `execute_upgrade()` function
- `UpgradeArgs` struct
- Integration with global CLI context

**Architecture:** Should follow the pattern established by existing commands (build, up, exec) per SPEC.md Section 5.

### 2.2 Configuration Resolution
**Status:** ⚠️ Partially Available (needs integration)

**Available:** Configuration loading exists in `deacon_core::config`

**Missing:**
- Integration specific to upgrade workflow
- Re-loading config after feature version pinning edit
- Auto-discovery with fallback as per SPEC.md Section 4

### 2.3 Feature Version Pinning
**Status:** ❌ Missing

**Required:** Implement config file editing functionality per SPEC.md Section 5 Phase 2:
```pseudocode
FUNCTION update_feature_version_in_config(
    config: DevContainerConfig,
    config_path: string,
    feature_id: string,
    target_version: string
) -> Result<()>
```

**Behavior:**
- Text-based replacement (as per Design Decision in SPEC.md Appendix)
- Match by base identifier (strip `:tag` or `@digest`)
- Update first matching key only
- Handle case where feature is not present (log message, no error)

### 2.4 Feature Resolution
**Status:** ⚠️ Partially Available (needs adaptation)

**Available:** Feature resolution exists in `deacon_core::features`

**Missing:**
- Lockfile generation workflow integration
- Digest computation for lockfile entries
- Feature dependency resolution specific to upgrade context

### 2.5 Lockfile Generation
**Status:** ❌ Missing

**Required:** Core lockfile functionality per DATA-STRUCTURES.md:
```rust
struct Lockfile {
    features: HashMap<String, LockfileFeature>,
}

struct LockfileFeature {
    version: String,      // e.g., "2.11.1"
    resolved: String,     // e.g., "registry/path@sha256:..."
    integrity: String,    // sha256 digest
}
```

**Functions needed:**
- `generate_lockfile(features_cfg: FeaturesConfig) -> Lockfile`
- `write_lockfile(path: PathBuf, lockfile: Lockfile, force_init: bool) -> Result<()>`
- `get_lockfile_path(config_path: &Path) -> PathBuf` (naming rule implementation)

### 2.6 Lockfile Path Derivation
**Status:** ❌ Missing

**Required:** Implement naming rule per SPEC.md Section 6 and Design Decision:
- If config basename starts with `.` → `.devcontainer-lock.json`
- Otherwise → `devcontainer-lock.json`
- Write to same directory as config file

---

## 3. Data Structures — Missing Implementation

### 3.1 ParsedInput Struct
**Status:** ❌ Missing

**Required:** Create struct matching DATA-STRUCTURES.md:
```rust
struct UpgradeArgs {
    workspace_folder: Option<PathBuf>,
    config_file: Option<PathBuf>,
    docker_path: String,              // default "docker"
    docker_compose_path: String,       // default "docker-compose"
    log_level: LogLevel,
    dry_run: bool,
    feature: Option<String>,           // hidden
    target_version: Option<String>,    // hidden
}
```

### 3.2 Lockfile Structures
**Status:** ❌ Missing

**Required:** Implement complete lockfile data model per DATA-STRUCTURES.md with:
- Serialization/deserialization (serde)
- Sorted key ordering for deterministic output
- Pretty-printing with 2-space indentation

### 3.3 Feature Identifier Helpers
**Status:** ❌ Missing

**Required:** Utility functions per DATA-STRUCTURES.md:
```rust
fn get_feature_id_without_version(id: &str) -> &str;
fn get_lockfile_path(config_or_path: &Path) -> PathBuf;
```

---

## 4. External System Interactions — Missing Implementation

### 4.1 OCI Registry Integration
**Status:** ⚠️ Partially Available (needs lockfile context)

**Available:** OCI registry client exists in `deacon_core::oci`

**Missing:**
- Manifest/tag fetching for version resolution
- Digest computation for lockfile integrity fields
- Integration with lockfile generation workflow

### 4.2 Filesystem Operations
**Status:** ⚠️ Partially Available (needs specific operations)

**Available:** File I/O utilities exist

**Missing:**
- Atomic lockfile write with truncation (per SPEC.md Section 6)
- Config file in-place editing for feature pinning
- Permission error handling with user-friendly messages

---

## 5. Output Specifications — Missing Implementation

### 5.1 Dry-Run Mode
**Status:** ❌ Missing

**Required:** Per SPEC.md Section 10:
- Print lockfile JSON to stdout with 2-space indentation
- No filesystem writes to lockfile (but config edits still apply)
- Zero output to stdout when not in dry-run

### 5.2 Progress Logging
**Status:** ⚠️ Framework Available (needs messages)

**Available:** Logging framework exists via `tracing`

**Missing:**
- Upgrade-specific log messages:
  - "Updating '<feature>' to '<target_version>' in devcontainer.json"
  - "No Features found in '<path>'" (when pinning target not present)
  - Feature resolution progress
  - Lockfile write confirmation

### 5.3 Error Messages
**Status:** ❌ Missing

**Required:** Implement error cases per SPEC.md Section 9:
- "Dev container config (...) not found."
- "... must contain a JSON object literal."
- "Failed to update lockfile" (with underlying cause)
- CLI validation errors (argument pairing, format)

### 5.4 Exit Codes
**Status:** ⚠️ Framework Available

**Available:** Standard exit code handling exists

**Required:** Ensure upgrade returns:
- `0` for success
- `1` for all error cases

---

## 6. Testing — Missing Implementation

### 6.1 Test Suite
**Status:** ❌ Missing

**Required:** Comprehensive test suite per SPEC.md Section 15:

#### Unit Tests Needed:
- [ ] Argument validation (pairing constraint)
- [ ] Version format regex validation
- [ ] Lockfile path derivation (dotfile naming rule)
- [ ] Feature ID base identifier extraction
- [ ] Lockfile serialization (sorted keys, formatting)

#### Integration Tests Needed:
- [ ] Basic upgrade: outdated lockfile → upgraded lockfile
- [ ] Dry-run: print JSON without writing file
- [ ] Feature pinning: `--feature` + `--target-version` + dry-run
- [ ] Config not found error
- [ ] Invalid target-version format error
- [ ] Missing pairing (feature without version or vice versa)
- [ ] Empty features config (no-op lockfile generation)

#### Test Fixtures Needed:
- Sample devcontainer configs with features
- Outdated lockfiles
- Expected upgraded lockfiles
- Configs with various feature identifier formats (with/without tags/digests)

### 6.2 Examples
**Status:** ❌ Missing

**Required:** Add examples to `examples/` directory:
- `examples/upgrade/basic/` — Simple upgrade workflow
- `examples/upgrade/dry-run/` — Using `--dry-run` flag
- `examples/upgrade/pin-feature/` — Using hidden flags for feature pinning
- Update `examples/README.md` with upgrade section

---

## 7. Documentation — Missing Implementation

### 7.1 User Documentation
**Status:** ❌ Missing

**Required:**
- [ ] README.md section on upgrade subcommand
- [ ] CONTRIBUTING.md guidance for lockfile workflows
- [ ] examples/upgrade/README.md with usage patterns

### 7.2 Code Documentation
**Status:** ❌ Missing

**Required:**
- [ ] Rustdoc for all public functions and structs
- [ ] Module-level documentation for `commands/upgrade.rs`
- [ ] Inline comments for complex logic (feature pinning, lockfile generation)

---

## 8. Cross-Platform Considerations — Status Unknown

### 8.1 Path Handling
**Status:** ⚠️ Needs Verification

**Required:** Ensure path normalization works per SPEC.md Section 13:
- Linux: POSIX paths
- macOS: POSIX paths
- Windows: Win path normalization
- WSL2: Host path normalization

**Note:** Existing path handling in other commands should be reviewed for consistency.

---

## 9. Edge Cases — Unhandled

Per SPEC.md Section 14, the following edge cases must be handled:

- [ ] No `features` in config → empty lockfile generation succeeds
- [ ] `--dry-run` with `--feature/--target-version` → config edited, lockfile only printed
- [ ] Multiple matching feature keys (different tags/digests) → only first updated
- [ ] Feature key appears elsewhere in file (e.g., in string values) → text replacement risk documented
- [ ] Lockfile path changes when config file is renamed/moved
- [ ] Config file without write permissions (pinning fails gracefully)

---

## 10. Design Decisions — Not Yet Implemented

The following design decisions from SPEC.md Appendix require careful implementation:

### 10.1 Hidden Flags Edit Config in Dry-Run
**Decision:** Config edits persist even in `--dry-run` mode; only lockfile persistence is skipped.

**Implementation Note:** This must be explicitly documented in help text and error messages to avoid user confusion.

### 10.2 Feature Key Matching Ignores Tag/Digest
**Decision:** Match by base identifier up to last `:` or `@`.

**Implementation Note:** Required utility function `get_feature_id_without_version()` per DATA-STRUCTURES.md.

### 10.3 Text-Based Key Replacement
**Decision:** Simple text replacement in config file (not JSON AST rewrite).

**Trade-offs Acknowledged:**
- Risk of unintended replacements in other string contexts
- Accepted for simplicity and robustness
- Future refinement could use JSON AST if needed

**Implementation Note:** Should log clear warning if replacement count != 1.

### 10.4 Lockfile Naming Rule
**Decision:** Dotfile-aware naming convention.

**Implementation Note:** Must maintain compatibility with upstream CLI behavior.

---

## 11. Dependencies — Assessment

### 11.1 External Crates Needed
Evaluate if additional dependencies are required:

- JSON pretty-printing (likely covered by `serde_json`)
- File atomic writes (evaluate `tempfile` or similar)
- Regex for version format validation (likely `regex` crate)

### 11.2 Internal Dependencies
Required integration points:

- ✅ `deacon_core::config` — Config loading
- ⚠️ `deacon_core::features` — Feature resolution (needs lockfile adaptation)
- ⚠️ `deacon_core::oci` — Registry interactions (needs lockfile context)
- ✅ `deacon_core::logging` — Tracing integration
- ✅ `deacon_core::errors` — Error types

---

## 12. Priority Implementation Roadmap

### Phase 1: Foundation (High Priority)
1. **CLI Integration**
   - Add `Upgrade` variant to `Commands` enum
   - Implement argument parsing and validation
   - Add command dispatch in `cli.rs`

2. **Data Structures**
   - Define `Lockfile`, `LockfileFeature` structs
   - Implement serialization with sorted keys
   - Create `UpgradeArgs` struct

3. **Command Handler Skeleton**
   - Create `commands/upgrade.rs` module
   - Implement basic `execute_upgrade()` function
   - Wire up error handling

### Phase 2: Core Functionality (High Priority)
4. **Lockfile Generation**
   - Implement `generate_lockfile()` function
   - Add lockfile path derivation with naming rule
   - Implement `write_lockfile()` with truncation logic

5. **Feature Resolution Integration**
   - Integrate with existing feature resolver
   - Add digest computation
   - Map resolved features to lockfile entries

6. **Dry-Run Mode**
   - Implement stdout JSON output
   - Skip lockfile persistence
   - Maintain config edit behavior

### Phase 3: Advanced Features (Medium Priority)
7. **Feature Pinning (Hidden Flags)**
   - Implement `update_feature_version_in_config()`
   - Add feature ID base extraction utility
   - Handle config re-loading after edit

8. **Error Handling**
   - Map all error cases per specification
   - Implement user-friendly error messages
   - Add validation error messages

### Phase 4: Quality Assurance (Medium Priority)
9. **Testing**
   - Unit tests for data structures and utilities
   - Integration tests for full workflows
   - Edge case coverage

10. **Examples and Documentation**
    - Create example directories
    - Write user-facing documentation
    - Add rustdoc comments

### Phase 5: Polish (Low Priority)
11. **Cross-Platform Testing**
    - Verify path handling on all platforms
    - Test with various config locations
    - Validate with symbolic links

12. **Performance Optimization**
    - Profile feature resolution
    - Optimize registry requests
    - Consider caching strategies

---

## 13. Risk Assessment

### High Risk
- **Feature resolution complexity:** Existing feature resolver may need significant adaptation for lockfile workflow
- **OCI registry integration:** Digest computation and manifest handling may reveal gaps in current OCI client
- **Text-based config editing:** Risk of unintended replacements documented but mitigation needed

### Medium Risk
- **Cross-platform path handling:** Need to verify Windows and WSL2 behavior
- **Error message compatibility:** Must match specification exactly for automation compatibility
- **Lockfile format stability:** Format must remain stable once released (semver considerations)

### Low Risk
- **CLI argument parsing:** Well-established pattern in existing commands
- **Dry-run implementation:** Straightforward control flow
- **Logging integration:** Framework already in place

---

## 14. Breaking Changes and Migration

### For Users
- **New subcommand:** No breaking changes to existing commands
- **Lockfile introduction:** Opt-in; existing workflows continue to work
- **Hidden flags:** Automation-oriented; not breaking user-facing behavior

### For Developers
- **Core API changes:** May require additions to `deacon_core::features` public API
- **OCI client enhancements:** May expose new methods for digest/manifest operations
- **Config module updates:** May need helpers for in-place editing

---

## 15. Compatibility Notes

### Upstream CLI Compatibility
The specification explicitly references compatibility with the upstream DevContainer CLI implementation:

- Lockfile format must match upstream schema
- Naming conventions (lockfile path derivation) must align
- Error messages should be similar for automation script compatibility

**Recommendation:** Cross-reference with upstream CLI implementation during development.

---

## 16. Open Questions for Implementation

1. **Feature resolution caching:** Should upgrade leverage existing feature cache, or require fresh resolution?
2. **Lockfile diff:** Should upgrade provide before/after comparison in verbose mode?
3. **Force flag:** Should there be a `--force` flag to regenerate lockfile even if unchanged?
4. **Registry authentication:** How should upgrade handle auth failures during resolution?
5. **Parallel resolution:** Should feature resolution leverage parallelization as mentioned in SPEC.md Section 11?
6. **Lockfile validation:** Should upgrade validate existing lockfile before overwriting?
7. **Config backup:** Should upgrade create backup of config before editing (pinning mode)?

---

## 17. Acceptance Criteria

The upgrade subcommand implementation will be considered complete when:

### Functional Requirements
- ✅ All CLI flags from specification are implemented and functional
- ✅ Argument validation enforces all rules from specification
- ✅ Lockfile generation produces valid, deterministic JSON output
- ✅ Dry-run mode works as specified (stdout only, config edits persist)
- ✅ Feature pinning (hidden flags) updates config correctly
- ✅ Lockfile path derivation follows naming rule
- ✅ Error messages match specification exactly

### Quality Requirements
- ✅ All test cases from SPEC.md Section 15 pass
- ✅ Edge cases from SPEC.md Section 14 are handled
- ✅ Cross-platform tests pass on Linux, macOS, Windows
- ✅ Integration tests with real OCI registries pass
- ✅ Code coverage ≥ 80% for new upgrade module

### Documentation Requirements
- ✅ Rustdoc complete for all public APIs
- ✅ User guide section added to README
- ✅ Examples created and documented
- ✅ CONTRIBUTING.md updated with lockfile workflow guidance

### Performance Requirements
- ✅ Upgrade completes in comparable time to feature resolution (no significant overhead)
- ✅ Registry requests are minimized (leverage caching)
- ✅ Lockfile writes are atomic (no corrupted state on interrupt)

---

## 18. Conclusion

The `upgrade` subcommand is a complete greenfield implementation requiring:

- **~800-1200 lines of new Rust code** (estimated)
- **~400-600 lines of test code** (estimated)
- **~5-8 new data structures**
- **~15-20 public functions**
- **~6-10 integration tests**
- **~3-5 example projects**

**Estimated Effort:** 2-3 weeks for experienced Rust developer, including testing and documentation.

**Recommended Approach:** Follow the phased roadmap in Section 12, with continuous testing and documentation alongside implementation.

**Critical Dependencies:**
1. Stable feature resolution API in `deacon_core::features`
2. OCI client support for digest computation in `deacon_core::oci`
3. Decision on lockfile schema versioning strategy

**Next Steps:**
1. Review and validate this gap analysis with project stakeholders
2. Create tracking issues for each phase of the roadmap
3. Assign priorities and timelines
4. Begin Phase 1 implementation

---

## Appendix A: Specification Document Coverage

This report covers the following authoritative specification documents:

- ✅ `/workspaces/deacon/docs/subcommand-specs/upgrade/SPEC.md` (complete)
- ✅ `/workspaces/deacon/docs/subcommand-specs/upgrade/DATA-STRUCTURES.md` (complete)
- ✅ `/workspaces/deacon/docs/subcommand-specs/upgrade/DIAGRAMS.md` (complete)

All sections of each document have been analyzed and cross-referenced against the current implementation state.

---

## Appendix B: Quick Reference Checklist

### Subcommand Registration
- [ ] Add `Upgrade` variant to `Commands` enum
- [ ] Add `--dry-run` flag
- [ ] Add `--feature` flag (hidden)
- [ ] Add `--target-version` flag (hidden)

### Argument Validation
- [ ] Implement pairing constraint validation
- [ ] Implement version format regex validation
- [ ] Add custom error messages

### Core Logic
- [ ] Create `commands/upgrade.rs` module
- [ ] Implement `execute_upgrade()` function
- [ ] Implement `generate_lockfile()` function
- [ ] Implement `write_lockfile()` function
- [ ] Implement `update_feature_version_in_config()` function
- [ ] Implement `get_lockfile_path()` utility

### Data Structures
- [ ] Define `Lockfile` struct with serde
- [ ] Define `LockfileFeature` struct with serde
- [ ] Define `UpgradeArgs` struct
- [ ] Implement sorted key serialization

### Integration Points
- [ ] Integrate with config loader
- [ ] Integrate with feature resolver
- [ ] Integrate with OCI client (digests)
- [ ] Integrate with logging framework

### Testing
- [ ] Unit tests for utilities
- [ ] Unit tests for data structures
- [ ] Integration test: basic upgrade
- [ ] Integration test: dry-run mode
- [ ] Integration test: feature pinning
- [ ] Integration test: error cases

### Documentation
- [ ] Rustdoc for public APIs
- [ ] User guide section
- [ ] Example projects
- [ ] Update CONTRIBUTING.md

### Examples
- [ ] `examples/upgrade/basic/`
- [ ] `examples/upgrade/dry-run/`
- [ ] `examples/upgrade/pin-feature/`
- [ ] Update `examples/README.md`

---

**End of Report**
