# Set-Up Subcommand Implementation Gap Analysis

**Report Date:** October 13, 2025  
**Specification Reference:** `docs/subcommand-specs/set-up/`  
**Current Implementation Status:** NOT IMPLEMENTED

---

## Executive Summary

The `set-up` subcommand is **completely missing** from the current implementation. This is a critical gap as `set-up` is a core subcommand of the DevContainer CLI specification that enables converting an already-running container into a Dev Container by applying configuration and lifecycle hooks at runtime.

**Priority:** HIGH - This is a fundamental subcommand expected by users and listed in the specification.

---

## 1. Missing Components

### 1.1 CLI Interface (100% Missing)

The `set-up` subcommand is not defined in `/workspaces/deacon/crates/deacon/src/cli.rs`.

**Required CLI Arguments (from SPEC.md Section 2):**

#### Required Flags
- ✗ `--container-id <string>`: Target container ID (REQUIRED)

#### Configuration Flags
- ✗ `--config <path>`: Optional devcontainer.json path

#### Logging/Terminal Flags
- ✗ `--log-level <info|debug|trace>`: Default info
- ✗ `--log-format <text|json>`: Default text
- ✗ `--terminal-columns <number>`: Implies terminal-rows
- ✗ `--terminal-rows <number>`: Implies terminal-columns

#### Lifecycle Control Flags
- ✗ `--skip-post-create`: Skip all lifecycle hooks and dotfiles
- ✗ `--skip-non-blocking-commands`: Stop after waitFor hook (default updateContent)

#### Environment/Dotfiles Flags
- ✗ `--remote-env <name=value>`: Repeatable, extra env for hooks
- ✗ `--dotfiles-repository <url or owner/repo>`
- ✗ `--dotfiles-install-command <string>`
- ✗ `--dotfiles-target-path <path>`: Default `~/dotfiles`
- ✗ `--container-session-data-folder <path>`: Cache location for probes

#### Data Folders Flags
- ✗ `--user-data-folder <path>`: Host persisted state
- ✗ `--container-data-folder <path>`: Inside-container user data (default `~/.devcontainer`)
- ✗ `--container-system-data-folder <path>`: Inside-container system data (default `/var/devcontainer`)

#### Output Shaping Flags
- ✗ `--include-configuration`: Include updated config in result
- ✗ `--include-merged-configuration`: Include merged config in result

#### Docker Path Flag
- ✗ `--docker-path <string>`: Path to Docker CLI

**Gap Assessment:**
- **0% implemented** - No CLI definition exists
- **Impact:** Users cannot invoke the command at all
- **Validation rules missing:** No validation for `--remote-env` format, terminal dims coupling, config path resolution

---

### 1.2 Core Execution Logic (100% Missing)

**Required Execution Flow (from SPEC.md Section 5):**

#### Phase 1: Initialization
- ✗ Parse and normalize CLI arguments into `SetUpOptions`
- ✗ Create `DockerParams` with all folder paths, lifecycle settings
- ✗ Validate `--remote-env` format (`name=value`)
- ✗ Validate terminal dimensions coupling (columns ↔ rows)
- ✗ Validate config path existence if provided

#### Phase 2: Container Discovery and Configuration
- ✗ Inspect container via `docker inspect <container-id>`
- ✗ Bail out if container not found: "Dev container not found."
- ✗ Read optional devcontainer.json from `--config` path
- ✗ Extract image metadata from container labels/Features
- ✗ Merge configuration: `mergeConfiguration(config.config, imageMetadata)`
- ✗ Create `ContainerProperties` with user, env, shell server

#### Phase 3: Main Execution (Inside Container)

**3a) System Patching (Once per Container)**
- ✗ Patch `/etc/environment` with container env pairs (root-only, with marker)
- ✗ Patch `/etc/profile` to preserve PATH on login shells (root-only, with marker)
- ✗ Marker files:
  - `/var/devcontainer/.patchEtcEnvironmentMarker`
  - `/var/devcontainer/.patchEtcProfileMarker`

**3b) Variable Substitution Using Live Container Env**
- ✗ Probe container environment (user shell context)
- ✗ Apply `${containerEnv:VAR}` substitution to config
- ✗ Apply `${containerEnv:VAR}` substitution to mergedConfig
- ✗ Produce `updatedConfig` and `updatedMerged`

