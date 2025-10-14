# Features Plan Implementation Gap Analysis

**Analysis Date**: October 13, 2025  
**Specification Version**: `docs/subcommand-specs/features-plan/SPEC.md`  
**Implementation Location**: `crates/deacon/src/commands/features.rs` (`execute_features_plan`)

## Executive Summary

The `features plan` subcommand is **partially implemented** with core functionality in place but several gaps exist relative to the official specification. The implementation successfully handles configuration loading, feature resolution, dependency resolution, and JSON/text output, but has issues with validation, error handling granularity, and missing specification-defined behaviors.

**Overall Compliance**: ~75% (Implemented features work correctly but incomplete coverage)

---

## 1. Command-Line Interface Compliance

### ✅ Implemented Correctly
- Basic command syntax: `deacon features plan`
- `--json` flag (defaults to `true` ✅)
- `--additional-features <JSON>` flag

### ⚠️ Partially Implemented
- **`--json` default behavior**: Specification says "default true" and implementation has `default_value_t = true`, which is correct.

### ❌ Missing or Incorrect
- **Argument validation**: Specification requires "Parse errors are fatal" for `--additional-features`. 
  - **Current behavior**: JSON parsing errors are handled but not validated as "must be a JSON object (map)" before use. The code attempts to merge and will fail during merge if invalid, but doesn't pre-validate the structure.
  - **Expected**: Explicit validation that `--additional-features` is a JSON object/map before any processing.

---

## 2. Input Processing Pipeline

### ✅ Implemented Correctly
- Parses command arguments correctly
- Default values applied as specified

### ❌ Missing
- **Explicit validation step**: Specification shows `parse_json_map` should validate structure
  - Current implementation parses JSON but doesn't explicitly validate it's a map/object before merging

---

## 3. Configuration Resolution

### ✅ Implemented Correctly
- Sources handled in correct precedence:
  1. CLI additional features (`--additional-features`)
  2. `devcontainer.json` (explicit `--config` or discovered)
  3. Default empty map
- Merge algorithm implemented via `FeatureMerger::merge_features`
- Empty features map returns empty plan (as specified)

### ⚠️ Partially Implemented
- **Merge semantics**: Specification states "additive merge without overwrite by default"
  - Current implementation uses `prefer_cli_features: false` which is correct
  - However, the merge behavior needs verification against spec's "last write wins per TS implementation pattern" guidance

### ❌ Missing
- **Variable Substitution**: Specification states "Not required for planning; feature IDs are treated as opaque strings. Option values are passed through."
  - This appears to be correctly NOT implemented (as intended), but there's no explicit comment or documentation confirming this design decision
  - **Recommendation**: Add comment confirming variable substitution is intentionally skipped per spec

---

## 4. Core Execution Logic

### ✅ Implemented Correctly
- **Phase 1: Initialization** ✅
  - Workspace determination
  - Config loading (explicit path > discovery > default)
  - Feature map merging

- **Phase 2: Pre-execution validation** ✅
  - Empty features map check
  - Returns empty plan correctly

- **Phase 3: Main execution** ✅
  - OCI fetcher creation
  - Feature metadata fetching from registries
  - Option normalization (Boolean and String types)
  - Dependency resolution via `FeatureDependencyResolver`
  - Installation plan creation
  - Graph building (union of `installsAfter` and `dependsOn`)

- **Phase 4: Post-execution** ✅
  - JSON/text output via `output_plan_result`

### ⚠️ Partially Implemented
- **Option normalization**: Current implementation only handles `Boolean` and `String` types
  - Spec doesn't specify which types should be supported
  - **Gap**: No handling for `Number` types or arrays (though these may not be needed per spec)
  - Current implementation has `_ => None, // Skip other types` which silently drops unsupported types

### ❌ Missing
- **Override order handling**: Specification mentions `override_order=config.overrideFeatureInstallOrder`
  - This IS implemented: `let override_order = config.override_feature_install_order.clone();`
  - However, there's **no documentation** in code about what this does or how it relates to the spec ✅ (actually implemented)

---

## 5. State Management

### ✅ Implemented Correctly
- Read-only operation (no state modifications)
- No persistent state

---

## 6. External System Interactions

### OCI Registries

#### ✅ Implemented Correctly
- Fetches feature metadata from registries
- Downloads `dependsOn` and `installsAfter` information
- Uses `default_fetcher()` for OCI client

#### ⚠️ Partially Implemented
- **Caching**: Specification says "Fetcher may cache blobs locally (implementation detail of OCI client)"
  - Current implementation delegates to `default_fetcher()` 
  - Cache behavior depends on OCI client implementation (not verified in this analysis)

#### ❌ Missing
- **Error messages**: Specification requires "surface feature ID and error" for OCI fetch failures
  - Current implementation: `"Failed to fetch feature '{}': {}"` ✅ Actually correctly implemented!

