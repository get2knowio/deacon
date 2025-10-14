# Run-User-Commands Implementation Gap Analysis

**Date:** October 13, 2025  
**Specification Source:** `/docs/subcommand-specs/run-user-commands/` (SPEC.md, DATA-STRUCTURES.md, DIAGRAMS.md)  
**Implementation Source:** `crates/deacon/src/commands/run_user_commands.rs`, `crates/core/src/container_lifecycle.rs`

---

## Executive Summary

The current implementation of `run-user-commands` provides a **partial, basic implementation** but is **significantly incomplete** when compared to the official specification. The implementation covers approximately **30-40%** of the required functionality.

**Critical Missing Components:**
1. ❌ JSON output contract (stdout/stderr separation)
2. ❌ Container selection by `--container-id` and `--id-label`
3. ❌ Prebuild and stop-for-personalization modes with proper exit states
4. ❌ Marker file-based idempotency system
5. ❌ Dotfiles installation workflow
6. ❌ Remote environment probing with caching
7. ❌ Secrets injection and log redaction
8. ❌ Two-phase variable substitution (pre-container + container)
9. ❌ Image metadata merge workflow
10. ❌ WaitFor semantics and proper non-blocking command handling

---

## 1. Command-Line Interface Gaps

### 1.1 Missing CLI Flags (High Priority)

| Flag | Status | Spec Requirement | Current Behavior |
|------|--------|------------------|------------------|
| `--container-id <ID>` | ❌ Missing | Target container ID directly | No way to specify container ID |
| `--id-label <name=value>` | ❌ Missing | Repeatable, find container by labels | Not implemented |
| `--workspace-folder <PATH>` | ✅ Implemented | Root for config discovery | Works |
| `--config <PATH>` | ✅ Implemented | Explicit config path | Works |
| `--override-config <PATH>` | ✅ Implemented | Override config | Works |
| `--docker-path <PATH>` | ❌ Missing | Docker CLI binary path | Uses default |
| `--docker-compose-path <PATH>` | ❌ Missing | Docker Compose CLI path | Uses default |
| `--container-data-folder <PATH>` | ❌ Missing | In-container user data folder | Not configurable |
| `--container-system-data-folder <PATH>` | ❌ Missing | In-container system state folder | Not configurable |
| `--container-session-data-folder <PATH>` | ❌ Missing | Cache folder for CLI data | No caching |
| `--mount-workspace-git-root` | ❌ Missing | Mount behavior parity | Not implemented |
| `--skip-non-blocking-commands` | ✅ Implemented | Stop after waitFor | Implemented but incomplete |
| `--prebuild` | ⚠️ Partial | Stop after updateContent, force rerun | Flag exists but not implemented |
| `--stop-for-personalization` | ⚠️ Partial | Stop after dotfiles | Flag exists but no dotfiles |
| `--skip-post-attach` | ✅ Implemented | Do not run postAttachCommand | Works |
| `--skip-post-create` | ✅ Implemented | Do not run postCreateCommand | Works |
| `--default-user-env-probe {none\|...}` | ❌ Missing | Default for userEnvProbe | Not configurable |
| `--remote-env <NAME=VALUE>` | ❌ Missing | Repeatable extra env vars | Not implemented |
| `--secrets-file <PATH>` | ⚠️ Partial | JSON file with secrets | Loaded but not injected/redacted |
| `--dotfiles-repository <URL>` | ❌ Missing | Dotfiles source | Not implemented |
| `--dotfiles-install-command <CMD>` | ❌ Missing | Install script path | Not implemented |
| `--dotfiles-target-path <PATH>` | ❌ Missing | Default ~/dotfiles | Not implemented |
| `--log-level {info\|debug\|trace}` | ✅ Implemented | Via global flag | Works |
| `--log-format {text\|json}` | ✅ Implemented | Via global flag | Works |
| `--terminal-columns <N>` | ❌ Missing | Requires --terminal-rows | Not implemented |
| `--terminal-rows <N>` | ❌ Missing | Requires --terminal-columns | Not implemented |
| `--skip-feature-auto-mapping` | ❌ Missing | Hidden/testing flag | Not implemented |

