# DevContainer CLI Parity Approach

**Date:** October 13, 2025  
**Purpose:** Strategy for achieving full parity with the official DevContainer CLI specification

---

## Overview

This document outlines the systematic approach for implementing missing functionality across all subcommands. Based on comprehensive gap analysis of 14 subcommands, we've identified cross-cutting concerns that must be addressed to ensure consistent, efficient implementation.

## Implementation Strategy

### Two-Phase Approach

1. **Phase 0: Foundation [COMPLETED]** - Build critical infrastructure that multiple subcommands depend on
2. **Phase 1-N: Subcommand Implementation** - Tackle individual subcommands while applying consistent patterns

## Phase 1-N: Subcommand Implementation

After Phase 0 is complete, tackle individual subcommands in dependency order. For each subcommand, ensure:

### Infrastructure to Build During Implementation

#### 5. Environment Probing System with Caching
- **Build When:** Implementing `run-user-commands` (first user-facing need)
- **Affects:** `exec`, `run-user-commands`, `set-up`
- **Components:**
  - `userEnvProbe` enum (none/loginInteractiveShell/interactiveShell/loginShell)
  - Shell command execution to probe environment
  - Result caching in `--container-session-data-folder`
  - Integration with variable substitution (`${containerEnv:VAR}`)

#### 6. Dotfiles Installation Workflow
- **Build When:** Implementing `run-user-commands`
- **Affects:** `run-user-commands`, `set-up`
- **Components:**
  - Git clone from `--dotfiles-repository`
  - Execute `--dotfiles-install-command` or `install.sh`
  - Target path handling (`--dotfiles-target-path`, default `~/dotfiles`)
  - Marker file idempotency (`.dotfilesMarker`)

#### 7. Secrets Management & Log Redaction
- **Build When:** Implementing `run-user-commands` or `set-up`
- **Affects:** Multiple subcommands
- **Components:**
  - Environment variable injection with `${secret:KEY}` substitution
  - Log output redaction for secret values
  - Trace/span field sanitization
- **Note:** Coordinate with existing Issue #41 placeholder

#### 8. Two-Phase Variable Substitution
- **Build When:** Extending `read-configuration` or `run-user-commands`
- **Affects:** `read-configuration`, `run-user-commands`, `set-up`, `up`, `build`
- **Components:**
  - Pre-container phase (local env, config values)
  - Post-container phase (`${containerEnv:VAR}` using probed environment)
  - Proper ordering and re-application

---

## Consistency Themes (Apply to ALL Subcommands)

### Theme 1: JSON Output Contract Compliance
For every subcommand with `--json` or `--output-format json`:
- ✅ JSON output goes to **stdout** (not stderr)
- ✅ Human-readable logs go to **stderr**
- ✅ Structure matches `DATA-STRUCTURES.md` exactly
- ✅ Error cases return `{}` or specified error shape with proper exit codes
- ✅ No trailing newlines or extra whitespace in JSON

### Theme 2: CLI Validation Rules
For every subcommand:
- ✅ Implement mutual exclusivity rules (clap `conflicts_with`)
- ✅ Implement paired requirement rules (clap `requires`)
- ✅ Regex format validation where specified (e.g., `--id-label` must match `/.+=.+/`)
- ✅ Use **exact error messages** from specification
- ✅ Validate before any expensive operations

### Theme 3: Collection Mode vs Single Mode
For `features-package`, `features-publish`, `templates`:
- ✅ Detection logic: if `src/` directory exists → collection mode
- ✅ Generate `devcontainer-collection.json` in both modes
- ✅ Consistent metadata structure across features and templates
- ✅ Proper iteration over collection items

### Theme 4: Semantic Versioning Operations
For `features-publish`, `templates`, `outdated`, `upgrade`:
- ✅ Use consistent semver parsing (regex `^\d+(\.\d+(\.\d+)?)?$`)
- ✅ Tag computation: `[major, major.minor, full, latest]`
- ✅ Sorting: descending semver order
- ✅ Filtering: exclude non-semver tags from registry results
- ✅ Use utilities from Phase 0, item #1

### Theme 5: Marker File Idempotency Pattern
For `run-user-commands`, `set-up`:
- ✅ Standard location: `/var/devcontainer/.{operation}Marker`
- ✅ Check existence before operation
- ✅ Write marker after successful completion
- ✅ `--prebuild` flag forces re-execution (ignores markers)
- ✅ Root-only operations for system patching