### File System

#### ✅ Implemented Correctly
- Reads config from workspace
- No writes (outputs to stdout only)

---

## 7. Data Flow

### ✅ Implemented Correctly
- Follows spec data flow:
  1. Config + CLI merge
  2. Feature IDs extraction
  3. Metadata fetch (OCI)
  4. Plan resolution (order)
  5. Graph derivation

---

## 8. Error Handling Strategy

### ✅ Implemented Correctly
- **System Errors**: OCI fetch failures include feature ID and error ✅
- **Configuration Errors**: Circular dependencies detected by resolver (delegated to `FeatureDependencyResolver`)

### ⚠️ Partially Implemented
- **User Errors**: JSON parse failures for `--additional-features`
  - Current implementation handles JSON parse errors through `FeatureMerger::merge_features`
  - **Gap**: Error message may not be clear that it came from `--additional-features` specifically

### ❌ Missing
- **Error context**: No explicit test that circular dependency errors include "details" as spec requires
  - The resolver likely does this, but needs verification
  - **Recommendation**: Add integration test for circular dependency error message format

---

## 9. Output Specifications

### ✅ Implemented Correctly
- **JSON Mode**: Outputs `{ "order": [...], "graph": {...} }` ✅
- **Text Mode**: Human-readable header, order list, and pretty-printed graph JSON ✅
- **Exit Codes**: Returns `Result<()>` which becomes 0 on success, 1 on failure ✅
- **Graph structure**: Spec shows `"graph": { "<id>": ["dep1", ...] }`
  - Implementation uses `build_graph_representation` which combines `installsAfter` and `dependsOn`
  - This is correct per spec's design decision: "combines installsAfter and dependsOn"
  - Graph values are dependencies (what this feature depends on), not dependents
  - ✅ **Verified**: Graph edges correctly show dependencies (Option A)
  - ✅ **Documented**: Comprehensive rustdoc explains direction and behavior
  - ✅ **Tested**: Six new tests verify graph structure, union, deduplication, and ordering

---

## 10. Performance Considerations

### ⚠️ Partially Implemented
- **Metadata fetches**: Specification says "serialized; could parallelize with concurrency limit"
  - Current implementation fetches serially in a loop
  - **Gap**: No parallelization or concurrency
  - **Recommendation**: Low priority for initial implementation, but should be tracked as future enhancement

### ✅ Implemented Correctly
- **Resolver complexity**: Uses efficient topological sort (delegated to `FeatureDependencyResolver`)

---

## 11. Security Considerations

### ✅ Implemented Correctly
- Remote fetch uses OCI client (presumably with TLS)
- No secrets printed in output

### ❌ Missing
- **Registry authentication**: Specification mentions "rely on TLS and registry auth"
  - Current implementation uses `default_fetcher()` which may or may not handle auth
  - **Gap**: No explicit verification that auth is supported
  - **Recommendation**: Document authentication behavior or add note about registry auth support

---

## 12. Cross-Platform Behavior

### ✅ Implemented Correctly
- OS-agnostic implementation
- Network I/O and stdout only
- No platform-specific code in the plan command

---

## 13. Edge Cases and Corner Cases

### ✅ Implemented Correctly
- **Empty features**: Returns empty order and graph ✅
- **Features without dependencies**: Handled correctly in graph building

### ⚠️ Partially Implemented
- **Mixed local/remote features**: Specification says "future enhancement: plan currently expects registry refs"
  - Current implementation only handles registry references via `parse_registry_reference`
  - **Gap**: No explicit error message or documentation when local features are used
  - Local features will fail during `parse_registry_reference` with potentially unclear error
  - **Recommendation**: Add explicit check and error message for local feature paths

### ❌ Missing Tests
- No test for simple chain (A → B installsAfter)
- No test for `dependsOn` cycles with error verification
- No test for additional-features merge verification

---

## 14. Testing Strategy

### ✅ Implemented Tests
1. ✅ `test_features_plan_empty_config` - Empty features test
2. ✅ `test_features_plan_with_additional_features` - Additional features (expects error for invalid refs)
3. ✅ `test_output_plan_result_json` - JSON output format
4. ✅ `test_output_plan_result_text` - Text output format
5. ✅ `test_graph_structure_no_dependencies` - Feature with no dependencies has empty array
6. ✅ `test_graph_structure_simple_chain` - Simple chain A->B verification
7. ✅ `test_graph_structure_combined_installs_after_and_depends_on` - Union of both fields
8. ✅ `test_graph_structure_union_deduplication` - Deduplication of duplicate dependencies
9. ✅ `test_graph_structure_fan_in` - Fan-in pattern (C depends on A and B)
10. ✅ `test_graph_structure_deterministic_ordering` - Lexicographic ordering verification