**3c) Lifecycle Execution & Dotfiles**
- ✗ Check if `--skip-post-create` is set
- If lifecycle enabled:
  - ✗ Probe remote env (userEnvProbe + remoteEnv merge)
  - ✗ Run lifecycle hooks in order:
    - onCreate
    - updateContent
    - postCreate
    - postStart
    - postAttach
  - ✗ Honor `waitFor` and `--skip-non-blocking-commands` semantics
  - ✗ Execute with marker files to prevent re-execution:
    - `~/.devcontainer/.onCreateCommandMarker`
    - `~/.devcontainer/.updateContentCommandMarker`
    - `~/.devcontainer/.postCreateCommandMarker`
    - `~/.devcontainer/.postStartCommandMarker`
  - ✗ Install dotfiles if configured (with marker to prevent reinstall)

#### Phase 4: Post-Execution
- ✗ Return JSON result with:
  - `outcome: "success"`
  - `configuration?: object` (if `--include-configuration`)
  - `mergedConfiguration?: object` (if `--include-merged-configuration`)
- ✗ Return exit code 0 on success, 1 on error

**Gap Assessment:**
- **0% implemented** - No execution logic exists
- **Impact:** Command cannot perform its primary function
- **Critical missing infrastructure:**
  - Container inspection and property extraction
  - System file patching (`/etc/environment`, `/etc/profile`)
  - Marker file management for idempotency
  - Lifecycle hook orchestration for existing containers

---

### 1.3 Data Structures (100% Missing)

**Required Structures (from DATA-STRUCTURES.md):**

#### Input Structures
- ✗ `SetUpOptions` struct with all CLI options
- ✗ `DotfilesConfiguration` struct (partially exists but incomplete)
- ✗ `ParsedInput` struct for normalized CLI args

#### Container Properties
- ✗ `ContainerProperties` struct with:
  - `createdAt`, `startedAt`
  - `osRelease` (hardware, id, version)
  - `user`, `gid`, `env`, `shell`, `homeFolder`
  - `userDataFolder`, `remoteWorkspaceFolder`
  - `remoteExec`, `remotePtyExec`, `remoteExecAsRoot`
  - `shellServer`, `launchRootShellServer`

#### Configuration Structures
- ✗ `CommonMergedDevContainerConfig` with:
  - `entrypoints?: string[]`
  - `onCreateCommands?: LifecycleCommand[]`
  - `updateContentCommands?: LifecycleCommand[]`
  - `postCreateCommands?: LifecycleCommand[]`
  - `postStartCommands?: LifecycleCommand[]`
  - `postAttachCommands?: LifecycleCommand[]`
- ✗ `LifecycleHooksInstallMap` for tracking origins

#### Output Structures
- ✗ Success JSON: `{ outcome: "success", configuration?, mergedConfiguration? }`
- ✗ Error JSON: `{ outcome: "error", message, description }`

**Gap Assessment:**
- **Partial infrastructure** - Some related structures exist but incomplete:
  - `DotfilesConfiguration` exists but doesn't match spec exactly
  - `ContainerProbeMode` exists and matches spec
  - Missing: `ContainerProperties`, `SetUpOptions`, result structures
- **Impact:** Cannot represent required data flows

---

### 1.4 State Management (100% Missing)

**Required State Handling (from SPEC.md Section 6):**

#### Persistent State Inside Container
- ✗ User data folder: `~/.devcontainer` (or `--container-data-folder`)
- ✗ System var folder: `/var/devcontainer` (or `--container-system-data-folder`)
- ✗ Lifecycle marker files:
  - `.onCreateCommandMarker`
  - `.updateContentCommandMarker`
  - `.postCreateCommandMarker`
  - `.postStartCommandMarker`
- ✗ System patch marker files:
  - `.patchEtcEnvironmentMarker`
  - `.patchEtcProfileMarker`
- ✗ Dotfiles marker at target path

#### Cache Management
- ✗ Environment probe cache under `--container-session-data-folder`
- ✗ Invalidation on container changes

#### Idempotency
- ✗ Safe re-run logic using marker files
- ✗ Prevention of duplicate patches/hooks/dotfiles

**Gap Assessment:**
- **0% implemented** - No marker file system exists
- **Impact:** Cannot guarantee idempotency; re-runs would duplicate work