**Summary:** 7/31 flags fully implemented (23%), 3/31 partially (10%), 21/31 missing (67%)

### 1.2 Argument Validation Issues

| Validation Rule | Status | Spec Requirement |
|----------------|--------|------------------|
| `--id-label` format validation (`.+=.+`) | ❌ Missing | Must match name=value with non-empty value |
| `--remote-env` format validation (`.+=.*`) | ❌ Missing | Must match name=value (empty value OK) |
| Container selection requirement | ❌ Missing | At least one of container-id, id-label, or workspace-folder required |
| `--terminal-columns` and `--terminal-rows` pairing | ❌ Missing | Must be provided together |
| Container not found error | ⚠️ Partial | Currently returns generic "No running container found" |

---

## 2. Configuration Resolution and Merging Gaps

### 2.1 Container Selection

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Direct by `--container-id` | ❌ Missing | Use provided container ID | Not implemented |
| By `--id-label` lookup | ❌ Missing | Find container matching all labels | Not implemented |
| By workspace folder (inferred labels) | ⚠️ Partial | Compute labels from workspace/config | Uses `resolve_target_container` helper but not spec-compliant |
| Container not found error | ⚠️ Partial | "Dev container not found." | Returns "No running container found. Run 'deacon up' first" |

### 2.2 Variable Substitution

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Two-phase substitution | ❌ Missing | 1) Before-container: `${devcontainerId}`, 2) In-container: `${containerEnv:VAR}` | Single-pass substitution only |
| `${devcontainerId}` | ❌ Missing | Derived from provided/implicit labels | Not substituted |
| `${containerEnv:VAR}` | ⚠️ Partial | Use actual container environment | Partial - merged env exists but not substituted correctly |
| `${containerWorkspaceFolder}` | ✅ Implemented | Resolved from config or default | Works |
| `${containerWorkspaceFolderBasename}` | ❌ Missing | Basename of workspace folder | Not implemented |
| `${env:V}` / `${localEnv:V}` | ✅ Implemented | Host environment | Works |
| `${localWorkspaceFolder}` | ✅ Implemented | Host workspace folder | Works |
| `${localWorkspaceFolderBasename}` | ❌ Missing | Basename of host workspace | Not implemented |
| Default values `${env:NAME:default}` | ❌ Missing | Default when variable missing | Not implemented |
| Error on missing variable name `${env:}` | ❌ Missing | Must be an error | Not validated |

### 2.3 Image Metadata Merge

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Read image metadata from container | ❌ Missing | Extract metadata embedded during build | Not implemented |
| Merge lifecycle commands | ❌ Missing | Collect arrays from features + config | Not implemented |
| Merge `remoteEnv` | ❌ Missing | Shallow merge from metadata | Not implemented |
| Merge `containerEnv` | ❌ Missing | Shallow merge from metadata | Not implemented |
| Merge `waitFor` | ❌ Missing | Use last defined value | Not implemented |
| Merge `mounts` | ❌ Missing | Merge unique targets | Not implemented |
| Merge `customizations` | ❌ Missing | Fold customizations | Not implemented |
| Create `MergedDevContainerConfig` | ❌ Missing | Final merged config structure | Not implemented |

### 2.4 ContainerProperties Creation

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| `createdAt` timestamp | ❌ Missing | From container inspect | Not captured |
| `startedAt` timestamp | ❌ Missing | From container inspect | Not captured |
| `osRelease` info | ❌ Missing | Hardware, id, version | Not captured |
| `user` | ⚠️ Partial | From config or inspect | Partially implemented |
| `gid` | ❌ Missing | Group ID | Not captured |
| `env` | ⚠️ Partial | Merged environment | Partial implementation |
| `shell` | ⚠️ Partial | Detected shell (e.g., /bin/bash) | Detection exists but not fully integrated |
| `homeFolder` | ❌ Missing | User's home folder | Not captured |
| `userDataFolder` | ❌ Missing | Default ~/.devcontainer | Not captured |
| `installFolder` | ❌ Missing | E.g., /workspaces/.devcontainer | Not captured |
| `remoteWorkspaceFolder` | ✅ Implemented | Container workspace folder | Works |
| `remoteExec` function | ⚠️ Partial | Docker exec wrapper | Exists in lifecycle module |
| `remotePtyExec` function | ❌ Missing | PTY exec wrapper | Not implemented |
| `shellServer` | ❌ Missing | Shell server abstraction | Not implemented |

