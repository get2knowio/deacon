# Development Workflow

## Part 1: Feature Implementation

Invoke the slash-command `/speckit.implement` along with the following prompt:

```
Implement the tasks in {tasks_file}, each in their own subagent, parallelizing where possible. Each subagent should run as a speckit-rust-implementer.

Specification directory: {spec_dir}/
```

This single command handles:
- Reading and parsing the tasks file
- Processing tasks serially by default
- Parallelizing adjacent tasks marked with "P"
- Loading spec context for each task
- Marking tasks complete as they finish
- Running validation checks
- Reporting overall completion status

Wait for `/speckit.implement` to complete before proceeding to Part 2. **Do not work through the task list in any other way except via /speckit.implement**

---

## Part 2: Code Review and Improvement

### Phase 2.1: Parallel Reviews

Launch three subagents simultaneously:

**Subagent 1: CodeRabbit Review**
```
Run `coderabbit review --prompt-only` and return the complete output.
Do not summarize - return everything.
```

**Subagent 2: Technical Code Review**
```
Use rust-code-reviewer to do a thorough code review of the changes on this branch (both committed and uncommitted).

Return structured report with:
- File-by-file findings
- Severity (critical/major/minor/suggestion)
- Line numbers where applicable
- Concrete recommendations
```
**Subagent 3: Spec Compliance Review**
```
Use spec-compliance-reviewer to do a thorough spec compliance review of the changes in this branch (both committed and uncommitted).

Return structured report with:
- File-by-file findings
- Severity (critical/major/minor/suggestion)
- Line numbers where applicable
- Spec references where possible
- Concrete recommendations
```

### Phase 2.2: Consolidate Findings

Synthesize all three reviews:

1. **Deduplicate** overlapping findings

2. **Categorize** each unique issue:
   - `[CRITICAL]` - Bugs, security issues, spec violations
   - `[MAJOR]` - Architecture/design problems
   - `[MINOR]` - Code quality improvements
   - `[STYLE]` - Formatting, naming

3. **Create prioritized TODO list**

4. **Analyze parallelization:**
   - Issues in different files → can parallelize
   - Same file or dependencies → must serialize
   - Max 3-4 parallel subagents

5. **Identify deferrals**: For issues that are out of scope or require significant refactoring, note them for Phase 2.5

### Phase 2.3: Execute Improvements

For each batch of parallelizable issues, spawn subagents:
```
Task: Fix ISSUE-XXX

Issue: [description]
File(s): [files to modify]

Requirements:
- Minimal change for this specific issue
- Do NOT refactor unrelated code
- Run `cargo check` before completing
- Note (don't fix) any new issues discovered
```

After each batch: review changes, resolve conflicts, update TODO, proceed.

### Phase 2.4: Validation

Run `.claude/scripts/run-validation.sh` and parse results.

**If `all_passed` is true:** Proceed to Phase 2.5

**If any check failed:**
1. Parse error output from failed checks
2. Create TODO list of failures
3. Fix ALL failures (even if unrelated to our changes)
4. Priority: compilation → clippy → tests → formatting
5. Iterate (max 5 times, then report blockers)

For test failures, spawn subagents:
```
Fix failing test: [test name]
Error: [error output]
File: [location]

Investigate whether it's a test bug or implementation bug.
Fix the actual issue - do NOT weaken assertions.
```

### Phase 2.5: Create Tech Debt Issues

For each deferred issue identified during consolidation (Phase 2.2), create a GitHub issue directly:

```bash
python3 .claude/scripts/create_tech_debt_issue.py \
    --title "Brief descriptive title" \
    --problem "What's wrong and why it matters" \
    --rationale "Why this was deferred (pre-existing, out of scope, etc.)" \
    --pattern "Reference to correct pattern if applicable" \
    --files path/to/file1.rs path/to/file2.rs \
    --acceptance "What 'done' looks like" \
    --labels component1 component2 \
    --source-branch {branch}
```