### Theme 6: Error Message Standardization
For all subcommands:
- ✅ Use exact error messages from specification where provided
- ✅ Follow format: `"Error description."` (sentence case, period)
- ✅ Include actionable information (what failed, why, how to fix)
- ✅ Use `bail!` or `anyhow::Context` for error chains
- ✅ No raw `unwrap()` or `expect()` in production code

---

## Subcommand Implementation Order (Recommended)

### Tier 1: Foundation (Unblock Others)
1. **read-configuration** - Core config resolution, needed by many others
2. **features-plan** - Feature resolution foundation (already ~75% done)

### Tier 2: Feature Lifecycle
3. **features-package** - Required before publish
4. **features-publish** - Uses Phase 0 OCI infrastructure
5. **features-info** - Uses Phase 0 OCI infrastructure
6. **features-test** - Independent, large scope (80-90% missing)

### Tier 3: Lockfile-Dependent
7. **outdated** - Uses Phase 0 lockfile infrastructure
8. **upgrade** - Uses Phase 0 lockfile infrastructure

### Tier 4: Container Lifecycle
9. **build** - Core container creation
10. **up** - Depends on build, run-user-commands
11. **run-user-commands** - Complex lifecycle execution
12. **set-up** - Similar to run-user-commands but for existing containers
13. **exec** - Simpler, depends on container selection

### Tier 5: Templates
14. **templates** - Full rewrite needed for CLI and OCI integration

---

## Quality Gates (Every Subcommand Must Pass)

Before marking any subcommand as "complete":

### Code Quality
- [ ] All CI checks pass (build, test, fmt, clippy)
- [ ] No `unsafe` code introduced
- [ ] No `unwrap()` or `expect()` in production paths
- [ ] Proper error context with `anyhow::Context`
- [ ] Tracing spans for major operations

### Specification Compliance
- [ ] All required CLI flags implemented
- [ ] All validation rules enforced
- [ ] JSON output matches DATA-STRUCTURES.md
- [ ] Error messages match specification
- [ ] Exit codes match specification

### Testing
- [ ] Unit tests for pure logic (>80% coverage)
- [ ] Integration tests for cross-module workflows
- [ ] Smoke tests updated in `crates/deacon/tests/smoke_basic.rs`
- [ ] Examples updated in `examples/` directory
- [ ] Fixtures added/updated in `fixtures/` directory

### Documentation
- [ ] Rustdoc for public functions
- [ ] Examples in `EXAMPLES.md` if user-facing
- [ ] Update gap analysis with "COMPLETED" status
- [ ] Update CLI_PARITY.md checklist

---

## Progress Tracking

### Phase 0 Status
- [ ] **Issue #1**: OCI Registry Infrastructure Enhancement
- [ ] **Issue #2**: Lockfile Data Structures & I/O
- [ ] **Issue #3**: Global CLI Flag Consolidation
- [ ] **Issue #4**: Container Selection & Inspection Utilities

### Subcommand Status
- [ ] build (40% complete, gap analysis exists)
- [ ] exec (50% complete, gap analysis exists)
- [ ] features-info (15% complete, gap analysis exists)
- [ ] features-package (30% complete, gap analysis exists)
- [ ] features-plan (75% complete, gap analysis exists)
- [ ] features-publish (40% complete, gap analysis exists)
- [ ] features-test (10% complete, gap analysis exists)
- [ ] outdated (0% complete, gap analysis exists, blocked by Phase 0)
- [ ] read-configuration (25% complete, gap analysis exists)
- [ ] run-user-commands (30% complete, gap analysis exists)
- [ ] set-up (0% complete, gap analysis exists)
- [ ] templates (40% complete, gap analysis exists)
- [ ] up (40% complete, gap analysis exists)
- [ ] upgrade (0% complete, gap analysis exists, blocked by Phase 0)

---

## References

- **Specification:** `/workspaces/deacon/docs/subcommand-specs/*/SPEC.md`
- **Gap Analyses:** `/workspaces/deacon/docs/subcommand-specs/*/IMPLEMENTATION_GAP_ANALYSIS.md`
- **Copilot Instructions:** `/workspaces/deacon/.github/copilot-instructions.md`
- **Contributing Guide:** `/workspaces/deacon/CONTRIBUTING.md`

---

**Last Updated:** October 13, 2025
