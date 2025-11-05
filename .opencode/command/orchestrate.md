---
description: Orchestrate Spec Kit tasks by iterating a tasks.md file and invoking /speckit.implement per task via a subagent
agent: orchestrator
model: github-copilot/claude-haiku-4.5
subtask: false
---

# Orchestrate Spec Kit Tasks

Input (required):

```text
$ARGUMENTS
```

Usage examples:
- /orchestrate @specs/005-features-test-gap/tasks.md
- /orchestrate @/workspaces/deacon-project/worktrees-deacon/005-features-test-cmd/specs/005-features-test-gap/tasks.md
- /orchestrate #file:specs/005-features-test-gap/tasks.md

Behavior:
- Read the provided tasks file (either literal content via @file include, or resolve a #file:<path> argument).
- Sort tasks globally by numeric ID ascending (T001, T002, …). Start from task 001. Tasks without numeric IDs are processed after numbered ones in stable order.
- For each task:
	- If the task is completed ("- [X]"): verify it via the task-verifier; on failure, auto re‑implement (delegated to @general with failure context) and re‑verify.
	- If the task is not completed ("- [ ]"): implement it via @general, then verify via task-verifier; on verification failure, auto re‑implement (with failure context) and re‑verify.

```
@general /speckit.implement "Implement ONLY the following task (do not process others).\n\nTask payload:\n<PASTE THE EXACT TASK BLOCK HERE>\n\nConstraints:\n- Keep changes minimal and reviewable.\n- Respect repo instructions in `.github/copilot-instructions.md` and `AGENTS.md`.\n- If the task spans multiple files, finish one file then validate (build/test/fmt/clippy) before moving on.\n- Mark this task as [X] in tasks.md when done.\n- If blocked by missing details, stop and ask a concise clarification."
```

			- Completed task verification (no re‑implementation here; auto-fix path is described separately):

```
	@task-verifier /speckit.implement "VERIFY COMPLETION ONLY for the following task — do not re‑implement.\n\nTask payload:\n<PASTE THE EXACT TASK BLOCK HERE>\n\nVerification steps:\n- Validate acceptance criteria from the task/spec; confirm artifacts and code changes exist.\n- Run project checks relevant to this repo (eg: cargo build/test/fmt/clippy; or project equivalents).\n- Confirm behavior matches the plan/spec and tests (if any) pass.\n- If verification PASSES: report PASS and make no changes.\n- If verification FAILS: report FAIL with evidence and (optionally) re‑open the task by changing [X] -> [ ] with a brief note. Do not re‑implement here."
```

Output:
- Provide a short progress update after each task (ID, short title, PASS/FAIL, next steps if any).
- Treat verification failures as task failures. Stop current phase on a sequential task failure and report the failing output succinctly.
- Print a final summary with task counts per phase and overall PASS/FAIL.

On verification failure (Option A, non‑interactive):
- The orchestrator automatically re‑opens and re‑implements the task (delegated to `@general /speckit.implement`, passing the verifier’s failure context), then re‑runs verification.
- If the re‑verification still fails, it records the failure and proceeds according to phase rules (halt sequential phases; continue reporting for [P] groups).

Important:
- Do not edit files directly within this command. Always delegate actual changes to subagents via /speckit.implement.
- Prefer absolute paths when resolving #file:<path>.
