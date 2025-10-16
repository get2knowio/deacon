# Prompts

Purpose: executable agent workflows that define the task structure and compose chatmodes, contexts, and instructions.

- Files: `*.prompt.md`
- Frontmatter: typically includes `description`; may include runtime-specific fields like `mode` and `tools`
- Body: task sections using placeholders (e.g., `{{variables}}`) followed by a `## Composition` block
- Composition: references by basename only
  - `Role mode: <chatmode>` — must match a file in `.apm/chatmodes/*.chatmode.md`
  - `Contexts: a, b, c` — must match `.apm/context/*.context.md`
  - `Instructions: x, y, z` — must match `.apm/instructions/*.instructions.md`

Example (skeleton):

````markdown
---
description: Implement a spec-governed capability with tests
---

## Task summary
{{task_summary}}

## Composition
- Role mode: spec-implementer
- Contexts: codebase-map, examples-and-fixtures
- Instructions: prime-directives, testing-strategy, quality-gates
````

## Files
- [add-or-extend-tests.prompt.md](add-or-extend-tests.prompt.md) — Add or extend tests to define behavior
- [fix-ci-to-green.prompt.md](fix-ci-to-green.prompt.md) — Recover CI to green without changing behavior
- [implement-spec-task.prompt.md](implement-spec-task.prompt.md) — Implement a spec-governed capability with tests
- [observability-pass.prompt.md](observability-pass.prompt.md) — Improve observability with spans, fields, and redaction
- [performance-pass.prompt.md](performance-pass.prompt.md) — Optimize performance with evidence and guardrails
- [refactor-safely.prompt.md](refactor-safely.prompt.md) — Refactor structure without behavior changes
- [triage-and-fix-bug.prompt.md](triage-and-fix-bug.prompt.md) — Reproduce, isolate, and fix a regression
- [update-docs-and-examples.prompt.md](update-docs-and-examples.prompt.md) — Update docs, examples, and fixtures to match behavior
