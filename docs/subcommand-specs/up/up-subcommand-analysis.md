# Up Subcommand Implementation Analysis

**Date**: October 12, 2025  
**Analysis of**: `deacon up` implementation vs. official specification in `docs/subcommand-specs/up/`

## Executive Summary

This report analyzes the current implementation of the `up` subcommand against the official specification documents. The implementation provides basic functionality for both traditional containers and Docker Compose workflows but is **missing substantial functionality** required by the specification.

**Overall Completeness**: ~40% implemented

## 1. Command-Line Interface (CLI Flags)

### ✅ Implemented Flags

| Flag | Status | Notes |
|------|--------|-------|
| `--workspace-folder` | ✅ Implemented | Global flag |
| `--config` | ✅ Implemented | Global flag |
| `--override-config` | ✅ Implemented | Global flag |
| `--remove-existing-container` | ✅ Implemented | |
| `--skip-post-create` | ✅ Implemented | |
| `--skip-non-blocking-commands` | ✅ Implemented | |
| `--ports-events` | ✅ Implemented | |
| `--shutdown` | ✅ Implemented | |
| `--forward-port` | ✅ Implemented | Repeatable, with port parsing |
| `--container-name` | ✅ Implemented | |
| `--additional-features` | ✅ Implemented | |
| `--prefer-cli-features` | ✅ Implemented | |
| `--feature-install-order` | ✅ Implemented | |
| `--ignore-host-requirements` | ✅ Implemented | |
| `--env-file` | ✅ Implemented | For compose |
| `--log-level` | ✅ Implemented | Global flag (info/debug/trace) |
| `--log-format` | ✅ Implemented | Global flag (text/json) |
| `--runtime` | ✅ Implemented | Global flag (docker/podman) |

### ❌ Missing Flags (Specification Required)

#### Docker/Compose Paths and Data
- `--docker-path` - Path to docker executable
- `--docker-compose-path` - Path to docker compose executable
- `--container-data-folder` - Container data directory
- `--container-system-data-folder` - System data directory

#### Workspace and Config Selection
- `--id-label` - Container identification labels (repeatable)
- `--mount-workspace-git-root` - Mount git root (boolean, default true)

#### Terminal Configuration
- `--terminal-columns` - Terminal columns for output
- `--terminal-rows` - Terminal rows for output

#### Runtime Behavior
- `--build-no-cache` - Build without cache (boolean)
- `--expect-existing-container` - Require existing container (boolean)
- `--workspace-mount-consistency` - Mount consistency mode (consistent|cached|delegated)
- `--gpu-availability` - GPU availability control (all|detect|none)
- `--default-user-env-probe` - Environment probe mode (none|loginInteractiveShell|interactiveShell|loginShell)
- `--update-remote-user-uid-default` - UID update control (never|on|off)

#### Lifecycle Control
- `--prebuild` - Stop after onCreate/updateContent; rerun updateContent
- `--skip-post-attach` - Skip postAttach phase only

#### Additional Mounts/Env/Cache/Build
- `--mount` - Additional mount specifications (repeatable, regex-validated)
- `--remote-env` - Remote environment variables (repeatable, name=value format)
- `--cache-from` - Build cache sources (repeatable)
- `--cache-to` - Build cache destination
- `--buildkit` - BuildKit control (auto|never)

#### Features/Dotfiles/Metadata
- `--skip-feature-auto-mapping` - Skip automatic feature mapping
- `--dotfiles-repository` - Dotfiles git repository URL
- `--dotfiles-install-command` - Dotfiles installation command
- `--dotfiles-target-path` - Dotfiles target path (default ~/dotfiles)
- `--container-session-data-folder` - Session data path
- `--omit-config-remote-env-from-metadata` - Omit remote env from metadata
- `--experimental-lockfile` - Enable lockfile support
- `--experimental-frozen-lockfile` - Enable frozen lockfile mode
- `--omit-syntax-directive` - Omit syntax directives

#### Output Shaping
- `--include-configuration` - Include config in output
- `--include-merged-configuration` - Include merged config in output
- `--user-data-folder` - Persisted host state path

### ⚠️ Validation Rules Not Enforced

The specification requires:
1. **At least one of**: `--workspace-folder` OR `--id-label` (currently only workspace-folder supported)
2. **At least one of**: `--workspace-folder` OR `--override-config`
3. **Mount format validation**: Regex `type=(bind|volume),source=([^,]+),target=([^,]+)(,external=(true|false))?`
4. **Remote-env format validation**: Must match `<name>=<value>`
5. **Terminal dimensions**: `--terminal-columns` implies `--terminal-rows` and vice versa
6. **Mutual exclusion/influence**: Proper handling of conflicting flags