---

## 3. Core Execution Logic Gaps

### 3.1 Lifecycle Hook Execution

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| `initializeCommand` (host) | ✅ Implemented | Runs on host before container | Works |
| `onCreateCommand` (container) | ✅ Implemented | First container hook | Works |
| `updateContentCommand` (container) | ✅ Implemented | Update hook | Works |
| `postCreateCommand` (container) | ✅ Implemented | After create | Works |
| `postStartCommand` (container) | ⚠️ Partial | Non-blocking phase | Queued but not executed |
| `postAttachCommand` (container) | ⚠️ Partial | Non-blocking phase | Queued but not executed |
| Object syntax (parallel execution) | ❌ Missing | Object form runs values concurrently | Only string/array supported |
| Named command steps | ❌ Missing | Object keys name steps | Not implemented |
| Buffered output per command | ❌ Missing | Avoid interleaving | Not implemented |

### 3.2 Idempotency and Markers

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Marker files in `userDataFolder` | ❌ Missing | `.onCreateCommandMarker`, etc. | Not implemented |
| Marker content (timestamp) | ❌ Missing | Container CreatedAt/StartedAt | Not implemented |
| Atomic marker updates | ❌ Missing | Use shell noclobber/mkdir -p | Not implemented |
| Check marker before running hook | ❌ Missing | Skip if marker matches state | Not implemented |
| `--prebuild` forces updateContent rerun | ❌ Missing | Rerun even if marker exists | Not implemented |
| Permission denied on marker folder | ❌ Missing | Hook skipped if can't write | Not handled |

### 3.3 Dotfiles Installation

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Dotfiles repository clone | ❌ Missing | Git clone inside container | Not implemented |
| `owner/repo` expansion | ❌ Missing | Expand to github.com URL | Not implemented |
| Local path support | ❌ Missing | `./path` dotfiles | Not implemented |
| Install command execution | ❌ Missing | Run specified or fallback script | Not implemented |
| Fallback script names | ❌ Missing | install/bootstrap/setup | Not implemented |
| Target path | ❌ Missing | Default ~/dotfiles | Not implemented |
| Stop after dotfiles | ❌ Missing | `--stop-for-personalization` | Not implemented |

### 3.4 Remote Environment Probing

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| `userEnvProbe` modes | ⚠️ Partial | none, loginInteractiveShell, interactiveShell, loginShell | Basic probing exists but not spec-compliant |
| Shell detection | ⚠️ Partial | Detect user's shell | Implemented in `container_env_probe` |
| Spawn shell with flags | ⚠️ Partial | `-lic`, `-ic`, `-lc`, `-c` | Partially implemented |
| Read /proc/self/environ | ❌ Missing | Or fallback to printenv | Not implemented |
| Cache in `--container-session-data-folder` | ❌ Missing | `env-<probe>.json` | Not implemented |
| Cache hit/miss logic | ❌ Missing | Read cache first, probe on miss | Not implemented |
| Merge with `remoteEnv` flags | ❌ Missing | Shell env + remote env + config.remoteEnv | Partial merging exists |
| Default `userEnvProbe` from flag | ❌ Missing | `--default-user-env-probe` | Not configurable |

### 3.5 Secrets Handling

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Load from `--secrets-file` (JSON) | ✅ Implemented | Read key/value secrets | Works |
| Inject into execution environment | ❌ Missing | Merge into command env | Loaded but not injected into lifecycle |
| Log redaction/masking | ⚠️ Partial | Replace secret values with `********` | Framework exists but not applied to lifecycle |
| Filter `BASH_FUNC_*` keys | ❌ Missing | Filter function exports | Not implemented |
| Multi-file secrets support | ✅ Implemented | Multiple --secrets-file | Works |

