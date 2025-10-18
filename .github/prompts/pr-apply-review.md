---
name: PR Apply Review
description: Address actionable review feedback on a Rust CLI PR and iterate to green CI.
version: 1
slashCommand: pr-apply-review
mode: address-pr-comments
model: Claude Sonnet 4.5
tags:
   - rust
   - ci
   - testing
   - review
   - conventional-commits
---

# Purpose
Apply actionable review feedback on a Rust CLI pull request, update code and tests, and iterate until all GitHub Actions workflows pass. The PR number will be provided in the prompt (e.g., "for PR #123").

# Notes and references
Role, guardrails, and coding standards are defined by the chatmode at `.github/chatmodes/address-pr-comments.chatmode.md` (which in turn references `.github/copilot-instructions.md` and the spec under `docs/subcommand-specs/*/SPEC.md`). This prompt focuses on the task flow and deliverables only.

# Required Tools and Environment
- GitHub CLI (`gh`) authenticated for the repository.
- GitHub MCP Server
- Rust toolchain available (`cargo`, `rustfmt`, `clippy`).
- Ability to run repository tests and CI locally.

# Procedure
1. Parse review feedback
   - Extract actionable items and convert them into a checklist in the PR description, grouped by priority or area.
   - Link each item to relevant files/lines when possible.
2. Address items incrementally
   - Add or adjust tests first (happy path + edge cases), then implement code.
   - After each change, run the full quality checklist defined in the chatmode guardrails (fmt, build, tests, doctests, clippy) and keep the build green.
   - Push commits and confirm CI results are green.
   - Update the PR checklist, marking items as completed.
3. Documentation and examples
   - Update examples and fixtures to reflect any behavior or flag changes.
   - Maintain smoke tests when CLI behavior changes.
4. Review thread responses
   - Reply to each addressed thread summarizing the change and linking to commit(s).
   - Resolve threads when appropriate.
5. Final verification
   - Ensure all review items are checked off.
   - All CI workflows pass; doctests pass; zero clippy warnings; code formatted (per chatmode checklist).
   - Add a brief “What changed since review” summary to the PR description.

# Deliverables
- Updated code and tests addressing all review feedback.
- Resolved review threads with explanations.
- Passing CI across all workflows and a PR description ready for approval.
