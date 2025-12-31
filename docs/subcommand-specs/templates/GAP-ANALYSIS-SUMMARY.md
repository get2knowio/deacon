# Templates Subcommand - Gap Analysis Summary

**Generated**: October 13, 2025  
**Specification Source**: `/workspaces/deacon/docs/subcommand-specs/templates/`  
**Implementation Source**: `/workspaces/deacon/crates/deacon/src/commands/templates.rs`

---

## 📊 Overall Assessment

**Compliance Score**: ~40%  
**Severity**: 🔴 **Critical** - Major refactoring required  
**Recommendation**: Target for v1.0 milestone with breaking changes

### What Works
- ✅ Basic local template application (files, directories)
- ✅ Variable substitution for built-in variables
- ✅ Dry-run mode and overwrite protection
- ✅ Basic OCI registry interaction (pull, publish)
- ✅ Documentation generation

### What's Broken
- ❌ CLI interface doesn't match specification
- ❌ Missing critical features (feature injection, omit paths)
- ❌ Template option substitution pattern not supported
- ❌ Publish workflow incomplete (no semantic versioning, collections)
- ❌ Metadata command uses wrong input type
- ❌ Output formats don't match specification

---

## 🚨 Top 7 Critical Issues

### 1. CLI Interface Mismatch ⚠️ CRITICAL
**What spec says**: `templates apply --template-id <oci-ref> --template-args '{"key":"value"}'`  
**What we have**: `templates apply <template> --option key=value`  
**Impact**: Command is not invokable as specified  
**Effort**: 2-3 days

### 2. Missing Feature Injection ⚠️ CRITICAL
**What spec says**: `--features '[{"id":"...","options":{}}]'` should inject into devcontainer.json  
**What we have**: No `--features` flag at all  
**Impact**: Cannot apply features alongside templates  
**Effort**: 2-3 days

### 3. Template Option Substitution Pattern ⚠️ CRITICAL
**What spec says**: Files should use `${templateOption:KEY}` for user options  
**What we have**: Only general `${localWorkspaceFolder}` style variables  
**Impact**: Template parameterization doesn't work  
**Effort**: 1-2 days

### 4. Metadata Command Wrong Scope ⚠️ CRITICAL
**What spec says**: `templates metadata <oci-ref>` fetches from registry  
**What we have**: `templates metadata <local-path>` reads local file  
**Impact**: Cannot query published template metadata  
**Effort**: 2-3 days

### 5. Semantic Versioning Missing ⚠️ CRITICAL
**What spec says**: Publish should push tags: `1`, `1.2`, `1.2.3`, `latest`  
**What we have**: Single tag only  
**Impact**: Cannot follow registry best practices  
**Effort**: 2-3 days

### 6. Collection Support Missing ⚠️ CRITICAL
**What spec says**: Publish can handle `src/` with multiple templates  
**What we have**: Single template only  
**Impact**: Cannot publish multi-template repositories  
**Effort**: 3-4 days

### 7. Missing Structured JSON Output ⚠️ CRITICAL
**What spec says**: `apply` outputs `{"files":["..."]}`  
**What we have**: Logs only, no structured output  
**Impact**: Cannot parse results programmatically  
**Effort**: 1 day

---

## 📁 Generated Documentation

Three comprehensive documents have been created in `/workspaces/deacon/docs/subcommand-specs/templates/`:

### 1. IMPLEMENTATION-GAP-ANALYSIS.md (Detailed Analysis)
- **Scope**: In-depth analysis of every gap
- **Sections**: CLI interface, data structures, registry operations, testing
- **Use for**: Understanding exact discrepancies, planning implementation
- **Length**: ~600 lines with tables, code samples, recommendations

### 2. IMPLEMENTATION-STATUS.md (Quick Overview)
- **Scope**: High-level status by subcommand
- **Sections**: Compliance tables, roadmap, test coverage
- **Use for**: Sprint planning, progress tracking
- **Length**: ~200 lines with status indicators

### 3. QUICK-REFERENCE.md (Developer Cheat Sheet)
- **Scope**: At-a-glance comparisons and code snippets
- **Sections**: Command signatures, data structures, code changes needed
- **Use for**: Daily reference while implementing
- **Length**: ~300 lines with examples and checklists

---

## 🗺️ Recommended Implementation Path

### Week 1: Foundation (Critical Fixes)
**Goal**: Make CLI spec-compliant and add core features

- [ ] Refactor `TemplateCommands` enum in `cli.rs`
- [ ] Add `${templateOption:KEY}` substitution support
- [ ] Add `--features` parsing and injection
- [ ] Add structured JSON output for `apply`
- [ ] Update argument parsing throughout

**Deliverable**: Apply command matches spec

### Week 2: Registry Operations
**Goal**: Fix publish and metadata workflows

- [ ] Rewrite `metadata` to accept OCI references
- [ ] Add semantic version tag computation
- [ ] Add collection detection and handling
- [ ] Fix `publish` output format
- [ ] Generate collection metadata

**Deliverable**: Publish and metadata commands match spec

### Week 3: Polish & Release
**Goal**: Complete remaining features and prepare release