### 3.6 WaitFor and Early Exit

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| `waitFor` configuration | ❌ Missing | Default `updateContentCommand` | Not read from config |
| `--skip-non-blocking-commands` | ⚠️ Partial | Stop after `waitFor` hook | Skips postStart/postAttach but doesn't check waitFor |
| Return `"skipNonBlocking"` | ❌ Missing | JSON result on stdout | Not implemented |
| `--prebuild` mode | ❌ Missing | Stop after updateContent, return `"prebuild"` | Flag exists but not implemented |
| `--stop-for-personalization` | ❌ Missing | Stop after dotfiles, return `"stopForPersonalization"` | Flag exists but not implemented |
| Return `"done"` on completion | ❌ Missing | JSON result on stdout | Not implemented |

---

## 4. Output Contract Gaps

### 4.1 Standard Output (stdout)

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Single JSON line to stdout | ❌ Missing | Always emit one JSON line | No JSON output |
| Success schema | ❌ Missing | `{ "outcome": "success", "result": "..." }` | Not implemented |
| Error schema | ❌ Missing | `{ "outcome": "error", "message": "...", "description": "..." }` | Not implemented |
| Result values | ❌ Missing | `skipNonBlocking`, `prebuild`, `stopForPersonalization`, `done` | Not implemented |

### 4.2 Standard Error (stderr)

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| All logs to stderr | ✅ Implemented | Logs, progress, diagnostics | Works |
| JSON log format option | ✅ Implemented | `--log-format json` | Works |
| Secret redaction in logs | ⚠️ Partial | Replace secret values | Framework exists but not fully applied |

### 4.3 Exit Codes

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Exit 0 on success | ✅ Implemented | Includes early exits | Works |
| Exit 1 on error | ✅ Implemented | Argument, config, runtime failures | Works |
| Error message on failure | ⚠️ Partial | Descriptive errors | Basic errors but not JSON contract |

---

## 5. State Management and Persistence Gaps

### 5.1 Marker Files

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| `.onCreateCommandMarker` | ❌ Missing | Timestamp of onCreate execution | Not implemented |
| `.updateContentCommandMarker` | ❌ Missing | Timestamp of updateContent execution | Not implemented |
| `.postCreateCommandMarker` | ❌ Missing | Timestamp of postCreate execution | Not implemented |
| `.postStartCommandMarker` | ❌ Missing | Timestamp of postStart execution | Not implemented |
| Marker location | ❌ Missing | `${HOME}/.devcontainer` or `--container-data-folder` | Not implemented |
| Atomic creation | ❌ Missing | Shell noclobber or mkdir -p | Not implemented |

### 5.2 Environment Cache

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Cache location | ❌ Missing | `--container-session-data-folder` | Not implemented |
| Cache filename | ❌ Missing | `env-<userEnvProbe>.json` | Not implemented |
| Cache read on probe | ❌ Missing | Check cache before probing | Not implemented |
| Cache write after probe | ❌ Missing | Write cache after successful probe | Not implemented |
| Cache invalidation | ❌ Missing | Per probe mode | Not implemented |

---

## 6. Error Handling Gaps

### 6.1 User Errors

| Error Scenario | Status | Spec Requirement | Current Implementation |
|----------------|--------|------------------|------------------------|
| Invalid `--id-label` format | ❌ Missing | CLI argument error, exit 1 | No validation |
| Invalid `--remote-env` format | ❌ Missing | CLI argument error, exit 1 | No validation |
| Missing container selection | ❌ Missing | CLI argument error | Not validated (falls back to workspace) |
| Config not found | ⚠️ Partial | JSON error with specific message | Generic error, not JSON |
| Container not found | ⚠️ Partial | JSON error: "Dev container not found." | Generic error, not JSON |

### 6.2 System Errors

| Error Scenario | Status | Spec Requirement | Current Implementation |
|----------------|--------|------------------|------------------------|
| Docker unavailable | ⚠️ Partial | `ContainerError` → JSON error, exit 1 | Error but not JSON output |
| Inspect fails | ⚠️ Partial | `ContainerError` → JSON error | Error but not JSON output |
| Network failure (dotfiles) | ❌ Missing | `ContainerError`, stop execution | Dotfiles not implemented |

