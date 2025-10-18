---
name: PR Senior Review
description: Perform a rigorous senior-level review of a Rust CLI PR and post a consolidated comment with required actions.
version: 1
slashCommand: pr-senior-review
mode: maintainer-review
model: Claude Sonnet 4.5
tags:
   - rust
   - review
   - coderabbit
   - quality-gates
   - testing
---

# Purpose
Conduct a rigorous senior-level code review of a Rust CLI pull request and post a single consolidated comment with findings and actionable items.

# Role
Use the role, standards, and acceptance bar defined by the chat mode `maintainer-review`. This prompt adds task-specific steps and the output template.

# Constraints and References
- All role-level constraints come from the chat mode. For this prompt, ensure the comment references any spec deltas and explicitly calls out quality gate failures with remediation steps.

# Required Tools and Environment
- coderabbit CLI installed and authenticated (for diff and insights).
- GitHub CLI (`gh`) to fetch PR details and post comments.
- GitHub MCP Server
- Rust toolchain to validate build and tests.

# Review Procedure
1. Fetch latest PR state
   - Retrieve PR number, commits, changed files, and current description.
   - Run coderabbit analysis against the PR to surface hot spots via `coderabbit review --plain`.
2. Validate quality gates locally
   - `cargo fmt --all -- --check`
   - `cargo build --verbose`
   - `cargo test --verbose -- --test-threads=1`
   - `cargo test --doc`
   - `cargo clippy --all-targets -- -D warnings`
3. Review dimensions and criteria (apply mode standards)
   - Spec alignment and lifecycle order per repo spec files.
   - Design and error taxonomy; tracing and UX clarity.
   - Tests (happy/edge/failure), determinism, and smoke-tests when behavior changes.
   - Performance and allocations; avoid unnecessary clones.
   - Docs/examples/fixtures synced; doctests compile.
4. Compose a single consolidated PR comment using the template below.
5. Post the comment and apply or suggest appropriate labels.

# Acceptance Bar
- Same as chat mode; ensure your comment includes specific remediation guidance for any failing quality gates.

# Output Template (for your PR comment)

Title: Senior review summary and required actions

Summary:
- Overall assessment: [approve/changes requested/blocking]
- Key strengths: [list]
- Key risks: [list]

Detailed Findings:
- Specification alignment:
- Design and architecture:
- Error handling and UX:
- Tests and coverage:
- Performance and allocations:
- Docs/examples/fixtures:

Actionable Items (required for approval):
- [ ] Item 1 — What to change, why, and acceptance criteria
- [ ] Item 2 — What to change, why, and acceptance criteria
- [ ] Item 3 — What to change, why, and acceptance criteria

Optional Follow-ups:
- [ ] Improvement 1
- [ ] Improvement 2

Notes:
- Reference specific files/lines when helpful; keep recommendations precise and measurable.