The script:
- Creates a GitHub issue with `tech-debt` label plus specified component labels
- Returns the issue URL
- Supports `--dry-run` to preview and `--json` for structured output

**Collect all created issue URLs** for the PR description in Part 4.

**Deferral criteria** - Only defer when:
- The issue is pre-existing tech debt unrelated to the feature
- Fixing requires architectural changes beyond the feature scope
- The issue is a spec compliance gap that doesn't affect core functionality

---

## Part 3: Constitution Update

Before creating/updating the PR, feed learnings back into project conventions.

### Synthesize Learnings

Review all issues found during code review (Phase 2.1-2.3) and identify:

1. **Recurring patterns** - Issues that appeared multiple times
2. **Architectural anti-patterns** - Structural problems that could be prevented
3. **Specification gaps** - Areas where specs were ambiguous or incomplete
4. **Convention violations** - Inconsistencies that suggest missing guidelines

### Invoke Constitution Update

Run `/speckit.constitution` with a prompt structured like this:

```
Based on implementing {spec_dir}, the following issues were discovered during code review that could be prevented with better project conventions:

## Recurring Issues Found
[List patterns that appeared multiple times with examples]

## Architectural Concerns
[List structural issues that better guidelines could prevent]

## Suggested CLAUDE.md Updates
[Specific additions/changes to CLAUDE.md that would help Claude avoid these issues]
- Example: "Always use `thiserror` for error types, not manual `impl Error`"
- Example: "Prefer `&str` over `String` in function parameters unless ownership is needed"
- Example: "All public API functions must have doc comments with examples"

## Suggested Specification Conventions
[Patterns for writing clearer specs in the future]

## Suggested Code Conventions
[Project-specific coding standards to add to constitution]

Please update the project constitution and CLAUDE.md to incorporate these learnings.
```

**If no significant learnings:** Skip this step and note "No convention updates needed" in the final report.

---

## Part 4: PR Management

### Generate Final Report

Create the report (this becomes the PR description):

```markdown
## Summary

[One paragraph: what this PR accomplishes, main outcomes]

---

# Development & Review Summary

## Tasks Implemented
- Total tasks: X
- Completed: Y
- [List each task with brief summary]

## Code Review Findings
- CodeRabbit issues: X
- Architecture review issues: Y
- Total unique issues: Z

## Improvements Made
- Critical: X
- Major: Y
- Minor: Z
- Style: W

### Changes by File
- `path/to/file.rs`: [summary]

## Validation Status
- cargo fmt: ✅/❌
- cargo clippy: ✅/❌
- cargo build: ✅/❌
- cargo test: ✅/❌ (X passed, Y failed)

## Tech Debt Created
- [List any tech debt issues created with their GitHub URLs]
- Or "None"

## Convention Updates
- [Summary of changes made via /speckit.constitution, or "None needed"]

## Remaining Issues
- [Any unresolved items]

## Recommendations
- [Suggested follow-up work]
```

### Create/Update PR

1. **Generate PR title** using conventional commits:
   - `feat(scope):` - New features
   - `fix(scope):` - Bug fixes
   - `refactor(scope):` - Code restructuring
   - `docs(scope):` - Documentation
   - `test(scope):` - Tests
   - `chore(scope):` - Maintenance

   Scope = branch name or primary area of change

2. **Save report to temp file:**
   ```bash
   echo "[FINAL REPORT]" > /tmp/pr-body.md
   ```

3. **Run PR script:**
   ```bash
   .claude/scripts/manage-pr.sh "feat(scope): description" /tmp/pr-body.md
   ```

4. **Report the PR URL to the user**

---

## Execution Notes

- Commit after Part 1 (feature implementation)
- Commit after Part 2 (code review fixes)
- Commit after Part 3 (convention updates) if changes were made
- Subagent timeout: 5 minutes, then proceed
- Prefer many small subagents over few large ones
- When uncertain about parallelization, run sequentially
- Tech debt goes directly to GitHub issues (not tracked in tasks.md)
