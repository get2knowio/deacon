# Templates Subcommand Documentation

This directory contains the complete specification and implementation analysis for the `templates` subcommand.

## 📚 Documentation Index

### Specification (Source of Truth)
1. **[SPEC.md](./SPEC.md)** - Complete specification
   - Command-line interface
   - Input processing pipeline
   - Core execution logic
   - OCI registry interactions
   - Error handling strategy
   - Performance considerations
   - Security considerations
   
2. **[DATA-STRUCTURES.md](./DATA-STRUCTURES.md)** - Data structure definitions
   - CLI argument structures
   - Template metadata format
   - OCI structures
   - Command outputs
   
3. **[DIAGRAMS.md](./DIAGRAMS.md)** - Visual documentation
   - Sequence diagrams (Mermaid)
   - Data flow diagrams (ASCII)

### Implementation Analysis
4. **[GAP-ANALYSIS-SUMMARY.md](./GAP-ANALYSIS-SUMMARY.md)** ⭐ **START HERE**
   - Executive summary of gaps
   - Top 7 critical issues
   - Recommended implementation path
   - Success metrics
   
5. **[IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md)** - Detailed analysis
   - CLI interface gaps (by subcommand)
   - Data structure gaps
   - OCI/registry interaction gaps
   - Functional gaps
   - Testing gaps
   - ~600 lines of detailed comparison
   
6. **[IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md)** - Progress tracking
   - Compliance tables by subcommand
   - Test coverage status
   - Implementation roadmap
   - Developer notes

### Developer Resources
7. **[QUICK-REFERENCE.md](./QUICK-REFERENCE.md)** - Cheat sheet
   - Side-by-side command comparisons
   - Code snippets for required changes
   - Test cases to add
   - Implementation checklist

## 🚀 Quick Start

### For Reviewers
1. Read [GAP-ANALYSIS-SUMMARY.md](./GAP-ANALYSIS-SUMMARY.md) for the overview
2. Check [IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md) for current state
3. Review [SPEC.md](./SPEC.md) to understand what should be implemented

### For Implementers
1. Read [QUICK-REFERENCE.md](./QUICK-REFERENCE.md) for at-a-glance comparisons
2. Use [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md) for detailed requirements
3. Track progress with checklists in [IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md)
4. Reference [SPEC.md](./SPEC.md) as the source of truth

### For Testers
1. Check [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md) §5 for test gaps
2. Use test cases from [QUICK-REFERENCE.md](./QUICK-REFERENCE.md) §7
3. Reference [SPEC.md](./SPEC.md) §15 for testing strategy

## 📊 Current Status

**Overall Compliance**: ~40%  
**Status**: 🔴 Critical gaps require major refactoring  
**Target**: v1.0 release with breaking changes  
**Estimated Effort**: 12-15 developer days

### Critical Issues (Must Fix)
1. ❌ CLI interface completely mismatched
2. ❌ Missing `--features` injection
3. ❌ No `${templateOption:KEY}` substitution
4. ❌ `metadata` uses wrong input type
5. ❌ No semantic versioning in publish
6. ❌ No collection support in publish
7. ❌ Missing structured JSON output

### What Works
- ✅ Local template application
- ✅ Basic variable substitution
- ✅ Dry-run mode
- ✅ Basic OCI registry interaction
- ✅ Documentation generation

## 🗺️ Implementation Roadmap

### Week 1: Foundation
- Refactor CLI interface
- Add template option substitution
- Add feature injection
- Add structured JSON output

### Week 2: Registry Operations
- Rewrite metadata command
- Add semantic versioning
- Add collection support
- Fix publish output format

### Week 3: Polish & Release
- Add omit paths support
- Complete missing flags
- Write comprehensive tests
- Update documentation

## 🎯 Success Criteria

- [ ] All CLI flags match specification
- [ ] Template option substitution works
- [ ] Feature injection works
- [ ] Semantic versioning works
- [ ] Collection publishing works
- [ ] All output formats match spec
- [ ] 20+ integration tests pass
- [ ] Migration guide published

## 📁 Related Files

### Implementation
- Source: `/workspaces/deacon/crates/deacon/src/commands/templates.rs`
- Core: `/workspaces/deacon/crates/core/src/templates.rs`
- CLI: `/workspaces/deacon/crates/deacon/src/cli.rs`
- Tests: `/workspaces/deacon/crates/deacon/tests/test_templates_cli.rs`

### Fixtures
- `/workspaces/deacon/fixtures/templates/minimal`
- `/workspaces/deacon/fixtures/templates/with-options`

### Examples
- `/workspaces/deacon/examples/template-management/`

## 🔄 Document Maintenance

### When to Update

**Update GAP-ANALYSIS-SUMMARY.md when**:
- Critical issues are resolved
- Overall compliance score changes
- Roadmap timeline changes

**Update IMPLEMENTATION-STATUS.md when**:
- Any subcommand implementation changes
- Test coverage increases
- Checklist items are completed

**Update QUICK-REFERENCE.md when**:
- Code patterns change
- New helper functions added
- Test templates change

**Update SPEC.md when**:
- Spec itself changes (upstream)
- Implementation discovers spec ambiguities
- Design decisions are made

### Version History

- **2025-10-13**: Initial gap analysis created
  - Analyzed specification vs. implementation
  - Identified 7 critical gaps
  - Created 4 supporting documents
  - Estimated 12-15 days effort for compliance

## 🤝 Contributing

When working on templates implementation:

1. **Always** reference the spec as source of truth
2. **Update** the relevant analysis docs when making changes
3. **Check** the quick reference for code patterns to follow
4. **Add** tests for any new functionality
5. **Mark** checklist items as complete in IMPLEMENTATION-STATUS.md

## 📞 Questions?

- **What should be implemented?** → See [SPEC.md](./SPEC.md)
- **What's the current gap?** → See [GAP-ANALYSIS-SUMMARY.md](./GAP-ANALYSIS-SUMMARY.md)
- **How do I implement it?** → See [QUICK-REFERENCE.md](./QUICK-REFERENCE.md)
- **What's the current status?** → See [IMPLEMENTATION-STATUS.md](./IMPLEMENTATION-STATUS.md)
- **Need detailed analysis?** → See [IMPLEMENTATION-GAP-ANALYSIS.md](./IMPLEMENTATION-GAP-ANALYSIS.md)

## 📜 License

Same as parent project. See `/workspaces/deacon/LICENSE`.

---

**Last Updated**: October 13, 2025  
**Specification Version**: v1.0 (from subcommand-specs/templates/)  
**Implementation Version**: Current main branch  
**Compliance**: ~40%
