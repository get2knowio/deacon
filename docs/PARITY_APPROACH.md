# DevContainer CLI Parity Approach

**Date:** October 13, 2025  
**Purpose:** Strategy for achieving full parity with the official DevContainer CLI specification

---

## Overview

This document outlines the systematic approach for implementing missing functionality across all subcommands. Based on comprehensive gap analysis of 14 subcommands, we've identified cross-cutting concerns that must be addressed to ensure consistent, efficient implementation.

## Implementation Strategy

### Two-Phase Approach

1. **Phase 0: Foundation** - Build critical infrastructure that multiple subcommands depend on
2. **Phase 1-N: Subcommand Implementation** - Tackle individual subcommands while applying consistent patterns

---

## Phase 0: Critical Foundation (Must Complete First)

These four infrastructure pieces block or affect 6+ subcommands each. Building them first prevents duplication and ensures consistency.

### 1. OCI Registry Infrastructure Enhancement 🔴 HIGHEST PRIORITY

**Affects:** `features-info`, `features-publish`, `features-package`, `templates`, `build`, `outdated` (6 subcommands)

**Current State:** Basic OCI client exists (`crates/core/src/oci.rs`) but missing critical operations

**Required Capabilities:**
- **Tag Listing API**: Implement OCI Distribution Spec `/v2/<name>/tags/list` endpoint
- **Manifest Fetching by Digest**: Get manifests with specific digest references
- **Semantic Version Operations**:
  - Tag computation: `[major, major.minor, major.minor.patch, latest]`
  - Semver filtering (exclude non-semver tags)
  - Semver sorting (descending)
  - Version comparison utilities
- **Collection Metadata Publishing**: Support for `devcontainer-collection.json`
- **Multi-Tag Publishing**: Publish multiple tags in single operation with idempotency checks

**Deliverables:**
- `OciClient::list_tags()` method
- `OciClient::get_manifest_by_digest()` method
- `semver` utilities module in `crates/core/src/` or leverage existing crate
- Collection metadata structures and publishing logic
- Integration tests for all new OCI operations

**Dependencies:** None (foundational)

**Estimated Impact:** Unblocks 6 subcommands, prevents reimplementation 6+ times

---

### 2. Lockfile Data Structures & I/O 🔴 CRITICAL

**Affects:** `outdated`, `upgrade` (both 100% blocked without this)

**Current State:** No lockfile support exists

**Required Components:**

#### Data Structures
```rust
// crates/core/src/lockfile.rs (new module)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    pub features: HashMap<String, LockfileFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileFeature {
    pub version: String,        // e.g., "2.11.1"
    pub resolved: String,       // Full OCI reference with digest
    pub integrity: String,      // sha256 digest
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}
```

#### Path Derivation Rules
- If config basename starts with `.` → `.devcontainer-lock.json`
- Otherwise → `devcontainer-lock.json`
- Write to same directory as config file

#### Operations
- `read_lockfile(path: &Path) -> Result<Option<Lockfile>>`
- `write_lockfile(path: &Path, lockfile: &Lockfile, force_init: bool) -> Result<()>`
- `get_lockfile_path(config_path: &Path) -> PathBuf`
- `merge_lockfile_features(existing: &Lockfile, new: &Lockfile) -> Lockfile`

**Deliverables:**
- New module `crates/core/src/lockfile.rs`
- Unit tests for path derivation logic
- Integration tests for read/write operations
- Documentation in module header

**Dependencies:** None (foundational)

**Estimated Impact:** Completely unblocks `outdated` and `upgrade` subcommands

---

### 3. Global CLI Flag Consolidation 🔴 CRITICAL

**Affects:** ALL subcommands (14 total)

**Current Issue:** Inconsistent flag placement causes duplication and user confusion

**Required Changes:**

