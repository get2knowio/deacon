# Chatmodes

Purpose: define AI assistant personalities and behavior that shape reasoning and outputs.

- Files: `*.chatmode.md`
- Frontmatter: at least a `description` field
- Name: the basename (without extension) is the value used under `Role mode:` in prompt Composition
- Content: describe behavior, focus areas, and guardrails (e.g., what to prefer/avoid)
- Optional: you may include additional sections like “Behavior”, “Key principles”, and “Scope limits”

Example (skeleton):

````markdown
---
description: "Bring CI back to green without altering behavior"
---

# ci-green-keeper (chatmode)

Behavior: Bring CI back to green without altering behavior.

- Triage build → tests → doctests → fmt → clippy
- Minimal, local edits; no public API changes
- Summarize root cause and exact fix
````

## Files
- [ci-green-keeper.chatmode.md](ci-green-keeper.chatmode.md) — Bring CI back to green without altering behavior
- [docs-curator.chatmode.md](docs-curator.chatmode.md) — Keep docs/examples in sync with behavior
- [observability-advocate.chatmode.md](observability-advocate.chatmode.md) — Add structured tracing and sensitive data redaction
- [oci-integrator.chatmode.md](oci-integrator.chatmode.md) — Implement OCI flows safely and correctly
- [performance-engineer.chatmode.md](performance-engineer.chatmode.md) — Measure-first optimization with pragmatic, evidence-based improvements
- [refactor-guardian.chatmode.md](refactor-guardian.chatmode.md) — Improve structure without changing behavior or public APIs
- [spec-implementer.chatmode.md](spec-implementer.chatmode.md) — Implements capabilities strictly per spec with small, reviewable changes and tests
- [tdd-author.chatmode.md](tdd-author.chatmode.md) — Tests-first development with minimal implementations
- [troubleshooting-analyst.chatmode.md](troubleshooting-analyst.chatmode.md) — Reproduce, isolate, and fix defects with minimal patches