## 2. Input Processing Pipeline

### ✅ Implemented
- Basic argument parsing via clap
- Workspace folder resolution
- Config file loading with fallback discovery
- Variable substitution applied to configuration

### ❌ Missing
- No normalization of arrays (mount, remote-env, cache-from)
- No parsing of additionalFeatures as JSON with validation
- No provided ID labels support
- Missing complete normalization pseudocode implementation from spec

## 3. Configuration Resolution

### ✅ Implemented
- Configuration file loading from path or discovery
- Override configuration support
- Variable substitution with SubstitutionContext
- Feature merging with CLI-provided features
- Host requirements validation

### ❌ Missing
- **Filename validation**: Should error if config file not named `devcontainer.json` or `.devcontainer/devcontainer.json`
- **initializeCommand execution**: Only partially implemented (exists but may not follow spec exactly)
- **ID labels discovery**: No `findContainerAndIdLabels` implementation
- **Beforehand container substitution**: Variable substitution timing may not match spec
- **Disallowed Features check**: No validation for prohibited feature IDs
- **Complete merging with image metadata**: Partial implementation

## 4. Core Execution Logic

### ✅ Implemented

#### Phase 1: Initialization
- ✅ Docker params creation
- ✅ Basic progress tracking

#### Phase 2: Configuration Resolution
- ✅ Configuration loading
- ✅ Compose vs. traditional detection
- ✅ Variable substitution

#### Phase 3: Main Execution - Dockerfile/Image Flow
- ✅ Find existing container (basic)
- ✅ Start existing container if present
- ✅ Build image (partial - missing features extension)
- ✅ Run container with labels, mounts, env
- ✅ Container inspection
- ⚠️ Merge image metadata (partial)
- ⚠️ Setup in container (partial lifecycle)

#### Phase 3: Main Execution - Compose Flow
- ✅ Resolve compose config
- ✅ Find container by project/service
- ✅ Remove if requested
- ✅ Create with `compose up -d`
- ⚠️ Read and merge image metadata (partial)
- ⚠️ Setup in container (partial lifecycle)

### ❌ Missing from Core Execution

1. **updateRemoteUserUID**: Config field exists but full implementation incomplete
   - No proper UID detection and image rebuilding
   - Missing BuildKit/Dockerfile generation for UID updates
   
2. **Container creation details**:
   - Missing security options application (capAdd, securityOpt, init, privileged)
   - Incomplete entrypoint handling
   
3. **Image metadata merging**:
   - No complete merge of devcontainer labels from image
   - Missing Feature provenance tracking
   
4. **Features extension**:
   - No image extension with Features during build
   - Missing BuildKit context construction for Features
   
5. **setupInContainer completeness**:
   - Partial lifecycle hooks (missing updateContent)
   - No dotfiles installation
   - Incomplete remote env setup
   - Missing secrets handling in lifecycle
   - No user env probe implementation

## 5. State Management

### ✅ Implemented
- Container state persistence (ContainerState)
- Compose state persistence (ComposeState)
- Workspace hash-based state tracking
- State cleanup on shutdown

### ❌ Missing
- No `--user-data-folder` support for host-side caching
- No `--container-session-data-folder` support
- Missing CLI data caching (userEnvProbe results)
- Missing temporary build artifacts tracking
- No lockfile support (experimental features)
- No cache management beyond Docker's built-in caching

## 6. External System Interactions

### ✅ Implemented

#### Docker/Container Runtime
- ✅ `docker inspect`
- ✅ `docker ps --filter label=...`
- ✅ `docker rm -f`
- ✅ `docker run` (basic)
- ✅ `docker exec`
- ✅ `docker compose config`
- ✅ `docker compose up -d`
- ⚠️ `docker build` (missing BuildKit features)

### ❌ Missing

#### Docker/Container Runtime
- `docker buildx build` with complete options:
  - Missing `--cache-from` handling
  - Missing `--cache-to` handling
  - Missing `--platform` support in up command
  - Missing `--push` support
- Compose project name inference from `.env` file
- TTY vs non-TTY handling differences
- Windows Unicode fallback via PTY for Compose v1

#### OCI Registries
- No Feature extension image metadata fetching
- Missing authentication and registry interaction for Features
- No caching of Feature resolution
- No lockfile generation/validation

## 7. Data Flow

### ✅ Implemented
- Basic flow: Args → Parse → Config → Container/Compose → Result
- Progress event emission
- State persistence

### ❌ Missing
- Complete merge configuration flow with image metadata
- Feature extension pipeline integration
- Dotfiles installation flow
- Complete user env probe flow

## 8. Error Handling Strategy