#### Move to Global Flags (in `crates/deacon/src/cli.rs`)
```rust
#[command(name = "deacon")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    // Already global ✅
    #[arg(long, global = true)]
    pub workspace_folder: Option<PathBuf>,
    
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    
    // ... existing global flags ...

    // ADD THESE:
    /// Path to docker executable
    #[arg(long, global = true, default_value = "docker")]
    pub docker_path: String,

    /// Path to docker-compose executable  
    #[arg(long, global = true, default_value = "docker-compose")]
    pub docker_compose_path: String,

    /// Terminal columns (requires --terminal-rows)
    #[arg(long, global = true, requires = "terminal_rows")]
    pub terminal_columns: Option<u32>,

    /// Terminal rows (requires --terminal-columns)
    #[arg(long, global = true, requires = "terminal_columns")]
    pub terminal_rows: Option<u32>,
}
```

#### Validation
- Enforce paired terminal dimensions (both or neither)
- Update all subcommands to remove duplicate local flags
- Update documentation and examples

**Deliverables:**
- Updated `cli.rs` with consolidated global flags
- Remove duplicate flags from all subcommand definitions
- Update smoke tests to use global flags
- Update `EXAMPLES.md` and `README.md`

**Dependencies:** None (foundational)

**Estimated Impact:** Prevents implementing same flags 10+ different ways, improves UX consistency

---

### 4. Container Selection & Inspection Utilities 🔴 CRITICAL

**Affects:** `exec`, `read-configuration`, `run-user-commands`, `set-up` (4 major subcommands)

**Current State:** No container selection logic exists

**Required Components:**

#### CLI Flags (add to relevant subcommands)
```rust
/// Target container ID directly
#[arg(long)]
pub container_id: Option<String>,

/// Locate container by label (repeatable, format: name=value)
#[arg(long = "id-label")]
pub id_labels: Vec<String>,
```

#### Validation Logic
```rust
// crates/core/src/container.rs (extend existing)

pub struct ContainerSelector {
    pub container_id: Option<String>,
    pub id_labels: Vec<(String, String)>,
    pub workspace_folder: Option<PathBuf>,
}

impl ContainerSelector {
    /// Validate that at least one selector is provided
    pub fn validate(&self) -> Result<()> {
        if self.container_id.is_none() 
            && self.id_labels.is_empty() 
            && self.workspace_folder.is_none() {
            bail!("At least one of --container-id, --id-label, or --workspace-folder is required");
        }
        Ok(())
    }

    /// Validate id-label format (must be name=value with non-empty parts)
    pub fn validate_labels(labels: &[String]) -> Result<Vec<(String, String)>> {
        let regex = Regex::new(r"^.+=.+$")?;
        labels.iter()
            .map(|label| {
                if !regex.is_match(label) {
                    bail!("Invalid label format '{}'. Must be name=value", label);
                }
                let parts: Vec<&str> = label.splitn(2, '=').collect();
                Ok((parts[0].to_string(), parts[1].to_string()))
            })
            .collect()
    }
}
```

#### Container Lookup Operations
```rust
/// Find container by ID (exact match)
pub async fn find_container_by_id(
    docker: &Docker,
    container_id: &str,
) -> Result<Option<ContainerInfo>>;

/// Find container(s) by label filters
pub async fn find_containers_by_labels(
    docker: &Docker,
    labels: &[(String, String)],
) -> Result<Vec<ContainerInfo>>;

/// Inspect container and return metadata
pub async fn inspect_container(
    docker: &Docker,
    container_id: &str,
) -> Result<ContainerInspectResponse>;
```

**Deliverables:**
- `ContainerSelector` struct with validation
- Container lookup utilities in `crates/core/src/container.rs`
- Label format validation with regex
- Unit tests for validation logic
- Integration tests for container lookup (may require Docker)

**Dependencies:** None (foundational)

**Estimated Impact:** Unblocks 4 major subcommands with consistent container selection

---

## Phase 1-N: Subcommand Implementation

After Phase 0 is complete, tackle individual subcommands in dependency order. For each subcommand, ensure:

### Infrastructure to Build During Implementation

