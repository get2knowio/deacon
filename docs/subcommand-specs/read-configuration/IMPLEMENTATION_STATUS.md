# Read-Configuration Implementation Status

**Last Updated:** October 13, 2025

## Quick Status Overview

| Category | Status | Compliance |
|----------|--------|------------|
| **Overall Implementation** | 🟡 Partial | ~25% |
| **CLI Flags** | 🔴 Critical Gaps | ~21% |
| **Config Resolution** | 🟡 Basic Working | ~50% |
| **Container Integration** | 🔴 Not Implemented | 0% |
| **Feature Resolution** | 🔴 Not Implemented | 0% |
| **Output Structure** | 🔴 Non-Compliant | 0% |
| **Error Handling** | 🟡 Partial | ~60% |
| **Testing** | 🟡 Basic Coverage | ~33% |

**Legend:** 🟢 Complete | 🟡 Partial | 🔴 Missing/Critical Gap

---

## What Works Today ✅

1. **Basic configuration reading** from workspace
2. **Override file merging** (base + override configs)
3. **Variable substitution** (host environment variables)
4. **JSONC parsing** (comments, trailing commas)
5. **Error reporting** for missing/invalid configs
6. **Secrets loading** from files
7. **JSON output** to stdout

---

## What's Missing (Critical) ❌

1. **Container selection** (`--container-id`, `--id-label`)
2. **Container metadata reading** (inspect, environment)
3. **Feature resolution** (`--include-features-configuration`)
4. **Proper merged configuration** (base + image metadata, not base + override)
5. **Output structure** (missing `workspace`, `featuresConfiguration` fields)
6. **Docker CLI configuration** (`--docker-path`, `--docker-compose-path`)
7. **Additional features** (`--additional-features` JSON)
8. **Required argument validation** (at least one selector)

---

## Top 5 Priorities for Spec Compliance

### 1. Container Integration 🔴 CRITICAL
**Impact:** Blocks all container-based workflows

**Required:**
- Add `--container-id` and `--id-label` flags
- Implement `findContainerAndIdLabels` logic
- Add Docker inspect calls
- Implement container-based variable substitution (`${containerEnv:*}`)

**Estimated Effort:** 2-3 days

### 2. Feature Resolution 🔴 CRITICAL
**Impact:** Cannot output feature information

**Required:**
- Add `--include-features-configuration` flag
- Implement feature resolution logic
- Output `featuresConfiguration` field with `featureSets` array
- Support `--additional-features` JSON

**Estimated Effort:** 2 days

### 3. Fix Output Structure 🔴 CRITICAL
**Impact:** Output is non-spec-compliant

**Required:**
- Always output `configuration` field (even when merged)
- Add `workspace` field (workspaceFolder, workspaceMount, etc.)
- Add `featuresConfiguration` field (when requested)
- Keep `mergedConfiguration` as separate field (not mutually exclusive)

**Estimated Effort:** 1 day

### 4. Fix Merge Algorithm 🔴 CRITICAL
**Impact:** Wrong merge semantics

**Current:** Merges base config + override config  
**Required:** Merge base config + image metadata (from container OR features)

**Steps:**
- When container: read metadata from container labels/env
- When no container: derive metadata from features
- Use `mergeConfiguration(base, metadata)` algorithm

**Estimated Effort:** 2 days

### 5. Add Input Validation 🔴 CRITICAL
**Impact:** Commands can fail with unclear errors

**Required:**
- Validate at least one of `--container-id`, `--id-label`, or `--workspace-folder` is provided
- Validate `--id-label` format (`<name>=<value>` with non-empty parts)
- Validate terminal dimensions are paired

**Estimated Effort:** 0.5 days

---

## Secondary Priorities (High)

6. **Docker tooling flags** - Add `--docker-path`, `--docker-compose-path`
7. **Mount workspace git root** - Add `--mount-workspace-git-root` flag
8. **Workspace config output** - Compute and output `WorkspaceConfig` structure
9. **Additional features support** - Parse and merge `--additional-features` JSON

---

## Known Issues

### Issue 1: Output Structure Incompatibility
**Problem:** Current implementation outputs either `configuration` OR `mergedConfiguration` (mutually exclusive)  
**Spec:** Should output `configuration` always, plus `mergedConfiguration` when flag is set  
**Impact:** Breaking change required to fix

### Issue 2: Merge Semantics Wrong
**Problem:** Merges override files instead of image metadata  
**Spec:** Should merge base config + image metadata (from container or features)  
**Impact:** Current "merged" output doesn't match spec semantics

### Issue 3: No Container Support
**Problem:** Cannot work with running containers at all  
**Spec:** Major use case is reading config from containers  
**Impact:** Large portion of spec functionality unavailable

---

## Test Coverage Gaps

**Implemented Tests:**
- ✅ Basic config reading
- ✅ Variable substitution
- ✅ Override config merging
- ✅ Secrets integration
- ✅ Config not found error

**Missing Tests:**
- ❌ Container selection and metadata reading
- ❌ Feature resolution
- ❌ Proper merged configuration
- ❌ Required argument validation
- ❌ ID label validation
- ❌ Additional features merging

---

## Breaking Changes Required

The following changes will break existing behavior:

1. **Output structure** - Move from direct config JSON to wrapped structure
2. **Merged semantics** - Change from override merging to metadata merging
3. **Validation** - Enforce required selector argument

**Recommendation:** Implement all breaking changes together in a single release with clear migration guide.

---

## Next Steps

1. Review and approve this gap analysis
2. Create GitHub issues for each critical item
3. Implement items 1-5 (critical path to spec compliance)
4. Update tests to match spec test suite
5. Document breaking changes and migration path

---

## Full Analysis

See [IMPLEMENTATION_GAP_ANALYSIS.md](./IMPLEMENTATION_GAP_ANALYSIS.md) for detailed analysis.
