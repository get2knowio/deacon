---
description: "Dependency management rules for a lean, intentional workspace"
---

# Dependency Policy

- Keep dependencies lean; prefer std and existing deps
- Add via `cargo add --workspace` when shared
- Prefer `cargo update` for lock refresh; upgrade intentionally
