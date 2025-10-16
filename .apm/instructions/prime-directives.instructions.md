---
description: "Non-negotiable development directives for spec-first, green builds"
---

# Prime Directives

- Follow docs/CLI-SPEC.md and subcommand specs as the source of truth
- Prefer incremental, small, reviewable changes
- Maintain idiomatic Rust 2021; no unsafe without justification
- Keep build green; run full quality gates after each change
- No silent fallbacks; fail fast with clear user-facing errors
