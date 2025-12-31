---
subcommand: set-up
type: enhancement
priority: high
scope: medium
labels: ["subcommand: set-up", "type: enhancement", "priority: high", "area: core"]
---

# [set-up] Implement core data structures and result schema

## Issue Type
- [ ] Missing CLI Flags
- [x] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [x] Testing & Validation
- [ ] Other: 

## Description
Introduce the core types required to flow data through the set-up workflow: normalized options, container properties for execution, merged config shapes, and the stdout JSON result. These types are prerequisites for all subsequent implementation tasks.

## Specification Reference

**From SPEC.md Section:** §3 Input Processing Pipeline, §4 Configuration Resolution, §5 Core Execution Logic, §10 Output Specifications

**From GAP.md Section:** 1.3 Data Structures (100% Missing)

### Expected Behavior
- Provide Rust structs matching `SetUpOptions`, `ParsedInput`, `DotfilesConfiguration`.
- Define `ContainerProperties` with fields for user, env, shell, folders, and exec functions.
- Define `CommonMergedDevContainerConfig` and `LifecycleHooksInstallMap` as per data shapes.
- Define result enums/structs to print `{ outcome: "success"|"error", ... }` to stdout.

### Current Behavior
- Partial types exist elsewhere, but not aligned to set-up needs; no result schema type.

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/core/src/setup/types.rs` – New module for set-up types (place in core for reuse).
- `crates/core/src/lib.rs` – Export new types module.
- `crates/core/src/variable.rs` – Add placeholder types for container-side substitution inputs if helpful (optional for this issue).

#### Specific Tasks
- [ ] Add `SetUpOptions`, `ParsedInput`, `DotfilesConfiguration`.
- [ ] Add `ContainerProperties` with fields listed in DATA-STRUCTURES.md.
- [ ] Add `CommonMergedDevContainerConfig`, `LifecycleHooksInstallMap` type aliases/structs (reuse existing config types where possible to avoid duplication).
- [ ] Add `SetUpResult` enum with variants `Success { configuration: Option<Value>, merged_configuration: Option<Value> }` and `Error { message: String, description: String }` and helper to serialize as one-line JSON.

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
#[derive(Debug, Clone)]
pub struct SetUpOptions { /* see DATA-STRUCTURES.md mapping */ }

#[derive(Debug, Clone)]
pub struct ParsedInput { /* see DATA-STRUCTURES.md mapping */ }

#[derive(Debug, Clone)]
pub struct DotfilesConfiguration {
    pub repository: Option<String>,
    pub install_command: Option<String>,
    pub target_path: String,
}

#[derive(Debug, Clone)]
pub struct ContainerProperties { /* createdAt, startedAt, osRelease, user, gid, env, shell, homeFolder, userDataFolder, remoteWorkspaceFolder, remoteExec, remotePtyExec, remoteExecAsRoot, shellServer */ }

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum SetUpResult { /* success|error with fields */ }
```

### 3. Validation Rules
- [ ] Ensure result serialization follows Theme 1: exactly one JSON line to stdout.

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [x] Theme 1 - JSON Output Contract
- [x] Theme 6 - Error Messages (types carry message/description; callers ensure exact strings)

## Testing Requirements

### Unit Tests
- [ ] Round-trip serde tests for `SetUpResult` to ensure correct shape and tag casing.
- [ ] Defaulting and builder tests for `DotfilesConfiguration`.

### Integration Tests
- [ ] None yet; wiring added in later issues will use these types.

### Smoke Tests
- [ ] Not applicable for this issue.

### Examples
- [ ] Add a doc comment example showing success and error JSON serialization.

## Acceptance Criteria
- [ ] Types compile and are exported from core.
- [ ] Serialization matches DATA-STRUCTURES.md shapes.
- [ ] CI checks pass (build, tests, fmt, clippy) with no warnings.

## Implementation Notes
- Prefer BTreeMap for deterministic serialization of maps in tests.
- Keep exec function fields as trait objects or function pointers placeholders; concrete implementations arrive later.

### Edge Cases to Handle
- Ensure optional fields are omitted from JSON when None (serde skip).

## Definition of Done
- [ ] All types added and documented with rustdoc.
- [ ] Unit tests for JSON shapes added and passing.

## References
- Specification: `docs/subcommand-specs/set-up/SPEC.md` (§3, §4, §5, §10)
- Gap Analysis: `docs/subcommand-specs/set-up/GAP.md` (§1.3)
- Data Structures: `docs/subcommand-specs/set-up/DATA-STRUCTURES.md`