#### 5. Environment Probing System with Caching
- **Build When:** Implementing `run-user-commands` (first user-facing need)
- **Affects:** `exec`, `run-user-commands`, `set-up`
- **Components:**
  - `userEnvProbe` enum (none/loginInteractiveShell/interactiveShell/loginShell)
  - Shell command execution to probe environment
  - Result caching in `--container-session-data-folder`
  - Integration with variable substitution (`${containerEnv:VAR}`)

#### 6. Dotfiles Installation Workflow
- **Build When:** Implementing `run-user-commands`
- **Affects:** `run-user-commands`, `set-up`
- **Components:**
  - Git clone from `--dotfiles-repository`
  - Execute `--dotfiles-install-command` or `install.sh`
  - Target path handling (`--dotfiles-target-path`, default `~/dotfiles`)
  - Marker file idempotency (`.dotfilesMarker`)

#### 7. Secrets Management & Log Redaction
- **Build When:** Implementing `run-user-commands` or `set-up`
- **Affects:** Multiple subcommands
- **Components:**
  - Environment variable injection with `${secret:KEY}` substitution
  - Log output redaction for secret values
  - Trace/span field sanitization
- **Note:** Coordinate with existing Issue #41 placeholder

#### 8. Two-Phase Variable Substitution
- **Build When:** Extending `read-configuration` or `run-user-commands`
- **Affects:** `read-configuration`, `run-user-commands`, `set-up`, `up`, `build`
- **Components:**
  - Pre-container phase (local env, config values)
  - Post-container phase (`${containerEnv:VAR}` using probed environment)
  - Proper ordering and re-application

---

## Consistency Themes (Apply to ALL Subcommands)

### Theme 1: JSON Output Contract Compliance
For every subcommand with `--json` or `--output-format json`:
- ✅ JSON output goes to **stdout** (not stderr)
- ✅ Human-readable logs go to **stderr**
- ✅ Structure matches `DATA-STRUCTURES.md` exactly
- ✅ Error cases return `{}` or specified error shape with proper exit codes
- ✅ No trailing newlines or extra whitespace in JSON

### Theme 2: CLI Validation Rules
For every subcommand:
- ✅ Implement mutual exclusivity rules (clap `conflicts_with`)
- ✅ Implement paired requirement rules (clap `requires`)
- ✅ Regex format validation where specified (e.g., `--id-label` must match `/.+=.+/`)
- ✅ Use **exact error messages** from specification
- ✅ Validate before any expensive operations

### Theme 3: Collection Mode vs Single Mode
For `features-package`, `features-publish`, `templates`:
- ✅ Detection logic: if `src/` directory exists → collection mode
- ✅ Generate `devcontainer-collection.json` in both modes
- ✅ Consistent metadata structure across features and templates
- ✅ Proper iteration over collection items

### Theme 4: Semantic Versioning Operations
For `features-publish`, `templates`, `outdated`, `upgrade`:
- ✅ Use consistent semver parsing (regex `^\d+(\.\d+(\.\d+)?)?$`)
- ✅ Tag computation: `[major, major.minor, full, latest]`
- ✅ Sorting: descending semver order
- ✅ Filtering: exclude non-semver tags from registry results
- ✅ Use utilities from Phase 0, item #1

### Theme 5: Marker File Idempotency Pattern
For `run-user-commands`, `set-up`:
- ✅ Standard location: `/var/devcontainer/.{operation}Marker`
- ✅ Check existence before operation
- ✅ Write marker after successful completion
- ✅ `--prebuild` flag forces re-execution (ignores markers)
- ✅ Root-only operations for system patching

### Theme 6: Error Message Standardization
For all subcommands:
- ✅ Use exact error messages from specification where provided
- ✅ Follow format: `"Error description."` (sentence case, period)
- ✅ Include actionable information (what failed, why, how to fix)
- ✅ Use `bail!` or `anyhow::Context` for error chains
- ✅ No raw `unwrap()` or `expect()` in production code