---

### 1.5 External System Interactions (Partial)

**Required Docker/Container Operations (from SPEC.md Section 7):**

#### Existing Infrastructure (Partial Match)
- ✓ `docker inspect <containerId>` - exists in core
- ✓ `docker exec` - exists in core
- ✓ PTY allocation logic - exists in core

#### Missing Operations for Set-Up
- ✗ Root exec for system patching: `docker exec -u root <id> <cmd>`
- ✗ Launch persistent shell server for user/root multiplexing
- ✗ Write to `/etc/environment` via heredoc or temp scripts
- ✗ Write to `/etc/profile` via heredoc or temp scripts
- ✗ Execute lifecycle hooks with proper user context
- ✗ Clone and run dotfiles scripts in container

**Gap Assessment:**
- **30% infrastructure exists** - Basic docker exec exists but missing:
  - Root shell server management
  - System file patching infrastructure
  - Marker file write/check operations
- **Impact:** Cannot perform privileged operations required for setup

---

### 1.6 Variable Substitution (Partial)

**Required Substitution (from SPEC.md Section 4):**

#### Container Environment Substitution
- ✗ `${containerEnv:VAR}` - resolve from live container env
- ✗ Case-insensitive on Windows-like contexts
- ✗ Two-pass substitution:
  1. Initial config read (host-side vars)
  2. Container-side substitution after env probe

#### Existing Infrastructure
- ✓ `${localEnv:VAR}` - exists in `crates/core/src/variable.rs`
- ✓ Basic substitution engine exists

#### Missing Infrastructure
- ✗ Container-side substitution pass
- ✗ Integration with container env probe
- ✗ Error handling for missing containerEnv vars

**Gap Assessment:**
- **40% infrastructure exists** - Basic substitution exists but:
  - No container-side substitution implementation
  - No integration with live container env
- **Impact:** Cannot resolve runtime container variables

---

### 1.7 Lifecycle Hook Execution (Partial)

**Required Lifecycle Orchestration (from SPEC.md Section 5):**

#### Existing Infrastructure
- ✓ `crates/core/src/container_lifecycle.rs` - lifecycle execution exists
- ✓ `crates/core/src/lifecycle.rs` - lifecycle phases defined
- ✓ Hook ordering: onCreate → updateContent → postCreate → postStart → postAttach
- ✓ `--skip-post-create` support
- ✓ `--skip-non-blocking-commands` support

#### Missing for Set-Up
- ✗ Image metadata extraction and merging
- ✗ Lifecycle command origin tracking (`LifecycleHooksInstallMap`)
- ✗ Marker file integration for idempotency
- ✗ Integration with container properties (shell server, user context)
- ✗ `waitFor` semantics with `--skip-non-blocking-commands`

**Gap Assessment:**
- **50% infrastructure exists** - Core lifecycle exists but:
  - Not integrated with container properties concept
  - No marker file system
  - No image metadata merging
- **Impact:** Cannot run lifecycle against existing container properly

---

### 1.8 Dotfiles Integration (Partial)

**Required Dotfiles Support (from SPEC.md Section 5):**

#### Existing Infrastructure
- ✓ `crates/core/src/dotfiles.rs` - dotfiles module exists
- ✓ Repository cloning via git
- ✓ Auto-detect install scripts: `install.sh`, `setup.sh`
- ✓ `DotfilesConfiguration` struct

#### Missing for Set-Up
- ✗ Container-side execution (currently host-side only)
- ✗ Marker file to prevent reinstall
- ✗ CLI flag integration:
  - `--dotfiles-repository`
  - `--dotfiles-install-command`
  - `--dotfiles-target-path`
- ✗ Integration with lifecycle execution
- ✗ Extended installer detection (spec requires: `bootstrap`, `script/*` too)

**Gap Assessment:**
- **40% infrastructure exists** - Basic dotfiles exists but:
  - Not wired to CLI
  - Not integrated with container execution
  - Missing spec-compliant installer detection
- **Impact:** Cannot install dotfiles in target container

---

### 1.9 Error Handling (Minimal)

**Required Error Cases (from SPEC.md Section 9):**

#### User Errors
- ✗ Missing `--container-id` → argument validation error
- ✗ Invalid `--remote-env` format → argument validation error
- ✗ `--config` path not found → `ContainerError` with "Dev container config (<path>) not found."

