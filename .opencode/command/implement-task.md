---
description: Implement a task end-to-end
agent: build
model: openai/gpt-5-codex-medium
---
# Instructions
Implement the task end-to-end in a Rust CLI project, using the GitHub CLI to manage the PR and maintaining a live checklist in the PR body while iterating until all GitHub Actions workflows pass. Your job is to implement the code and tests to fully satisfy the task, keep the PR body updated with a todo list and progress, and ensure all CI checks pass. Keep commits small and use Conventional Commits.

# Details
PR ID: !`gh pr view --json number --jq '.number' --head "$(git rev-parse --abbrev-ref HEAD)"`
Subcommand documentation: !`$(echo "$ARGUMENTS" | cut -d'/' -f1-3)`
Task detail: @$ARGUMENTS
Rust rules: @.opencode/context/rust-lang.md
Project constraints: @.github/copilot-instructions.md

# Required Tools and Environment
- GitHub CLI (`gh`) authenticated for the repository.
- GitHub MCP Server
- Rust toolchain available (`cargo`, `rustfmt`, `clippy`).
- Ability to run repository tests and CI locally.

# Operating Procedure
1. Create a PR checklist
	- Insert a markdown checklist at the top of the PR description derived from the PR’s success criteria and the CLI specification.
	- Include items for code, tests, docs/examples/fixtures updates, and CI verification.
	- Update the list after each meaningful change.
2. Plan implementation against the CLI specification
	- Determine data shapes, flags, outputs, and side effects that must change, using the subcommand documentation as necessary.
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
- Implemented Rust code and tests that satisfy the PR's success criteria while abiding by our rust rules and project constraints.
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