### 6.3 Execution Errors

| Error Scenario | Status | Spec Requirement | Current Implementation |
|----------------|--------|------------------|------------------------|
| Lifecycle command fails (non-zero) | ⚠️ Partial | Skip remaining hooks, JSON error | Error propagation works but no JSON output |
| SIGINT handling | ❌ Missing | "interrupted" error message | Not specifically handled |
| Substitution failure | ⚠️ Partial | Error with source reference | Basic error handling |

---

## 7. Performance and Caching Gaps

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Remote env probe caching | ❌ Missing | Cache per probe mode | Not implemented |
| Object syntax parallelization | ❌ Missing | Run values concurrently | Not supported |
| Buffered output (parallel) | ❌ Missing | Avoid interleaving | Not implemented |
| Resource limits awareness | ❌ Missing | PTY by default, terminal dimensions | Basic exec only |

---

## 8. Security Gaps

| Feature | Status | Spec Requirement | Current Implementation |
|---------|--------|------------------|------------------------|
| Secrets injection | ⚠️ Partial | Merge into execution env | Loaded but not injected |
| Secrets masking in logs | ⚠️ Partial | Replace values with `********` | Framework exists but not fully applied |
| `BASH_FUNC_*` filtering | ❌ Missing | Filter function exports | Not implemented |
| Array command syntax (injection protection) | ⚠️ Partial | Avoid shell injection | Supports arrays but not object syntax |

---

## 9. Testing Gaps

Based on spec Section 15, the following test scenarios are **not currently covered**:

| Test Scenario | Status | Notes |
|---------------|--------|-------|
| Happy path with markers | ❌ Missing | No marker implementation |
| Config in subfolder | ❌ Missing | No tests for subfolder configs |
| Invalid workspace error | ❌ Missing | No validation tests |
| `--skip-non-blocking-commands` with waitFor | ❌ Missing | No waitFor implementation |
| `--prebuild` with updateContent rerun | ❌ Missing | No prebuild implementation |
| `--skip-post-attach` | ⚠️ Partial | Flag works but no comprehensive tests |
| Secrets injection and masking | ❌ Missing | No integration tests |

---

## 10. Cross-Platform Gaps

| Aspect | Status | Notes |
|--------|--------|-------|
| Path handling (Linux) | ✅ Works | POSIX paths in container |
| Path handling (macOS) | ⚠️ Untested | Should work but not verified |
| Path handling (Windows) | ⚠️ Untested | Host path resolution needs testing |
| Path handling (WSL2) | ⚠️ Untested | Windows host + Linux container |
| Docker socket access | ⚠️ Partial | Assumes default socket, not configurable via `--docker-path` |

---

## 11. Data Structure Gaps

Comparing against `DATA-STRUCTURES.md`:

### 11.1 RunUserCommandsArgs Structure

| Field | Status | Spec Requirement |
|-------|--------|------------------|
| `userDataFolder` | ❌ Missing | Host persisted path |
| `dockerPath` | ❌ Missing | Docker CLI path |
| `dockerComposePath` | ❌ Missing | Docker Compose path |
| `containerDataFolder` | ❌ Missing | In-container data folder |
| `containerSystemDataFolder` | ❌ Missing | In-container system folder |
| `workspaceFolder` | ✅ Implemented | Works |
| `mountWorkspaceGitRoot` | ❌ Missing | Mount behavior |
| `containerId` | ❌ Missing | Direct container ID |
| `idLabel` | ❌ Missing | Repeatable labels |
| `configPath` | ✅ Implemented | Works |
| `overrideConfigPath` | ✅ Implemented | Works |
| `logLevel` | ✅ Implemented | Via global flag |
| `logFormat` | ✅ Implemented | Via global flag |
| `terminalRows` | ❌ Missing | Terminal dimensions |
| `terminalColumns` | ❌ Missing | Terminal dimensions |
| `defaultUserEnvProbe` | ❌ Missing | Default probe mode |
| `skipNonBlocking` | ✅ Implemented | Works |
| `prebuild` | ⚠️ Partial | Flag exists, not implemented |
| `stopForPersonalization` | ⚠️ Partial | Flag exists, not implemented |
| `remoteEnv` | ❌ Missing | Repeatable KEY=VALUE |
| `skipFeatureAutoMapping` | ❌ Missing | Hidden flag |
| `skipPostAttach` | ✅ Implemented | Works |
| `dotfilesRepository` | ❌ Missing | Dotfiles source |
| `dotfilesInstallCommand` | ❌ Missing | Install script |
| `dotfilesTargetPath` | ❌ Missing | Target path |
| `containerSessionDataFolder` | ❌ Missing | Cache folder |
| `secretsFile` | ✅ Implemented | Works (loading only) |