#### System Errors
- ✗ Container not found → "Dev container not found."
- ✗ Lifecycle command failure → error with message and stdout/stderr
- ✗ Root operations failing → warnings, continue with best-effort

#### Output Format
- ✗ Success stdout JSON: `{ outcome: "success", configuration?, mergedConfiguration? }`
- ✗ Error stdout JSON: `{ outcome: "error", message, description }`
- ✗ Exit code 0 on success, 1 on error

**Gap Assessment:**
- **10% infrastructure exists** - Basic error types exist but:
  - No set-up specific error handling
  - No JSON output format for set-up
  - No outcome-based result structure
- **Impact:** Cannot provide spec-compliant error reporting

---

### 1.10 Output Specifications (Missing)

**Required Output (from SPEC.md Section 10):**

#### Success Output
```json
{
  "outcome": "success",
  "configuration": { /* if --include-configuration */ },
  "mergedConfiguration": { /* if --include-merged-configuration */ }
}
```

#### Error Output
```json
{
  "outcome": "error",
  "message": "string",
  "description": "string"
}
```

#### Notes from Spec
- stdout contains single JSON line with `outcome` field
- stderr contains logs according to `--log-format`
- Exit code 0 on success, 1 on error
- Container id NOT included in result (caller already has it)

**Gap Assessment:**
- **0% implemented** - No output structure exists
- **Impact:** Cannot return results to callers

---

## 2. Existing Infrastructure (Reusable Components)

### 2.1 Can Be Reused

The following existing modules can be leveraged:

#### Docker Operations (`crates/core/src/docker.rs`)
- ✓ Container inspection
- ✓ Exec operations
- ✓ PTY allocation

#### Configuration (`crates/core/src/config.rs`)
- ✓ devcontainer.json parsing
- ✓ Configuration merging (partial)

#### Variable Substitution (`crates/core/src/variable.rs`)
- ✓ Basic substitution engine
- ✓ `${localEnv:VAR}` support

#### Lifecycle (`crates/core/src/container_lifecycle.rs`)
- ✓ Hook execution order
- ✓ Skip flags support
- ✓ Command execution in container

#### Environment Probe (`crates/core/src/container_env_probe.rs`)
- ✓ `ContainerProbeMode` enum (matches spec)
- ✓ Shell detection and probe logic
- ✓ Environment capture from container

#### Dotfiles (`crates/core/src/dotfiles.rs`)
- ✓ Repository cloning
- ✓ Auto-detection of install scripts
- ✓ Basic execution

### 2.2 Needs Extension

The following need to be extended:

1. **Docker module** - Add:
   - Root shell server management
   - System file patching helpers

2. **Config module** - Add:
   - Image metadata extraction from labels
   - Metadata merging into config

3. **Variable module** - Add:
   - Container-side substitution pass
   - `${containerEnv:VAR}` support

4. **Lifecycle module** - Add:
   - Marker file integration
   - ContainerProperties concept
   - Origin tracking for commands

5. **Dotfiles module** - Add:
   - Container-side execution
   - Marker file support
   - Extended installer detection

---

## 3. Implementation Priority Recommendations

### 3.1 Critical Path (Must Implement First)

1. **CLI Definition** [HIGH]
   - Add `SetUp` variant to `Commands` enum
   - Define all required flags per spec Section 2
   - Implement argument validation

2. **Core Data Structures** [HIGH]
   - Create `SetUpOptions` struct
   - Create `ContainerProperties` struct
   - Create result structures (success/error JSON)

3. **Container Properties** [HIGH]
   - Implement container inspection and property extraction
   - Create shell server management
   - Implement root/user exec abstraction

4. **System Patching** [HIGH]
   - Implement `/etc/environment` patcher
   - Implement `/etc/profile` patcher
   - Add marker file system

5. **Basic Execution Flow** [HIGH]
   - Implement main `execute_set_up` function
   - Wire up phases 1-4 from spec Section 5
   - Add JSON output formatting

### 3.2 Secondary Features (Implement After Core)

6. **Image Metadata Merging** [MEDIUM]
   - Extract metadata from container labels
   - Implement merge algorithm per spec
   - Create `LifecycleHooksInstallMap`

7. **Container Variable Substitution** [MEDIUM]
   - Extend variable module for `${containerEnv:VAR}`
   - Integrate with env probe
   - Two-pass substitution (host → container)

