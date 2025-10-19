# Prompt: Generate Markdown Tasks for DevContainer CLI Parity Implementation

## Context

You are working on the `deacon` project, a Rust implementation of a DevContainer-like CLI. The project has comprehensive specification documents for each subcommand in `docs/subcommand-specs/`, and we need to create a structured series of GitHub issues to close implementation gaps.

## Your Task

For a given subcommand (e.g., `up`, `build`, `exec`, etc.), create multiple implementation tasks - each scoped to be completable by an AI coding agent in a single session

## Required Inputs

You will be provided with a subcommand directory containing the following files for the target subcommand:

- **GAP.md** - Detailed gap analysis showing what's missing vs. implemented
- **SPEC.md** - Complete specification of expected behavior
- **DATA-STRUCTURES.md** - Data structures and types required
- **DIAGRAMS.md** - Sequence diagrams and flow charts

You will also be provided with `PARITY_APPROACH.md`.  These are cross-cutting concerns and consistency themes (from `docs/`)

These are the inputs you *must* consult when determing which tasks to create and generating the content for those tasks.

## Output Requirements

For each logical unit of work, create a markdown implementation task in the <subcommand>/tasks directory:

**Title Format:** `[<subcommand>] Implement <Specific Feature/Category>`

**Scope Guidelines:**
- Should be completable in 1-3 hours by an AI agent
- Focus on ONE logical area (e.g., "Missing CLI Flags - Docker Paths", "Lifecycle Hooks Execution", "Environment Probing System")
- Include all necessary context for standalone completion

**Body Structure:**
```markdown
## Issue Type
- [ ] Missing CLI Flags
- [ ] Core Logic Implementation
- [ ] Infrastructure/Cross-Cutting Concern
- [ ] Error Handling
- [ ] Testing & Validation
- [ ] Other: ___________

## Description
[2-3 sentences describing what needs to be implemented and why it matters]

## Specification Reference

**From SPEC.md Section:** [e.g., "§3. Input Processing Pipeline" or "§7. External System Interactions"]

**From GAP.md Section:** [e.g., "2. Input Processing Pipeline - Missing"]

### Expected Behavior
[Extract relevant pseudocode or specification text from SPEC.md]

### Current Behavior
[Extract relevant status from GAP.md]

## Implementation Requirements

### 1. Code Changes Required

#### Files to Modify
- `crates/deacon/src/commands/<subcommand>.rs` - [specific changes]
- `crates/core/src/<module>.rs` - [specific changes]
- [Additional files as needed]

#### Specific Tasks
- [ ] Add CLI flags: `--flag-name`, `--another-flag`
- [ ] Implement validation rule: [description]
- [ ] Add data structure: [name from DATA-STRUCTURES.md]
- [ ] Implement function: [pseudocode reference]
- [ ] Add error handling for: [scenarios]

### 2. Data Structures

**Required from DATA-STRUCTURES.md:**
```rust
// Copy relevant struct/enum definitions from DATA-STRUCTURES.md
// translated to Rust syntax
```

### 3. Validation Rules
- [ ] Validate format: [regex or rule from SPEC.md]
- [ ] Mutual exclusion: [flags that conflict]
- [ ] Required pairing: [flags that require each other]
- [ ] Error message: "[Exact message from SPEC.md]"

### 4. Cross-Cutting Concerns

**Applies from PARITY_APPROACH.md:**
- [ ] **Theme 1 - JSON Output Contract:** [If applicable, specify JSON structure]
- [ ] **Theme 2 - CLI Validation:** [If applicable, list validation rules]
- [ ] **Theme 6 - Error Messages:** Use exact error messages from specification
- [ ] **Infrastructure Item 5-8:** [If this issue builds required infrastructure]

## Testing Requirements

### Unit Tests
- [ ] Test validation rules with valid/invalid inputs
- [ ] Test data structure parsing/serialization
- [ ] Test error conditions and messages
- [ ] [Specific test cases from SPEC.md §15]

### Integration Tests
- [ ] Test with real devcontainer.json configurations
- [ ] Test error scenarios (missing dependencies, invalid configs)
- [ ] Test interaction with [related systems]

### Smoke Tests
- [ ] Update `crates/deacon/tests/smoke_basic.rs` with new behavior
- [ ] Ensure tests pass with and without Docker available

### Examples
- [ ] Add/update example in `examples/<subcommand>/`
- [ ] Update `examples/README.md` if new example added
- [ ] Add fixture in `fixtures/` if needed for tests

## Acceptance Criteria

- [ ] All CLI flags implemented with correct types and descriptions
- [ ] All validation rules enforced (clap `conflicts_with`, `requires`, etc.)
- [ ] JSON output matches DATA-STRUCTURES.md schema exactly
- [ ] Error messages match specification exactly
- [ ] All CI checks pass:
  ```bash
  cargo build --verbose
  cargo test --verbose -- --test-threads=1
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  ```
- [ ] Documentation updated (rustdoc for public functions)
- [ ] No `unwrap()` or `expect()` in production code paths
- [ ] Proper error context with `anyhow::Context`
- [ ] Tracing spans added for observable operations

## Implementation Notes

### Key Considerations
- [Any specific notes from GAP.md about complexity or dependencies]
- [Performance considerations from SPEC.md §11]
- [Security considerations from SPEC.md §12]
- [Cross-platform notes from SPEC.md §13]

### Edge Cases to Handle
[Extract from SPEC.md §14 - Edge Cases relevant to this issue]

### Reference Implementation
[If TypeScript reference code is relevant, note the source file/function]

**Related to Infrastructure (PARITY_APPROACH.md):**
- [ ] Item 5: Environment Probing System with Caching
- [ ] Item 6: Dotfiles Installation Workflow
- [ ] Item 7: Secrets Management & Log Redaction
- [ ] Item 8: Two-Phase Variable Substitution

## Definition of Done

- [ ] Code implements all requirements from specification
- [ ] All tests pass (unit, integration, smoke)
- [ ] Examples demonstrate new functionality
- [ ] Documentation is complete and accurate
- [ ] CI pipeline passes all checks
- [ ] PR reviewed and approved
- [ ] GAP.md updated with completion status
- [ ] Tracking issue checklist updated

## References

- **Specification:** `docs/subcommand-specs/<subcommand>/SPEC.md` (§X)
- **Gap Analysis:** `docs/subcommand-specs/<subcommand>/GAP.md` (§X)
- **Data Structures:** `docs/subcommand-specs/<subcommand>/DATA-STRUCTURES.md`
- **Diagrams:** `docs/subcommand-specs/<subcommand>/DIAGRAMS.md`
- **Parity Approach:** `docs/PARITY_APPROACH.md`
- **Copilot Instructions:** `.github/copilot-instructions.md`
```

