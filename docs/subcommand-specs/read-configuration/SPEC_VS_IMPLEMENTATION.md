# Read-Configuration: Specification vs Implementation Matrix

**Last Updated:** October 13, 2025

This document provides a side-by-side comparison of specification requirements against current implementation status.

## Legend

- ✅ **Fully Implemented** - Feature works as specified
- 🟡 **Partially Implemented** - Feature exists but incomplete or incorrect
- ❌ **Not Implemented** - Feature missing
- 🔵 **Global Flag** - Implemented at CLI level (shared across commands)
- ⏭️ **Intentionally Skipped** - Not needed for current scope

---

## 1. CLI Flags Matrix

| Flag | Spec | Implementation | Status | Notes |
|------|------|----------------|--------|-------|
| `--workspace-folder <PATH>` | Required (one of three selectors) | Via global CLI flag | 🔵 | Works, but selector validation missing |
| `--config <PATH>` | Optional | Via global CLI flag | 🔵 | Works |
| `--override-config <PATH>` | Optional | Via global CLI flag | 🔵 | Works |
| `--container-id <ID>` | Optional (selector) | Implemented | ✅ | Accepted but not yet used for container metadata |
| `--id-label <name=value>` | Optional repeatable (selector) | Implemented | ✅ | Accepted but not yet used for container selection |
| `--docker-path <PATH>` | Optional (default `docker`) | Via global CLI flag | 🔵 | Works |
| `--docker-compose-path <PATH>` | Optional | Via global CLI flag | 🔵 | Works |
| `--mount-workspace-git-root` | Optional boolean (default true) | Implemented | ✅ | Flag accepted and passed to config loader |
| `--log-level` | Optional (info\|debug\|trace) | Via global CLI flag | 🔵 | Works |
| `--log-format` | Optional (text\|json) | Via global CLI flag | 🔵 | Works |
| `--terminal-columns <N>` | Optional (paired) | Via global CLI flag | 🔵 | Works |
| `--terminal-rows <N>` | Optional (paired) | Via global CLI flag | 🔵 | Works |
| `--include-merged-configuration` | Optional boolean | Implemented | ✅ | **But wrong semantics** |
| `--include-features-configuration` | Optional boolean | Not implemented | ❌ | **CRITICAL GAP** |
| `--additional-features <JSON>` | Optional | Not implemented | ❌ | Needed for feature workflows |
| `--skip-feature-auto-mapping` | Optional hidden boolean | Not implemented | ❌ | Testing flag |
| `--user-data-folder <PATH>` | Accepted but unused | Not implemented | ❌ | Low priority |

**CLI Flags Score: 12/17 implemented (71%)**

---

## 2. Argument Validation Matrix

| Validation Rule | Spec Requirement | Implementation | Status |
|-----------------|------------------|----------------|--------|
| At least one selector | `--container-id`, `--id-label`, OR `--workspace-folder` required | Not enforced | ❌ |
| `--id-label` format | Must match `<name>=<value>` with non-empty parts | Not validated | ❌ |
| Terminal dimensions paired | Both or neither of `--terminal-columns`/`--terminal-rows` | Not applicable (flags missing) | ❌ |
| `--additional-features` JSON | Must parse as valid JSON object | Not applicable (flag missing) | ❌ |
| Config filename (strict) | For this command, not strictly enforced | Not validated | 🟡 |

**Validation Score: 0/5 implemented (0%)**

---

## 3. Configuration Resolution Matrix