8. **Lifecycle Integration** [MEDIUM]
   - Wire lifecycle hooks to container properties
   - Add marker file support to lifecycle
   - Implement `waitFor` semantics

9. **Dotfiles for Set-Up** [MEDIUM]
   - Add CLI flags for dotfiles
   - Container-side dotfiles execution
   - Marker file integration

### 3.3 Polish & Edge Cases (Final Phase)

10. **Error Handling** [LOW]
    - Implement all error cases from spec Section 9
    - User-friendly error messages
    - Proper exit codes

11. **Terminal Dimensions** [LOW]
    - Add `--terminal-columns` and `--terminal-rows`
    - Validation for coupled flags

12. **Data Folders** [LOW]
    - Full support for all folder paths
    - Session data folder for probe cache

13. **Testing** [CRITICAL]
    - Unit tests for all components
    - Integration tests per spec Section 15
    - Example fixtures

---

## 4. Detailed Implementation Checklist

### Phase 1: Foundation (Weeks 1-2)

- [ ] CLI: Add `SetUp` command variant with all flags
- [ ] CLI: Implement argument parsing and validation
- [ ] Core: Create `SetUpOptions` struct
- [ ] Core: Create `ContainerProperties` struct
- [ ] Core: Create result JSON structures
- [ ] Core: Implement `execute_set_up` skeleton

### Phase 2: Container Operations (Weeks 2-3)

- [ ] Docker: Container inspection helper
- [ ] Docker: Create `ContainerProperties` from inspection
- [ ] Docker: Implement shell server management
- [ ] Docker: Add root exec operations
- [ ] Docker: Implement marker file read/write

### Phase 3: System Patching (Week 3)

- [ ] Core: Implement `/etc/environment` patcher
- [ ] Core: Implement `/etc/profile` patcher
- [ ] Core: Add marker file infrastructure
- [ ] Core: Idempotency tests for patches

### Phase 4: Configuration & Substitution (Week 4)

- [ ] Config: Extract image metadata from labels
- [ ] Config: Implement metadata merge algorithm
- [ ] Variable: Add `${containerEnv:VAR}` support
- [ ] Variable: Implement two-pass substitution
- [ ] Variable: Integration with env probe

### Phase 5: Lifecycle & Dotfiles (Week 5)

- [ ] Lifecycle: Integrate with ContainerProperties
- [ ] Lifecycle: Add marker file support
- [ ] Lifecycle: Implement origin tracking
- [ ] Dotfiles: Add CLI flags
- [ ] Dotfiles: Container-side execution
- [ ] Dotfiles: Marker file integration

### Phase 6: Output & Error Handling (Week 6)

- [ ] Output: Implement success JSON format
- [ ] Output: Implement error JSON format
- [ ] Errors: All error cases from spec
- [ ] Errors: Exit code handling
- [ ] Logging: stderr JSON/text formatting

### Phase 7: Testing & Polish (Week 7)

- [ ] Tests: CLI argument validation tests
- [ ] Tests: Container property extraction tests
- [ ] Tests: System patching idempotency tests
- [ ] Tests: Lifecycle marker file tests
- [ ] Tests: Integration tests per spec Section 15
- [ ] Examples: Add set-up examples to `examples/`
- [ ] Docs: Update README with set-up usage

---

## 5. Key Design Questions to Resolve

### 5.1 From Specification

1. **Secrets File Support**
   - Spec says "No `--secrets-file` in set-up" (Section 16)
   - Should we add it anyway for consistency with other commands?
   - **Recommendation:** Follow spec - omit for now, defer to `run-user-commands`

2. **Shell Server Persistence**
   - How long should shell servers remain alive?
   - Should they be per-command or persistent across lifecycle?
   - **Recommendation:** Per-lifecycle execution, reuse within single set-up invocation

3. **Marker File Locations**
   - User markers: `~/.devcontainer/` (configurable via `--container-data-folder`)
   - System markers: `/var/devcontainer/` (configurable via `--container-system-data-folder`)
   - **Recommendation:** Follow spec exactly

4. **Root Operations Failure**
   - Spec says "best-effort; continues when markers cannot be written" (Section 7)
   - Should we fail or warn?
   - **Recommendation:** Warn and continue (matches spec)