### ✅ Implemented
- User errors for missing/invalid args (partial)
- Configuration errors with descriptive messages
- Docker availability checking
- Lifecycle command failure handling
- Progress tracking for errors

### ❌ Missing
- Missing/invalid mount format validation
- Missing remote-env format validation
- Invalid devcontainer.json filename errors
- Disallowed Features error with feature ID
- BuildKit policy issue errors
- Windows Unicode/PTY fallback errors
- GPU requested but unsupported warnings
- Container exit during lifecycle detailed errors
- Permission denied error categorization

## 9. Output Specifications

### ✅ Implemented
- Basic command execution (no JSON output currently)
- Progress events emitted to tracker
- Debug/info logging

### ❌ Missing - **CRITICAL**

The specification requires **JSON output on stdout** with the following schema:

#### Success Output (Missing Entirely)
```json
{
  "outcome": "success",
  "containerId": "<string>",
  "composeProjectName": "<string, optional>",
  "remoteUser": "<string>",
  "remoteWorkspaceFolder": "<string>",
  "configuration": { /* when --include-configuration */ },
  "mergedConfiguration": { /* when --include-merged-configuration */ }
}
```

#### Error Output (Missing Entirely)
```json
{
  "outcome": "error",
  "message": "<string>",
  "description": "<string>",
  "containerId": "<string, optional>",
  "disallowedFeatureId": "<string, optional>",
  "didStopContainer": true,
  "learnMoreUrl": "<string, optional>"
}
```

**Current Implementation**: Returns Result<()> with no structured output
**Exit Codes**: Not explicitly managed (relies on anyhow error propagation)

## 10. Performance Considerations

### ✅ Implemented
- Container reuse when possible
- Basic progress tracking
- Background task handling for non-blocking commands

### ❌ Missing
- Docker build cache strategy (cache-from/to)
- BuildKit cache utilization
- Temp directories for Features optimization
- Parallel command execution in lifecycle (object form)
- finishBackgroundTasks await on success
- Host requirements caching (cpus, memory, storage, gpu)
- Image tagging optimization (avoid extra tags when no features)

## 11. Security Considerations

### ✅ Implemented
- Basic secret redaction support (RedactionConfig, SecretRegistry)
- Secret registry integration

### ❌ Missing
- `--secrets-file` flag
- Secrets mapped into env for lifecycle execution
- Complete secrets redaction in logs
- Privilege/capabilities application (init, privileged, capAdd, securityOpt)
- Input sanitization for mount and env formats
- Injection prevention via proper quoting in Compose YAML
- User/UID mapping isolation (updateRemoteUserUID incomplete)

## 12. Cross-Platform Behavior

### ✅ Implemented
- Runtime abstraction (Docker/Podman support)
- Basic path handling

### ❌ Missing
- Linux-specific: UID/GID mapping image updates
- macOS-specific: UID update path handling
- macOS-specific: BuildKit policy checks
- Windows-specific: Path handling via URI helpers
- Windows-specific: Compose v1 Unicode issue PTY fallback
- WSL2-specific: File URI to WSL path translation
- Host type detection influencing CLI exec choice

## 13. Edge Cases and Corner Cases

### ✅ Handled
- Missing configuration files (basic error)
- Container exits during lifecycle (error surfaced)

### ❌ Not Handled
- Empty configuration files (may not error explicitly)
- Circular `extends` validation
- Network partitions during build/pull
- Permission denied scenarios (writing temp files, creating volumes)
- Read-only filesystem errors
- Container not found after creation race conditions

## 14. Testing Strategy

### ✅ Existing Tests
- Unit tests for:
  - UpArgs creation
  - commands_from_json_value parsing
  - Compose vs traditional detection
  - Port specification parsing
  
### ❌ Missing Tests (from Specification)
- Happy path image test
- Happy path features test
- Compose image test
- Compose Dockerfile test
- Missing config error test
- Mount format invalid test
- Remote-env format invalid test
- Skip-post-create behavior test
- Prebuild mode test
- Remove-existing test
- Expect-existing test
- Include-config output test

## 15. Lifecycle Commands

### ✅ Implemented
- initializeCommand (host-side, partial)
- onCreateCommand execution
- postCreateCommand execution
- postStartCommand execution
- postAttachCommand execution

### ❌ Missing
- **updateContentCommand**: Not executed (critical for prebuild mode)
- Prebuild mode: Stop after onCreate/updateContent; rerun updateContent
- Parallel command blocks (object form with concurrent execution)
- Output buffering to avoid interleaving in parallel execution
- finishBackgroundTasks await pattern
- Proper lifecycle ordering per spec

## 16. Dotfiles Support

