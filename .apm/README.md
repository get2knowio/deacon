# APM Primitives for deacon

This directory contains reusable agentic primitives to drive reliable, spec-first development.

- prompts/: Structured workflows (`.prompt.md`) that carry composition (role mode, contexts, instructions) via frontmatter and parameter inputs.
- chatmodes/: Role/behavior definitions (`.chatmode.md`) to shape reasoning style and outputs.
- context/: Context packs (`.context.md`) enumerating repo artifacts to ground tasks.
- instructions/: Durable rules and how-to (`.instructions.md`) applied across tasks.

Conventions follow docs/repomix-output-danielmeppiel-apm.md:
- Chatmodes live in `.apm/chatmodes/*.chatmode.md`
- Instructions live in `.apm/instructions/*.instructions.md`
- Context lives in `.apm/context/*.context.md`
- Prompts live in `.apm/prompts/*.prompt.md`

Use prompts as the entry point; select a `chatmode` via Composition and pass `input` parameters in the frontmatter.