### 5.2 Implementation Details

5. **ContainerProperties Struct Design**
   - Should this be shared with `up` command?
   - Should it be in core or deacon crate?
   - **Recommendation:** Place in core, share with up/exec/run-user-commands

6. **Lifecycle Hook Origin Tracking**
   - Spec requires tracking which source (config vs feature) provided each hook
   - How to represent in merged config?
   - **Recommendation:** Add `origin` field to lifecycle command metadata

7. **Progress Events**
   - Should set-up emit progress events like build/up?
   - **Recommendation:** Yes, for consistency - add lifecycle phase events

---

## 6. Risk Assessment

### High Risk Items

1. **System File Patching** - Risk: Breaking container OS
   - Mitigation: Marker files, idempotency, test on multiple distros
   - Test: Verify on Debian, Ubuntu, Alpine, Fedora

2. **Shell Server Management** - Risk: Zombie processes, resource leaks
   - Mitigation: Proper cleanup, timeout handling
   - Test: Stress test with multiple invocations

3. **Root Operations** - Risk: Permission issues, security concerns
   - Mitigation: Minimal root operations, clear audit trail
   - Test: Test with rootless containers, non-root users

### Medium Risk Items

4. **Image Metadata Extraction** - Risk: Incorrect parsing of labels
   - Mitigation: Strict schema validation, fallback to empty
   - Test: Test with various image label formats

5. **Variable Substitution** - Risk: Incorrect expansion, infinite loops
   - Mitigation: Max depth limit, cycle detection
   - Test: Nested variable tests, cycle tests

### Low Risk Items

6. **Dotfiles Installation** - Risk: User script failures
   - Mitigation: Marker file prevents re-run, log all output
   - Test: Test with common dotfile repos

---

## 7. Testing Strategy (From Spec Section 15)

### Required Integration Tests

```pseudocode
TEST SUITE "set-up":
  ✗ TEST "config postAttachCommand": 
    - run container
    - set-up with --config
    - verify marker file created by postAttach
    
  ✗ TEST "metadata postCreateCommand": 
    - image label carries lifecycle
    - verify marker file created by postCreate
    
  ✗ TEST "include-config": 
    - set-up returns configuration and mergedConfiguration blobs
    
  ✗ TEST "remote-env substitution": 
    - container env TEST_CE propagates to merged "TEST_RE"
    
  ✗ TEST "invalid remote-env": 
    - argument validation error
    
  ✗ TEST "skip-post-create": 
    - set-up completes without running lifecycle
    - no dotfiles
    
  ✗ TEST "dotfiles install": 
    - with repository provided
    - marker prevents reinstall
    - respects explicit install command
END
```

**Status:** 0 of 7 tests implemented

---

## 8. Documentation Gaps

### Missing Documentation

1. **User-Facing**
   - ✗ No set-up section in README.md
   - ✗ No set-up examples in examples/
   - ✗ No set-up usage in EXAMPLES.md

2. **Developer-Facing**
   - ✗ No set-up architecture docs
   - ✗ No set-up API docs
   - ✗ No set-up testing guide

3. **Specification Alignment**
   - ✗ Not mentioned in AGENTS.md repository guidelines
   - ✗ Not in CLI parity tracking (CLI_PARITY.md lists it as missing)

---

## 9. Effort Estimate

### Implementation Effort

**Total Estimated Effort: 6-8 weeks (1 full-time developer)**

| Phase | Effort | Description |
|-------|--------|-------------|
| Phase 1: Foundation | 1.5 weeks | CLI, data structures, skeleton |
| Phase 2: Container Ops | 1.5 weeks | Shell servers, markers, exec |
| Phase 3: System Patching | 1 week | /etc patching, idempotency |
| Phase 4: Config/Substitution | 1.5 weeks | Metadata merge, containerEnv |
| Phase 5: Lifecycle/Dotfiles | 1.5 weeks | Hook integration, dotfiles |
| Phase 6: Output/Errors | 1 week | JSON output, error handling |
| Phase 7: Testing/Docs | 1 week | Tests, examples, docs |
| **Buffer** | 1 week | Unexpected issues, review |

### Complexity Factors

- **High Complexity:**
  - System file patching (requires root, OS-specific)
  - Shell server management (concurrency, cleanup)
  - Image metadata extraction (label parsing)