### ❌ Completely Missing
- No `--dotfiles-repository` support
- No `--dotfiles-install-command` support
- No `--dotfiles-target-path` support
- No dotfiles installation workflow
- Spec indicates dotfiles installed during setupInContainer phase

## 17. Features System Integration

### ✅ Implemented
- Feature merging from CLI
- Feature install order override
- Basic feature configuration

### ❌ Missing
- Feature extension of base images
- OCI registry resolution for Features
- Feature metadata fetching and caching
- Lockfile generation and validation (experimental flags)
- BuildKit context construction for Features
- Feature auto-mapping (and skip flag)
- Temp/cache folder for Features

## 18. Compose-Specific Functionality

### ✅ Implemented
- Basic compose project creation
- Compose file resolution
- Service specification
- `compose up -d` execution
- Project name handling
- Env file support

### ❌ Missing
- Compose project name inference from `.env` file
- Complete profile handling (--profile *)
- Windows Compose v1 Unicode PTY fallback
- Complete security options warning (partially implemented)
- Mount conversion for additional --mount flags

## 19. Missing Data Structures

From the specification's DATA-STRUCTURES.md:

### ❌ Not Implemented
- `ProvisionOptions` struct (complete spec version)
- `DockerResolverParameters` struct
- `ContainerProperties` struct (complete spec version)
- `ResolverResult` struct
- `LifecycleHooksInstallMap` struct
- Proper JSON schema output structures

## 20. Migration and Compatibility

### ✅ Implemented
- No deprecated features used
- Clean slate implementation

### ❌ Missing
- Reference TS CLI compatibility markers
- Compose v2 vs v1 handling
- Behavior parity with reference implementation

## Summary of Critical Missing Features

### High Priority (Breaks Core Functionality)
1. ❌ **JSON stdout output** - Spec requires structured JSON output with outcome/containerId/etc.
2. ❌ **updateContentCommand** - Critical lifecycle phase not executed
3. ❌ **Prebuild mode** - `--prebuild` flag not implemented
4. ❌ **ID labels** - `--id-label` flag for container identification
5. ❌ **Include configuration output** - `--include-configuration` and `--include-merged-configuration`
6. ❌ **Complete image metadata merging** - Feature provenance and labels
7. ❌ **Features image extension** - Building extended images with Features

### Medium Priority (Limits Functionality)
8. ❌ **Dotfiles installation** - Complete workflow missing
9. ❌ **Additional mounts** - `--mount` flag with validation
10. ❌ **Remote environment** - `--remote-env` flag
11. ❌ **Cache management** - `--cache-from` and `--cache-to`
12. ❌ **BuildKit complete support** - `--buildkit` flag and full features
13. ❌ **GPU availability** - `--gpu-availability` control
14. ❌ **User env probe** - `--default-user-env-probe` modes
15. ❌ **UID update control** - `--update-remote-user-uid-default`
16. ❌ **Terminal dimensions** - `--terminal-columns` and `--terminal-rows`

### Lower Priority (Nice to Have)
17. ❌ **Experimental lockfile** - Feature lockfile support
18. ❌ **Session data folder** - `--container-session-data-folder`
19. ❌ **User data folder** - `--user-data-folder`
20. ❌ **Custom Docker paths** - `--docker-path` and `--docker-compose-path`

## Recommendations

### Immediate Actions
1. **Implement JSON output structure** on stdout as specified
2. **Add updateContentCommand** execution in lifecycle
3. **Implement prebuild mode** (`--prebuild` flag)
4. **Add ID labels support** for container identification
5. **Implement configuration output** flags

### Short-term Actions
6. Add missing CLI flags with validation (mount, remote-env, cache)
7. Complete lifecycle command execution (updateContent phase)
8. Implement dotfiles installation workflow
9. Complete image metadata merging
10. Add Features image extension support

### Long-term Actions
11. Implement experimental lockfile support
12. Add complete BuildKit integration
13. Implement cross-platform specific behaviors
14. Add comprehensive test suite per specification
15. Performance optimizations (parallel execution, caching)

## Compatibility Assessment

**Current state**: The implementation provides a working MVP for basic use cases but **significantly deviates from the specification**. Users expecting reference implementation behavior will encounter:

- Missing JSON output (breaks tooling integration)
- Missing lifecycle phases (breaks prebuild workflows)
- Missing advanced features (dotfiles, custom mounts, caching)
- Incomplete metadata handling (breaks feature provenance)

**Estimated work**: Approximately 200-300 hours to reach full specification compliance.

---

**Report generated**: October 12, 2025  
**Specification version**: docs/subcommand-specs/up/ (current)  
**Implementation version**: Current main branch
