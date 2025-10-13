# Templates Subcommand Implementation Status

**Quick Reference**: This document provides a high-level overview of implementation compliance with the specification. For detailed analysis, see [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md).

**Last Updated**: October 13, 2025  
**Overall Compliance**: ~40%  

---

## 🚨 Critical Issues (Must Fix for v1.0)

| Issue | Impact | Spec Reference |
|-------|--------|----------------|
| **CLI interface completely mismatched** | Users cannot invoke commands as specified | SPEC.md §2 |
| **Missing `--features` injection** | Cannot apply features with templates | SPEC.md §2, §5 |
| **No `${templateOption:KEY}` substitution** | Template parameterization broken | SPEC.md §4, §5 |
| **`metadata` uses local paths, not OCI refs** | Cannot query published template metadata | SPEC.md §2 |
| **No semantic versioning in publish** | Cannot follow registry best practices | SPEC.md §5 (publish) |
| **No collection support in publish** | Cannot publish multi-template repos | SPEC.md §5 (publish) |
| **Missing structured JSON output for `apply`** | Cannot parse results programmatically | SPEC.md §10 |

---

## 📊 Compliance by Subcommand

### `templates apply`

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Flag: `--template-id` | ✅ Required | ❌ Positional arg | **FAIL** |
| Flag: `--workspace-folder` | ✅ Optional | ❌ Wrong name (`--output`) | **FAIL** |
| Flag: `--template-args` | ✅ JSON object | ❌ key=value strings | **FAIL** |
| Flag: `--features` | ✅ JSON array | ❌ Not present | **FAIL** |
| Flag: `--omit-paths` | ✅ JSON array | ❌ Not present | **FAIL** |
| Output: `{ files: [] }` | ✅ Required | ❌ Logs only | **FAIL** |
| Local templates | ✅ Support | ✅ Supported | **PASS** |
| Registry templates | ✅ Support | ✅ Supported | **PASS** |
| Variable substitution | ✅ `${templateOption:...}` | ⚠️ General vars only | **PARTIAL** |
| Dry run | ⚠️ Not specified | ✅ Implemented | **EXTENSION** |

**Status**: 🔴 **Non-compliant** - Major refactoring required

---

### `templates publish`

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Positional: `target` | ✅ Optional (default `.`) | ⚠️ Called `path` (required) | **PARTIAL** |
| Flag: `--namespace` | ✅ Required | ❌ Not present | **FAIL** |
| Flag: `--registry` | ✅ Optional (default `ghcr.io`) | ⚠️ Required | **PARTIAL** |
| Collection support | ✅ Required | ❌ Single template only | **FAIL** |
| Semantic version tags | ✅ Required | ❌ Not implemented | **FAIL** |
| Collection metadata | ✅ Required | ❌ Not implemented | **FAIL** |
| Output: Map of results | ✅ Required | ❌ Single result | **FAIL** |
| Dry run | ⚠️ Not specified | ✅ Implemented | **EXTENSION** |
| Manifest annotations | ✅ Required | ⚠️ Needs verification | **UNKNOWN** |

**Status**: 🔴 **Non-compliant** - Major refactoring required

---

### `templates metadata`

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Input: OCI reference | ✅ Required | ❌ Local path | **FAIL** |
| Fetch from registry | ✅ Required | ❌ Read local file | **FAIL** |
| Parse manifest annotation | ✅ Required | ❌ N/A (no registry) | **FAIL** |
| Return `{}` on missing | ✅ Required | ❌ Error | **FAIL** |
| Output: Template metadata JSON | ✅ Required | ✅ JSON output | **PASS** |

**Status**: 🔴 **Non-compliant** - Complete rewrite required

---

### `templates generate-docs`

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Flag: `--project-folder` | ✅ Optional | ⚠️ Positional `path` | **PARTIAL** |
| Flag: `--github-owner` | ✅ Optional | ❌ Not present | **FAIL** |
| Flag: `--github-repo` | ✅ Optional | ❌ Not present | **FAIL** |
| Output: Generated docs | ✅ Required | ✅ Implemented | **PASS** |
| README fragment | ⚠️ Implied | ✅ Implemented | **PASS** |

**Status**: 🟡 **Partially compliant** - Minor additions needed

---

### `templates pull`