## Task Creation Strategy

### Grouping Guidelines

Group missing functionality into issues by these categories (prioritize atomicity over perfect grouping):

1. **CLI Flags & Validation** (typically 2-3 issues)
   - Group related flags (e.g., "Docker/Compose Paths", "Build Configuration", "Lifecycle Control")
   - One issue per logical flag group (max 8-10 flags per issue)

2. **Core Execution Logic** (typically 3-5 issues)
   - One issue per major workflow phase (e.g., "Container Discovery", "Image Extension with Features", "Lifecycle Hooks")
   - Split complex phases into sub-issues

3. **Infrastructure/Cross-Cutting** (as needed)
   - Environment Probing (if needed by this subcommand)
   - Secrets Management (if needed)
   - Dotfiles Installation (if needed)
   - Variable Substitution Enhancement (if needed)

4. **External System Interactions** (typically 1-2 issues)
   - Docker/Compose operations
   - OCI Registry operations
   - File system operations

5. **State Management** (typically 1 issue)
   - Caching, lockfiles, session data

6. **Testing & Examples** (typically 1 issue per category)
   - Can be bundled with feature implementation or separate
   - Always include in acceptance criteria of feature issues

### Prioritization

Order issues by:
1. **Dependencies first** - Infrastructure that other issues need
2. **Foundation second** - CLI flags and validation (enables testing)
3. **Core logic third** - Main execution workflows
4. **Polish last** - Edge cases, advanced features

### Size Guidelines

- **Small Issue:** 1-2 hours, 1-2 files modified, <200 lines changed
- **Medium Issue:** 2-4 hours, 3-5 files modified, 200-500 lines changed
- **Large Issue:** 4-8 hours, 5+ files modified, >500 lines changed

**Prefer medium-sized issues.** Split large issues into smaller ones.

## Special Instructions

### For Infrastructure Items (PARITY_APPROACH.md Items 5-8)

If the subcommand requires one of these infrastructure items:

1. Create a **separate issue** for the infrastructure component
2. Label it: `infrastructure`, `cross-cutting`
5. Include in the issue title: `[Infrastructure]` prefix

### For Consistency Themes (PARITY_APPROACH.md Themes 1-6)

1. **Always include** relevant themes in the "Cross-Cutting Concerns" section
2. **Enforce theme compliance** in acceptance criteria
3. **Reference theme** in test requirements (e.g., "Test JSON output per Theme 1")
4. Common theme applications:
   - **Theme 1 (JSON Output):** All subcommands with JSON output
   - **Theme 2 (CLI Validation):** All CLI flag issues
   - **Theme 3 (Collection Mode):** Only features-package, features-publish, templates
   - **Theme 4 (Semver):** Only features-publish, templates, outdated, upgrade
   - **Theme 5 (Marker Files):** Only run-user-commands, set-up
   - **Theme 6 (Error Messages):** ALL issues

## Output Format

Generate issues in Markdown format, with labels suggested for each issue in frontmatter at the top

## Example Labels for Issues

```
subcommand: name
type: enhancement
priority: high
scope: medium
```

---

## Usage Instructions

When you receive this prompt along with a specific subcommand's documentation:

1. **Read all provided files** (GAP.md, SPEC.md, DATA-STRUCTURES.md, DIAGRAMS.md, PARITY_APPROACH.md)
3. **Identify logical groupings** for implementation tasks (aim for 5-10 issues total)
4. **Generate each implementation task** with full detail
5. **Generate them in dependency order**, and number them in their filename accordingly
5. **Verify completeness** - all missing functionality from GAP.md should be covered
7. **Validate scope** - each issue should be AI-agent-completable
