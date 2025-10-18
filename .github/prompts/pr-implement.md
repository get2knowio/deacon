---
name: PR Implementation
description: Implement the active PR end-to-end in a Rust CLI repo using gh, tests-first, and green CI.
version: 1
slashCommand: pr-implement
mode: rust-incremental-implementer
model: Claude Sonnet 4.5
tags:
   - rust
   - cli
   - gh
   - ci
   - testing
   - conventional-commits
---

# Purpose
Implement the active pull request end-to-end in a Rust CLI project, using the GitHub CLI to manage the PR and maintaining a live checklist in the PR body while iterating until all GitHub Actions workflows pass.

# Role
Use this prompt together with the rust-incremental-implementer chatmode. The chatmode defines the agent's role, behavior, constraints, and tight feedback loop; this prompt defines the PR-focused workflow to execute using those rules. Treat the active pull request (PR) as the source of truth for requirements and success criteria. Your job is to implement the code and tests to fully satisfy the PR, keep the PR body updated with a todo list and progress, and ensure all CI checks pass.

# Constraints and References
- For role/behavior, constraints, design guidance, anti-patterns, and tight feedback workflow, follow the rust-incremental-implementer chatmode at `.github/chatmodes/rust-incremental-implementer.chatmode.md`.
- For repository-specific rules and terminology, see `.github/copilot-instructions.md` and `docs/subcommand-specs/*/SPEC.md`.
- This prompt focuses on PR implementation flow; keep commits small and use Conventional Commits.

# Required Tools and Environment
- GitHub CLI (`gh`) authenticated for the repository.
- GitHub MCP Server
- Rust toolchain available (`cargo`, `rustfmt`, `clippy`).
- Ability to run repository tests and CI locally.

# Operating Procedure
1. Identify the target PR
   - The PR number will be provided in the prompt (e.g., "for PR #123").
   - Use `gh pr view <PR_NUMBER> --json number,title,body,headRefName,baseRefName,author,labels,assignees,state` to fetch PR details.
   - Read the PR description, success criteria, and any linked issues.
2. Create a PR checklist
   - Insert a markdown checklist at the top of the PR description derived from the PR’s success criteria and the CLI specification.
   - Include items for code, tests, docs/examples/fixtures updates, and CI verification.
   - Update the list after each meaningful change.
3. Prepare the branch
   - Ensure you are on the PR’s head branch.
   - Keep commits small with Conventional Commit titles.
4. Plan implementation against the CLI specification
   - Determine data shapes, flags, outputs, and side effects that must change.
   - Identify modules to modify and tests to add/adjust (unit and smoke/integration where applicable).
5. Implement iteratively with tests-first
   - For each step: write/adjust tests (happy path + at least one edge case), implement the smallest change to go green, and follow the Tight feedback workflow defined in the chatmode (build, tests, doctests, fmt, clippy).
   - Update the PR checklist and description with progress.
   - Push commits and confirm CI status in GitHub.
6. Keep documentation and examples in sync
   - Update examples and fixtures under `examples/` and `fixtures/` when flags/output change.
   - Maintain `smoke_basic.rs` tests when CLI behavior is altered.
7. Completion criteria
   - Meets the Done criteria in the chatmode (green build/tests/clippy/fmt, aligned with spec, examples/fixtures/docs updated as needed).
   - All GitHub Actions workflows are green.
   - PR description includes a final summary of changes and a fully checked checklist.

# Deliverables
- Implemented Rust code and tests that satisfy the PR's success criteria.
- Updated PR body with a maintained progress checklist and final summary.
- Updated PR title in Conventional Commits format using one of these lowercase prefixes:
  - `feat:` - New features or capabilities
  - `fix:` - Bug fixes
  - `perf:` - Performance improvements
  - `docs:` - Documentation changes
  - `refactor:` - Code refactoring without behavior changes
  - `ci:` - CI/CD pipeline changes
  - `build:` - Build system or dependency changes
  - `chore:` - Maintenance tasks
- Passing CI across all workflows.

# Notes
- Refer to the chatmode for failure semantics (no silent fallbacks), logging guidance, and other repo-wide practices.