- [ ] Add `--omit-paths` support
- [ ] Add GitHub flags to `generate-docs`
- [ ] Write comprehensive integration tests
- [ ] Update all documentation and examples
- [ ] Write migration guide

**Deliverable**: v1.0 ready for release

---

## 🎯 Breaking Changes in v1.0

The following changes will break existing usage:

### Command Line Interface
1. `templates apply <template>` → `templates apply --template-id <oci-ref>`
2. `--option key=value` → `--template-args '{"key":"value"}'`
3. `--output <dir>` → `--workspace-folder <dir>`
4. `templates metadata <path>` → `templates metadata <oci-ref>`
5. `templates publish <path> --registry <url>` → `templates publish [target] --namespace <ns>`

### Output Formats
6. Apply now outputs JSON to stdout instead of logs only
7. Publish outputs map of template results instead of single result

### Behavior
8. Template files must use `${templateOption:KEY}` for user-provided options
9. Built-in variables remain `${localWorkspaceFolder}` etc.

---

## 📈 Success Metrics

Track these to measure v1.0 completion:

- [ ] All CLI flags match specification (13 flags to add/change)
- [ ] Template option substitution works (1 new pattern)
- [ ] Feature injection works (1 new capability)
- [ ] Semantic versioning works (4 tags per publish)
- [ ] Collection publishing works (N templates + metadata)
- [ ] All output formats match spec (2 formats to fix)
- [ ] 20+ new integration tests pass
- [ ] Migration guide published

---

## 🔧 Files to Modify

| File | Changes Needed | Effort |
|------|----------------|--------|
| `crates/deacon/src/cli.rs` | Refactor `TemplateCommands` enum | 1 day |
| `crates/deacon/src/commands/templates.rs` | Update all execute functions | 3 days |
| `crates/core/src/templates.rs` | Add feature injection logic | 2 days |
| `crates/core/src/variable.rs` | Add `templateOption:` support | 1 day |
| `crates/core/src/oci.rs` | Add semantic tagging, verify annotations | 2 days |
| `crates/deacon/tests/test_templates_cli.rs` | Add 20+ new test cases | 2 days |
| Documentation & examples | Update all references | 1 day |

**Total Estimated Effort**: 12 days

---

## 🚦 Next Steps

### Immediate Actions (This Week)
1. Review this gap analysis with the team
2. Create GitHub issues for each critical gap
3. Prioritize which gaps to fix first
4. Decide on v1.0 timeline and breaking change policy

### Short Term (Next Sprint)
1. Start with CLI interface refactoring
2. Add template option substitution
3. Add feature injection
4. Write tests for new functionality

### Long Term (v1.0 Release)
1. Complete all critical gaps
2. Add comprehensive test coverage
3. Write migration guide
4. Update all documentation
5. Release v1.0 with breaking changes

---

## 📞 Questions & Decisions Needed

### Policy Questions
1. **Breaking changes**: Do we support gradual migration (deprecation warnings) or clean break?
2. **Extensions**: Keep `--dry-run`, `--force` as extensions? Keep `pull` command?
3. **Timeline**: What's the target date for v1.0?

### Technical Questions
1. **JSONC**: How to preserve formatting/comments when injecting features into devcontainer.json?
2. **Collection detection**: Use presence of `src/` directory or explicit marker file?
3. **Semantic versioning**: How to handle pre-release versions (1.2.3-alpha.1)?

### Testing Questions
1. **Registry**: Use mock registry or test against real ghcr.io?
2. **Coverage target**: What's acceptable test coverage percentage?
3. **Integration**: Run templates tests in CI with or without Docker?

---

## 📚 Reference Links

- **Specification**: `/workspaces/deacon/docs/subcommand-specs/templates/SPEC.md`
- **Data Structures**: `/workspaces/deacon/docs/subcommand-specs/templates/DATA-STRUCTURES.md`
- **Diagrams**: `/workspaces/deacon/docs/subcommand-specs/templates/DIAGRAMS.md`
- **Current Implementation**: `/workspaces/deacon/crates/deacon/src/commands/templates.rs`
- **Core Logic**: `/workspaces/deacon/crates/core/src/templates.rs`
- **Tests**: `/workspaces/deacon/crates/deacon/tests/test_templates_cli.rs`

---

## ✅ Conclusion

The templates subcommand requires **major refactoring** to match the specification. While the current implementation provides basic functionality, it diverges significantly in:

1. **CLI interface** (wrong flags, wrong formats)
2. **Feature completeness** (missing feature injection, omit paths)
3. **Registry operations** (no semantic versioning, no collections)
4. **Output formats** (missing structured JSON)

**Estimated effort**: 12-15 developer days for full compliance.

**Recommendation**: Make this a v1.0 milestone with proper breaking change communication and migration guide. The changes are substantial enough that trying to maintain backward compatibility would be more complex than a clean break.

**Priority**: HIGH - Templates are a core Dev Containers feature, and spec compliance is essential for ecosystem compatibility.

---

*For detailed analysis, see [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md)*  
*For implementation guidance, see [QUICK-REFERENCE.md](./QUICK-REFERENCE.md)*  
*For progress tracking, see [IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md)*