**Summary:** 8/29 fields implemented (28%), 2/29 partial (7%), 19/29 missing (65%)

### 11.2 ContainerProperties Structure

| Field | Status | Notes |
|-------|--------|-------|
| `createdAt` | ❌ Missing | Not captured |
| `startedAt` | ❌ Missing | Not captured |
| `osRelease` | ❌ Missing | Not captured |
| `user` | ⚠️ Partial | Partially implemented |
| `gid` | ❌ Missing | Not captured |
| `env` | ⚠️ Partial | Partially implemented |
| `shell` | ⚠️ Partial | Detection exists |
| `homeFolder` | ❌ Missing | Not captured |
| `userDataFolder` | ❌ Missing | Not captured |
| `installFolder` | ❌ Missing | Not captured |
| `remoteWorkspaceFolder` | ✅ Implemented | Works |
| `remoteExec` | ⚠️ Partial | Exists but not fully integrated |
| `remotePtyExec` | ❌ Missing | Not implemented |
| `shellServer` | ❌ Missing | Not implemented |

### 11.3 MergedDevContainerConfig Structure

**Status:** ❌ **Completely Missing**

The entire image metadata merge workflow is not implemented. The spec requires creating a `MergedDevContainerConfig` by:
- Reading image metadata from container labels/env
- Merging lifecycle command arrays from features + config
- Merging `remoteEnv`, `containerEnv`, `waitFor`, etc.

Current implementation only uses the base `DevContainerConfig` without any metadata merging.

### 11.4 ExecutionResult Structures

**Status:** ❌ **Completely Missing**

No JSON output is produced. The spec requires:
- `ExecutionResultSuccess`: `{ "outcome": "success", "result": "..." }`
- `ExecutionResultError`: `{ "outcome": "error", "message": "...", "description": "..." }`

Current implementation returns Rust `Result<()>` with no JSON serialization.

---

## 12. Priority Recommendations

### 12.1 Critical (Must-Have for Spec Compliance)

1. **JSON Output Contract** - Implement single-line JSON to stdout, all logs to stderr
2. **Container Selection** - Support `--container-id` and `--id-label` flags
3. **Idempotency System** - Marker files in `userDataFolder` with timestamp checking
4. **WaitFor Semantics** - Read `waitFor` from config, implement proper early exit
5. **Prebuild Mode** - Implement `--prebuild` with updateContent rerun and proper exit state
6. **Image Metadata Merge** - Read and merge image metadata into configuration
7. **Two-Phase Substitution** - Before-container and in-container substitution passes
8. **ContainerProperties** - Create complete ContainerProperties from container inspection

### 12.2 High Priority (Core Functionality)

9. **Dotfiles Installation** - Full dotfiles workflow with repository, install, and stop-for-personalization
10. **Remote Env Caching** - Implement `--container-session-data-folder` and env cache
11. **Secrets Injection** - Inject secrets into lifecycle command environment
12. **Object Syntax Support** - Parallel execution for object-form lifecycle commands
13. **CLI Flag Completion** - Add all missing CLI flags with proper validation
14. **Marker Failure Handling** - Handle permission denied and read-only filesystems

### 12.3 Medium Priority (Robustness)