- **Medium Complexity:**
  - Container variable substitution (two-pass)
  - Lifecycle marker files (idempotency)
  - JSON output formatting (outcome-based)

- **Low Complexity:**
  - CLI argument parsing (straightforward)
  - Dotfiles integration (existing code)
  - Error handling (established patterns)

---

## 10. Recommendations

### Immediate Actions (Week 1)

1. **Create tracking issue** for set-up implementation
2. **Add set-up to project roadmap** in README
3. **Create stub implementation** returning NotImplemented
4. **Update CLI_PARITY.md** with this analysis

### Short-Term (Weeks 2-4)

5. **Implement foundation** (CLI, data structures)
6. **Build container operations** (properties, shell servers)
7. **Add system patching** (with tests)

### Medium-Term (Weeks 5-8)

8. **Complete configuration merging**
9. **Integrate lifecycle and dotfiles**
10. **Add comprehensive testing**
11. **Write user documentation and examples**

### Long-Term Considerations

12. **Feature parity monitoring** - Track against TS CLI
13. **Cross-platform testing** - Test on Linux/macOS/Windows
14. **Performance optimization** - Shell server pooling, parallel operations

---

## Appendix A: Specification Cross-Reference

| Spec Section | Requirement | Implementation Status |
|--------------|-------------|----------------------|
| 1. Overview | Purpose and personas | ✗ Not implemented |
| 2. CLI | All flags and options | ✗ Not implemented |
| 3. Input Processing | Argument parsing and validation | ✗ Not implemented |
| 4. Configuration Resolution | Config merge and substitution | ✗ Not implemented |
| 5. Core Execution Logic | Main execution flow | ✗ Not implemented |
| 6. State Management | Marker files and idempotency | ✗ Not implemented |
| 7. External System Interactions | Docker operations | ~ Partial (30%) |
| 8. Data Flow Diagrams | N/A (documentation) | N/A |
| 9. Error Handling | Error cases and output | ✗ Not implemented |
| 10. Output Specifications | JSON output format | ✗ Not implemented |
| 11. Performance | Shell servers, caching | ✗ Not implemented |
| 12. Security | Root operations, secrets | ✗ Not implemented |
| 13. Cross-Platform | OS-specific behavior | ✗ Not implemented |
| 14. Edge Cases | Corner case handling | ✗ Not implemented |
| 15. Testing Strategy | Required test cases | ✗ Not implemented (0/7) |
| 16. Design Decisions | Rationale documentation | ✗ Not documented |

**Overall Compliance: 2% (only basic Docker operations exist)**

---

## Appendix B: Related Commands Comparison

| Feature | `up` | `build` | `exec` | `set-up` |
|---------|------|---------|--------|----------|
| Creates container | ✓ | ✓ | ✗ | ✗ |
| Runs lifecycle | ✓ | ~ partial | ✗ | ✓ |
| Requires existing container | ✗ | ✗ | ✓ | ✓ |
| Patches /etc files | ~ maybe | ✗ | ✗ | ✓ |
| Image metadata merge | ✓ | ✓ | ✗ | ✓ |
| Dotfiles support | ✓ | ✗ | ✗ | ✓ |
| Returns config | ~ partial | ~ partial | ✗ | ✓ |

**Key Insight:** `set-up` is most similar to `up` but for existing containers. Substantial code reuse opportunity.

---

## Appendix C: Quick Start Guide (For Implementers)

### Step 1: Read the Spec
```bash
cd docs/subcommand-specs/set-up
cat README.md SPEC.md DATA-STRUCTURES.md DIAGRAMS.md
```

### Step 2: Create Stub
```bash
# Add to crates/deacon/src/cli.rs
# Add to crates/deacon/src/commands/mod.rs
# Create crates/deacon/src/commands/set_up.rs
```

### Step 3: Implement Foundation
```rust
// set_up.rs
pub struct SetUpArgs {
    pub container_id: String,
    // ... all other flags
}

pub async fn execute_set_up(args: SetUpArgs) -> Result<()> {
    // Phase 1: Initialization
    // Phase 2: Container discovery
    // Phase 3: Main execution
    // Phase 4: Output
}
```

### Step 4: Add Tests
```bash
# Create integration test
touch crates/deacon/tests/integration_set_up.rs
```

---

**End of Report**