### ⚠️ Partially Implemented Tests (from spec)
1. ✅ **Graph structure test**: Verify graph contains correct edges ← NOW IMPLEMENTED
2. ⚠️ **Simple chain test**: "A -> B installsAfter; expect order [A, B]" - Graph verified; order tested separately
3. ❌ **Cycle detection test**: "dependsOn cycles: expect error" - Implemented in core, not in features.rs
4. ❌ **Additional features merge test**: "ensure CLI additions included in order/graph" - Partially tested
5. ❌ **Override order test**: Verify `overrideFeatureInstallOrder` behavior - Tested in core
6. ❌ **Option normalization test**: Verify different option value types
7. ❌ **Error message format test**: Verify circular dependency error includes details
8. ❌ **Local feature rejection test**: Verify clear error for local feature paths - EXISTS as test_features_plan_rejects_local_paths

---

## 15. Migration Notes

### ✅ Documented Correctly
- Specification notes: "Not present in TS CLI; aligns with TS dependency graph computation"
- No migration concerns as this is a new feature

---

## 16. Design Decision: Graph Content

### ✅ Implemented Correctly
- Graph combines `installsAfter` and `dependsOn` as per spec design decision
- Uses `BTreeSet` for deterministic ordering
- Produces unified adjacency list

### ✅ Verified and Documented
- **Edge direction**: Confirmed graph shows dependencies correctly
  - Current code: `graph[featureId] = [dep1, dep2]` (dependencies of featureId)
  - Spec example matches this interpretation ✅
  - Comprehensive rustdoc added explaining direction and behavior
  - Test coverage added to verify graph structure

---

## 17. Code Quality Issues

### Style and Documentation
1. ✅ **Function documentation added**: `build_graph_representation` now has comprehensive rustdoc
   - Explains graph direction (dependencies, not dependents)
   - Documents union behavior of installsAfter and dependsOn
   - Includes example JSON structure
   - References specification documents
2. ❌ **Missing design rationale**: No comment explaining why variable substitution is skipped
3. ⚠️ **Mock functions in non-test code**: `create_mock_resolved_feature*` are only `#[cfg(test)]` ✅ Actually correct
4. ⚠️ **Silent type dropping**: Option value conversion silently skips unsupported types with comment, but should this be logged?

### Maintainability
1. ⚠️ **Error context**: Could improve error messages to be more specific about which phase failed
2. ⚠️ **Tracing**: Good use of spans, but could add more structured fields (feature count, etc.)

---

## Summary of Gaps

### Critical (Blocks Spec Compliance) 🔴
1. **Missing explicit validation** that `--additional-features` is a JSON object/map before processing
2. **Missing tests** for core spec requirements (chains, cycles, merge behavior)
3. **No error handling** for local feature paths (will fail with unclear parse error)

### Important (Reduces Quality/Usability) 🟡
1. **No parallelization** of OCI fetches (performance gap)
2. **Incomplete option type support** (only Boolean and String, silently drops others)
3. **Missing documentation** on several functions and design decisions
4. **No explicit auth documentation** for registry access
5. **Error messages** could be more specific about source of failure

### Minor (Polish/Enhancement) 🟢
1. **Test coverage** gaps for edge cases
2. **Code documentation** could be improved
3. **Observability**: Could add more structured tracing fields

---

## Recommendations (Prioritized)

### Priority 1: Spec Compliance
1. Add explicit validation that `--additional-features` is a JSON object before merging
2. Add clear error message for local feature paths with guidance to use registry references
3. Implement missing tests from specification (chains, cycles, merge verification)
4. Verify and document graph edge direction matches specification intent

### Priority 2: Error Handling
1. Improve error context to specify which phase failed (fetch, resolve, etc.)
2. Add test for circular dependency error message format
3. Ensure all error messages include actionable information

### Priority 3: Documentation
1. Add rustdoc to `build_graph_representation` explaining graph structure
2. Add comment explaining variable substitution is intentionally skipped per spec
3. Document authentication behavior or add note about registry auth requirements
4. Document option type support and rationale for silently skipping unsupported types

### Priority 4: Performance
1. Consider parallelizing OCI fetches with concurrency limit (future enhancement)

### Priority 5: Polish
1. Add structured fields to tracing spans (feature count, fetch time, etc.)
2. Consider supporting Number type for option values
3. Improve test coverage for edge cases

---

## Conclusion

The `features plan` implementation provides a solid foundation and implements the core workflow correctly. The main gaps are in:
1. **Validation** (input validation for CLI arguments)
2. **Error handling** (local features, error context)
3. **Testing** (missing spec-defined test cases)
4. **Documentation** (code comments and design rationale)

The implementation correctly handles the critical path: configuration loading, feature resolution, dependency graph construction, and JSON/text output. With the recommended fixes, compliance would increase to 95%+.

**Estimated Effort to Full Compliance**: 2-3 days for Priority 1 items, +1 day for Priority 2 items.