| Feature | Spec Requirement | Implementation | Status |
|---------|------------------|----------------|--------|
| **Discovery** |
| Build CLI host | Create host with platform/env | Not explicit | 🟡 |
| Compute workspace | From `--workspace-folder` | Yes, uses arg | ✅ |
| Determine config path | Multi-source (explicit, discovered, override) | Yes, via `ConfigLoader::discover_config` | ✅ |
| Read config file | JSONC parsing with comments | Yes, via json5 crate | ✅ |
| Normalize old properties | `containerEnv` → `remoteEnv` | Yes, in ConfigLoader | ✅ |
| Handle missing config | Error with path | Yes, `ConfigError::NotFound` | ✅ |
| Empty config support | When only container flags | No | ❌ |
| **Substitution** |
| Pre-container | `${env:VAR}`, `${localEnv:VAR}`, `${localWorkspaceFolder}` | Yes, via `SubstitutionContext` | ✅ |
| Before-container | `${devcontainerId}` from id-labels | No | ❌ |
| Container | `${containerEnv:VAR}`, `${containerWorkspaceFolder}` | No | ❌ |
| Default values | `${localEnv:VAR:default}` | Unclear | 🟡 |
| **Features** |
| Compute when flag set | `--include-features-configuration` | No | ❌ |
| Compute for merged (no container) | Implicit requirement | No | ❌ |
| Additional features | Merge from `--additional-features` | No | ❌ |
| Auto-mapping toggle | `--skip-feature-auto-mapping` | No | ❌ |
| **Merge** |
| Container metadata read | `getImageMetadataFromContainer` | No | ❌ |
| Container substitution | Apply to metadata | No | ❌ |
| Feature metadata derive | When no container | No | ❌ |
| Merge algorithm | `mergeConfiguration(base, metadata)` | Wrong: merges override files | 🟡 |
| RemoteEnv merge | Last-wins per key | Unclear | 🟡 |
| Mounts deduplication | By target | Unclear | 🟡 |
| Lifecycle merge | Arrays/commands | Unclear | 🟡 |
| Host requirements merge | Structural | Unclear | 🟡 |

**Config Resolution Score: 7/24 implemented (29%)**

---

## 4. External System Interactions Matrix

| System | Operation | Spec Requirement | Implementation | Status |
|--------|-----------|------------------|----------------|--------|
| **Docker** |
| | Find container by ID | `findContainerAndIdLabels` with `--container-id` | No | ❌ |
| | Find container by labels | `findContainerAndIdLabels` with `--id-label` | No | ❌ |
| | Infer container | From workspace id-labels | No | ❌ |
| | Inspect container | `docker inspect <id>` for metadata | No | ❌ |
| | Read environment | Extract `Config.Env` | No | ❌ |
| | Read labels | Extract metadata labels | No | ❌ |
| | Handle unavailable | Error with diagnostic | No | ❌ |
| **OCI Registry** |
| | Contact registry | Not required by this command | N/A | ⏭️ |
| **File System** |
| | Read config file | JSONC with comments | Yes | ✅ |
| | Read override file | When provided | Yes | ✅ |
| | Cross-platform paths | POSIX, Win32, WSL2 | Yes (PathBuf) | ✅ |
| | Resolve symlinks | Where applicable | Assumed yes | 🟡 |
| | Handle permissions | Read-only sufficient | Yes | ✅ |

**External Systems Score: 4/11 implemented (36%)**

---

## 5. Output Structure Matrix

| Output Field | Spec Type | Spec Requirement | Implementation | Status |
|--------------|-----------|------------------|----------------|--------|
| `configuration` | DevContainerConfig | Always present (substituted) | Only when not merged | 🟡 |
| `workspace` | WorkspaceConfig? | Optional (workspaceFolder, mounts, paths) | Not present | ❌ |
| `featuresConfiguration` | FeaturesConfig? | When requested or needed | Not present | ❌ |
| `mergedConfiguration` | MergedDevContainerConfig? | When `--include-merged-configuration` | Present but wrong | 🟡 |

**Output Structure Score: 0/4 correct (0%)**

### Output Semantics Issues

| Issue | Spec Requirement | Current Behavior | Status |
|-------|------------------|------------------|--------|
| Always output `configuration` | Yes | Only outputs one of config OR merged | ❌ |
| Additive fields | All fields can coexist | Mutually exclusive | ❌ |
| Workspace info | Always compute | Never output | ❌ |
| Features info | When requested | Never output | ❌ |
| Merged semantics | Base + metadata | Base + override files | ❌ |

**Output Semantics Score: 0/5 correct (0%)**

---

## 6. Error Handling Matrix

| Error Category | Error Type | Spec Message | Implementation | Status |
|----------------|-----------|--------------|----------------|--------|
| **User Errors** |
| | Missing selector | "Missing required argument: One of --container-id, --id-label or --workspace-folder is required." | Not checked | ❌ |
| | Invalid id-label | "Unmatched argument format: id-label must match <name>=<value>." | N/A (flag missing) | ❌ |
| | Config not found | Includes resolved path | Yes, ConfigError::NotFound | ✅ |
| | Malformed JSON (config) | Parse error with details | Yes, propagated | ✅ |
| | Malformed JSON (additional-features) | Parse error | N/A (flag missing) | ❌ |
| **System Errors** |
| | Docker unavailable | Exit 1 with message | N/A (no Docker integration) | ❌ |
| | Docker inspect failure | Exit 1 with diagnostic | N/A (no Docker integration) | ❌ |
| | Filesystem read | Exit 1 with details | Yes | ✅ |
| **Config Errors** |
| | Non-object root | Validation message | Yes (JSON parser) | ✅ |
| | Compose without workspace | Error from helpers | Unclear | 🟡 |
| **Exit Codes** |
| | Success | Exit 0, JSON to stdout | Yes | ✅ |
| | Failure | Exit 1, message to stderr, no stdout | Yes | ✅ |