15. **Error JSON Output** - All errors as JSON with proper message/description
16. **Remote Env Probe Modes** - Full implementation of all userEnvProbe modes
17. **Terminal Dimensions** - Support `--terminal-columns` and `--terminal-rows`
18. **Shell Server Abstraction** - Implement shellServer for cleaner exec
19. **PTY Exec Support** - Implement remotePtyExec for interactive commands
20. **Comprehensive Testing** - Test suite per Section 15 requirements

### 12.4 Lower Priority (Nice-to-Have)

21. **Docker/Compose Path Configuration** - `--docker-path`, `--docker-compose-path`
22. **Mount Workspace Git Root** - `--mount-workspace-git-root` flag
23. **Skip Feature Auto Mapping** - Hidden `--skip-feature-auto-mapping` flag
24. **Cross-Platform Testing** - Verify macOS, Windows, WSL2 behavior
25. **Performance Optimization** - Buffered output, parallelization improvements

---

## 13. Estimated Implementation Effort

Based on the gap analysis:

| Priority Level | Features | Estimated Effort |
|----------------|----------|------------------|
| Critical (P1) | 8 items | 4-6 weeks |
| High (P2) | 6 items | 3-4 weeks |
| Medium (P3) | 6 items | 2-3 weeks |
| Lower (P4) | 5 items | 1-2 weeks |
| **Total** | **25 items** | **10-15 weeks** |

**Current Completeness:** ~30-40%  
**Effort to Full Compliance:** ~10-15 weeks (2.5-4 months)

---

## 14. Breaking Changes Required

To achieve spec compliance, the following **breaking changes** are required:

1. **Output Format Change**: Current implementation produces no structured output; must switch to JSON stdout/stderr separation
2. **Container Selection**: Current implementation only supports workspace-based lookup; must add `--container-id` and `--id-label`
3. **Exit States**: Must implement `skipNonBlocking`, `prebuild`, `stopForPersonalization` result states
4. **Marker Files**: Must create and check marker files (new filesystem state)
5. **Secrets Injection**: Must inject secrets into command environment (behavior change)

---

## 15. Conclusion

The current `run-user-commands` implementation is a **minimal prototype** that covers basic lifecycle command execution but **lacks most of the specification's requirements**. 

**Key Takeaways:**
- ✅ **What Works:** Basic command execution, skip flags for postCreate/postAttach, config loading, secrets file loading
- ⚠️ **Partially Works:** Container selection (workspace only), environment probing (basic), substitution (single-pass)
- ❌ **Missing Entirely:** JSON output contract, idempotency markers, dotfiles, prebuild mode, image metadata merge, remote env caching, full substitution, container-id/id-label selection

**Recommendation:** This subcommand should be marked as **experimental** or **preview** until the critical P1 and P2 items are implemented. Full spec compliance will require significant development effort (10-15 weeks).

---

## Appendix: Quick Reference Checklist

### Implementation Status by Category

| Category | Implemented | Partial | Missing | Total | % Complete |
|----------|-------------|---------|---------|-------|------------|
| CLI Flags | 7 | 3 | 21 | 31 | 23% |
| Container Selection | 0 | 1 | 3 | 4 | 0% |
| Variable Substitution | 3 | 2 | 5 | 10 | 30% |
| Image Metadata | 0 | 0 | 7 | 7 | 0% |
| ContainerProperties | 1 | 4 | 9 | 14 | 7% |
| Lifecycle Execution | 4 | 2 | 3 | 9 | 44% |
| Idempotency | 0 | 0 | 6 | 6 | 0% |
| Dotfiles | 0 | 0 | 7 | 7 | 0% |
| Remote Env Probe | 0 | 4 | 5 | 9 | 0% |
| Secrets | 2 | 1 | 2 | 5 | 40% |
| WaitFor/Early Exit | 0 | 1 | 5 | 6 | 0% |
| Output Contract | 2 | 1 | 4 | 7 | 29% |
| State Management | 0 | 0 | 11 | 11 | 0% |
| Error Handling | 2 | 7 | 2 | 11 | 18% |
| **Overall** | **21** | **26** | **90** | **137** | **~30%** |

---

**End of Gap Analysis**