---

## Subcommand Implementation Order (Recommended)

### Tier 1: Foundation (Unblock Others)
1. **read-configuration** - Core config resolution, needed by many others
2. **features-plan** - Feature resolution foundation (already ~75% done)

### Tier 2: Feature Lifecycle
3. **features-package** - Required before publish
4. **features-publish** - Uses Phase 0 OCI infrastructure
5. **features-info** - Uses Phase 0 OCI infrastructure
6. **features-test** - Independent, large scope (80-90% missing)

### Tier 3: Lockfile-Dependent
7. **outdated** - Uses Phase 0 lockfile infrastructure
8. **upgrade** - Uses Phase 0 lockfile infrastructure

### Tier 4: Container Lifecycle
9. **build** - Core container creation
10. **up** - Depends on build, run-user-commands
11. **run-user-commands** - Complex lifecycle execution
12. **set-up** - Similar to run-user-commands but for existing containers
13. **exec** - Simpler, depends on container selection

### Tier 5: Templates
14. **templates** - Full rewrite needed for CLI and OCI integration

---

## Quality Gates (Every Subcommand Must Pass)

Before marking any subcommand as "complete":

### Code Quality
- [ ] All CI checks pass (build, test, fmt, clippy)
- [ ] No `unsafe` code introduced
- [ ] No `unwrap()` or `expect()` in production paths
- [ ] Proper error context with `anyhow::Context`
- [ ] Tracing spans for major operations

### Specification Compliance
- [ ] All required CLI flags implemented
- [ ] All validation rules enforced
- [ ] JSON output matches DATA-STRUCTURES.md
- [ ] Error messages match specification
- [ ] Exit codes match specification

### Testing
- [ ] Unit tests for pure logic (>80% coverage)
- [ ] Integration tests for cross-module workflows
- [ ] Smoke tests updated in `crates/deacon/tests/smoke_basic.rs`
- [ ] Examples updated in `examples/` directory
- [ ] Fixtures added/updated in `fixtures/` directory

### Documentation
- [ ] Rustdoc for public functions
- [ ] Examples in `EXAMPLES.md` if user-facing
- [ ] Update gap analysis with "COMPLETED" status
- [ ] Update CLI_PARITY.md checklist

---

## Progress Tracking

### Phase 0 Status
- [ ] **Issue #1**: OCI Registry Infrastructure Enhancement
- [ ] **Issue #2**: Lockfile Data Structures & I/O
- [ ] **Issue #3**: Global CLI Flag Consolidation
- [ ] **Issue #4**: Container Selection & Inspection Utilities

### Subcommand Status
- [ ] build (40% complete, gap analysis exists)
- [ ] exec (50% complete, gap analysis exists)
- [ ] features-info (15% complete, gap analysis exists)
- [ ] features-package (30% complete, gap analysis exists)
- [ ] features-plan (75% complete, gap analysis exists)
- [ ] features-publish (40% complete, gap analysis exists)
- [ ] features-test (10% complete, gap analysis exists)
- [ ] outdated (0% complete, gap analysis exists, blocked by Phase 0)
- [ ] read-configuration (25% complete, gap analysis exists)
- [ ] run-user-commands (30% complete, gap analysis exists)
- [ ] set-up (0% complete, gap analysis exists)
- [ ] templates (40% complete, gap analysis exists)
- [ ] up (40% complete, gap analysis exists)
- [ ] upgrade (0% complete, gap analysis exists, blocked by Phase 0)

---

## References

- **Specification:** `/workspaces/deacon/docs/CLI-SPEC.md`
- **Gap Analyses:** `/workspaces/deacon/docs/subcommand-specs/*/IMPLEMENTATION_GAP_ANALYSIS.md`
- **Copilot Instructions:** `/workspaces/deacon/.github/copilot-instructions.md`
- **Contributing Guide:** `/workspaces/deacon/CONTRIBUTING.md`

---

**Last Updated:** October 13, 2025