**Error Handling Score: 6/12 implemented (50%)**

---

## 7. Testing Matrix

| Test Case | Spec Requirement | Implementation | Status |
|-----------|------------------|----------------|--------|
| Requires selector | Error when none provided | Not implemented | ❌ |
| ID label validation | Error on invalid format | Not implemented | ❌ |
| Reads config from workspace | Basic success case | Yes | ✅ |
| Variable substitution | Expands vars | Yes | ✅ |
| Override config | Merges override | Yes | ✅ |
| Secrets integration | Loads from files | Yes | ✅ |
| Config not found | Error case | Yes | ✅ |
| Include features only | `--include-features-configuration` | Not implemented | ❌ |
| Merged (no container) | Feature-derived metadata | Not implemented | ❌ |
| Merged (with container) | Container metadata | Not implemented | ❌ |
| Additional features | Merge extras | Not implemented | ❌ |

**Test Coverage Score: 5/11 test cases passing (45%)**

---

## 8. Cross-Platform Support Matrix

| Platform | Spec | Implementation | Status |
|----------|------|----------------|--------|
| Linux | Full support | Works | ✅ |
| macOS | Full support | Should work (needs testing) | 🟡 |
| Windows | Full support | Should work (needs testing) | 🟡 |
| WSL2 | Full support | Should work (needs testing) | 🟡 |
| Path handling | Cross-platform via cliHost | Uses PathBuf | 🟡 |
| Docker socket | Platform-specific | Not implemented yet | ❌ |

**Cross-Platform Score: 1/6 verified (17%)**

---

## 9. Overall Compliance Summary

| Category | Score | Status |
|----------|-------|--------|
| CLI Flags | 6/17 (35%) | 🔴 Critical gaps |
| Argument Validation | 0/5 (0%) | 🔴 Not implemented |
| Configuration Resolution | 7/24 (29%) | 🔴 Major gaps |
| External Systems | 4/11 (36%) | 🔴 Docker missing |
| Output Structure | 0/4 (0%) | 🔴 Non-compliant |
| Output Semantics | 0/5 (0%) | 🔴 Wrong behavior |
| Error Handling | 6/12 (50%) | 🟡 Partial |
| Testing | 5/11 (45%) | 🟡 Basic coverage |
| Cross-Platform | 1/6 (17%) | 🔴 Needs verification |
| **OVERALL AVERAGE** | **~25%** | 🔴 **Non-compliant** |

---

## 10. Priority Gap Categories

### 🔴 CRITICAL (Blocks Spec Compliance)
- Container integration (flags, Docker, substitution)
- Feature resolution and output
- Output structure (wrapper object, all fields)
- Merge algorithm (metadata, not override files)
- Selector validation

### 🟠 HIGH (Important Functionality)
- Docker tooling configuration
- Additional features support
- Workspace config output
- Remaining substitution rules

### 🟡 MEDIUM (Completeness)
- Terminal dimension flags
- Mount workspace git root flag
- User data folder flag
- Cross-platform testing

### 🟢 LOW (Polish)
- Skip feature auto-mapping flag
- Expanded error messages
- Additional test cases

---

## 11. Implementation Roadmap

**Phase 1 (CRITICAL)** - ~5 days
- ✅ Gap analysis complete
- ⬜ Add container selection flags
- ⬜ Implement Docker integration
- ⬜ Add feature resolution flag and logic
- ⬜ Fix output structure
- ⬜ Correct merge algorithm
- ⬜ Add selector validation

**Phase 2 (HIGH)** - ~2 days
- ⬜ Add Docker tooling flags
- ⬜ Implement workspace output
- ⬜ Add additional features support
- ⬜ Expand test coverage

**Phase 3 (MEDIUM/LOW)** - ~2 days
- ⬜ Add remaining flags
- ⬜ Cross-platform testing
- ⬜ Documentation
- ⬜ Examples

**Estimated Total:** 9 days to full spec compliance

---

**Generated:** October 13, 2025  
**Version:** 1.0