| Feature | Spec | Implementation | Status |
|---------|------|----------------|--------|
| Command existence | ⚠️ **Not in spec** | ✅ Implemented | **EXTENSION** |
| Registry fetch | N/A | ✅ Works | **EXTENSION** |
| JSON output | N/A | ✅ Configurable | **EXTENSION** |

**Status**: ⚪ **Non-standard extension** - Document as implementation-specific

---

## 🔧 Required Refactoring Summary

### Breaking Changes (v1.0 Target)

1. **Apply command**: Rename flags, change option format to JSON
2. **Publish command**: Add `--namespace`, change output format
3. **Metadata command**: Accept OCI refs instead of local paths

### New Features Required

1. Feature injection (`--features` flag in `apply`)
2. Template option substitution (`${templateOption:KEY}`)
3. Omit paths (`--omit-paths` flag)
4. Semantic versioning (publish)
5. Collection support (publish)
6. Structured JSON output (apply)

### Non-Breaking Enhancements

1. Add `--github-owner` and `--github-repo` to `generate-docs`
2. Verify manifest annotation handling in publish
3. Add comprehensive integration tests

---

## 📈 Implementation Roadmap

### Phase 1: Critical Fixes (Week 1)
- [ ] Refactor `apply` CLI interface
- [ ] Add `${templateOption:KEY}` substitution
- [ ] Add `--features` support
- [ ] Add structured JSON output

### Phase 2: Registry Operations (Week 2)
- [ ] Rewrite `metadata` to use OCI refs
- [ ] Add semantic versioning to `publish`
- [ ] Add collection support to `publish`
- [ ] Fix `publish` output format

### Phase 3: Polish & Test (Week 3)
- [ ] Add missing flags to `generate-docs`
- [ ] Add `--omit-paths` support
- [ ] Write spec-compliant integration tests
- [ ] Update documentation and examples

### Phase 4: Release Preparation
- [ ] Write migration guide
- [ ] Update CLI help text
- [ ] Add deprecation warnings (if gradual migration)
- [ ] Version bump to v1.0

---

## 📝 Developer Notes

### What Works Well

1. ✅ Local template application (files, directories)
2. ✅ Variable substitution (built-in variables)
3. ✅ Dry-run mode
4. ✅ Overwrite protection
5. ✅ Basic OCI registry interaction
6. ✅ Documentation generation

### Major Pain Points

1. ❌ CLI interface doesn't match spec at all
2. ❌ No feature injection
3. ❌ Template options not properly substituted
4. ❌ Publish workflow incomplete
5. ❌ Metadata command wrong scope

### Quick Wins

1. Add `--github-owner` and `--github-repo` to `generate-docs`
2. Add `--omit-paths` support (extend existing skip logic)
3. Add structured JSON output to `apply`

### Hard Problems

1. Semantic version tag computation and conflict resolution
2. Collection metadata aggregation
3. OCI manifest annotation handling
4. Feature injection with JSONC preservation

---

## 🧪 Test Coverage Status

### Existing Tests
- ✅ Metadata parsing (local)
- ✅ Publish dry-run
- ✅ Generate-docs output
- ✅ CLI help text

### Missing Tests
- ❌ Apply with JSON template args
- ❌ Apply with features
- ❌ Apply with omit paths
- ❌ Apply JSON output
- ❌ Metadata from registry
- ❌ Publish semantic tags
- ❌ Publish collections
- ❌ Template option substitution

**Test Coverage**: ~30% of spec requirements

---

## 📚 Related Documents

- [SPEC.md](./SPEC.md) - Full specification
- [DATA-STRUCTURES.md](./DATA-STRUCTURES.md) - Data structure definitions
- [DIAGRAMS.md](./DIAGRAMS.md) - Sequence and flow diagrams
- [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md) - Detailed gap analysis

---

## 🎯 Success Criteria for v1.0

- [ ] All CLI flags match specification
- [ ] All output formats match specification
- [ ] Template option substitution works with `${templateOption:...}`
- [ ] Feature injection works
- [ ] Semantic versioning works
- [ ] Collection publishing works
- [ ] All spec-defined test cases pass
- [ ] Documentation updated
- [ ] Migration guide published

**Target Date**: TBD (estimated 3 weeks effort)
