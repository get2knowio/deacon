---
description: Implement a task end-to-end
agent: build
model: openai/gpt-5-codex-medium
---
# Instructions
Implement the task end-to-end in a Rust CLI project. Your job is to implement the code and tests to fully satisfy the task, keep a local checklist of progress, and ensure all local checks pass.

# Details
Subcommand documentation: !`dirname "$(dirname "$ARGUMENTS")"`
Task detail: @$ARGUMENTS
Rust rules: @.opencode/context/rust-lang.md
Project constraints: @.github/copilot-instructions.md

# Argument validation and focus (do this first)
- Strictly use the task file provided via $ARGUMENTS. Do not infer or switch to a different subcommand or task on your own.
- Validate that $ARGUMENTS:
	- is non-empty
	- points to an existing file
	- matches the pattern docs/subcommand-specs/<subcommand>/tasks/<task>.md
- Derive SUBCOMMAND_DIR as: dirname(dirname($ARGUMENTS)) and verify it exists and contains SPEC.md and GAP.md
- If any validation fails, stop immediately with a clear, actionable error message that suggests the correct path and shows available tasks, e.g. by listing docs/subcommand-specs/*/tasks/*.md.
- Source for this behavior: https://opencode.ai/docs/commands/#arguments

# Required Tools and Environment
- Rust toolchain available (`cargo`, `rustfmt`, `clippy`).
- Ability to run repository tests locally.

# Operating Procedure
1. Create an implementation checklist
	- Maintain a local markdown checklist (e.g., in task notes) derived from the task’s success criteria and the CLI specification.
	- Include items for code, tests, docs/examples/fixtures updates, and verification via the Local green gates.
	- Update the list after each meaningful change.
2. Plan implementation against the CLI specification
	- Determine data shapes, flags, outputs, and side effects that must change, using the subcommand documentation as necessary.
	- Identify modules to modify and tests to add/adjust (unit and smoke/integration where applicable).
  - Read the SPEC.md (and DATA-STRUCTURES.md if present) from SUBCOMMAND_DIR = !`dirname "$(dirname "$ARGUMENTS")"` to align with the correct subcommand.
3. Implement iteratively with tests-first
	- For each step: write/adjust tests (happy path + at least one edge case), implement the smallest change to go green, and follow the Tight feedback workflow defined in the chatmode (build, tests, doctests, fmt, clippy).
	- Update your local checklist with progress.
4. Keep documentation and examples in sync
	- Update examples and fixtures under `examples/` and `fixtures/` when flags/output change.
	- Maintain `smoke_basic.rs` tests when CLI behavior is altered.

## Local green gates (mandatory)
Before checking off any checklist item related to implementation, ensure ALL of these pass locally. Treat failures as blockers and iterate until green. These are non-negotiable success criteria for this command:

- cargo build --verbose
- cargo test --verbose -- --test-threads=1
- cargo test --doc
- cargo fmt --all
- cargo fmt --all -- --check  # must report no changes required
- cargo clippy --all-targets -- -D warnings  # zero warnings allowed

If any command fails, fix the issues in small, focused changes and re-run the full set above before proceeding. Keep your local checklist updated.
 
## Final output (required)
When the task is complete, emit to the chat output ONLY the following (no other text or logs):

- Change summary: short description of what changed and why, listing key files touched.
- Verification steps: commands you ran (from the Local green gates) and their high-level results.

5. Completion criteria
	- Meets the Done criteria in the chatmode, including all Local green gates above (build, tests incl. doctests, fmt clean, clippy with -D warnings) and alignment with the spec; examples/fixtures/docs updated as needed.
	- Emits final chat output consisting ONLY of the change summary and verification steps (as described above).

# Deliverables
- Implemented Rust code and tests that satisfy the task's success criteria while abiding by our rust rules and project constraints.
- Updated documentation, examples, and fixtures as needed.
- All Local green gates pass successfully.
- Final chat output includes ONLY: (1) change summary and (2) verification steps; no other output.