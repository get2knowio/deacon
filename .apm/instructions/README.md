# Instructions

Purpose: durable rules, coding standards, and process guidelines applied across tasks.

- Files: `*.instructions.md`
- Frontmatter: at least a `description` field; optionally `applyTo` with glob patterns to scope applicability
- Name: the basename (without extension) is referenced in prompts under `Instructions:`
- Content: normative guidance (e.g., error taxonomy, logging, testing, style, project structure)

Example (skeleton):

````markdown
---
description: "Build, test, doctest, fmt, and clippy gates to keep CI green"
---

# Quality Gates

Run after every change:
- cargo build --verbose
- cargo test --verbose -- --test-threads=1
- cargo test --doc
- cargo fmt --all && cargo fmt --all -- --check
- cargo clippy --all-targets -- -D warnings
````

## Files
- [asynchronicity-and-io.instructions.md](asynchronicity-and-io.instructions.md) — Guidance for introducing async and isolating IO for testability
- [commit-and-pr-policy.instructions.md](commit-and-pr-policy.instructions.md) — Commit messages, PR requirements, and CI expectations
- [dependency-policy.instructions.md](dependency-policy.instructions.md) — Dependency management rules for a lean, intentional workspace
- [docs-and-examples-maintenance.instructions.md](docs-and-examples-maintenance.instructions.md) — Maintain docs/examples/fixtures in sync with behavior changes
- [error-taxonomy.instructions.md](error-taxonomy.instructions.md) — Error handling taxonomy using thiserror; anyhow only at binary boundaries
- [imports-formatting-and-style.instructions.md](imports-formatting-and-style.instructions.md) — Imports order, formatting, and code style conventions
- [logging-and-observability.instructions.md](logging-and-observability.instructions.md) — Tracing spans, structured fields, and secret redaction
- [module-and-project-structure.instructions.md](module-and-project-structure.instructions.md) — Module layout and responsibilities across crates
- [oci-http-client-guidelines.instructions.md](oci-http-client-guidelines.instructions.md) — OCI registry HTTP client usage: HEAD checks, Location handling, status codes
- [prime-directives.instructions.md](prime-directives.instructions.md) — Non-negotiable development directives for spec-first, green builds
- [quality-gates.instructions.md](quality-gates.instructions.md) — Build, test, doctest, fmt, and clippy gates to keep CI green
- [testing-strategy.instructions.md](testing-strategy.instructions.md) — Unit/integration testing approach with deterministic, hermetic tests
- [unsafe-and-security.instructions.md](unsafe-and-security.instructions.md) — Security posture and policy on unsafe code usage
