# Context

Purpose: repository knowledge packs used to ground tasks with concrete, local information.

- Files: `*.context.md`
- Name: the basename (without extension) is referenced in prompts under `Contexts:`
- Content: lists of relevant files/paths, architecture notes, invariants, test locations, or domain details
- Scope: keep contextual, not prescriptive; avoid repeating rules that belong in instructions

Example (skeleton):

````markdown
# context: codebase-map

Orientation for code locations:
- Cargo.toml (workspace)
- crates/deacon/** (CLI entrypoint and commands)
- crates/core/** (shared logic, parsing, runtime abstractions)
- tests and fixtures under crates/*/tests and fixtures/**
````

## Files
- [codebase-map.context.md](codebase-map.context.md) — Orientation for code locations and responsibilities
- [doctest-hygiene.context.md](doctest-hygiene.context.md) — Common doctest pitfalls and fixes
- [domain-config.context.md](domain-config.context.md) — Configuration semantics and references (core config, fixtures)
- [domain-features.context.md](domain-features.context.md) — Feature system semantics and assets
- [domain-lifecycle.context.md](domain-lifecycle.context.md) — Lifecycle command semantics and references
- [domain-oci.context.md](domain-oci.context.md) — OCI client and publish guidelines, traits, tests
- [domain-templates.context.md](domain-templates.context.md) — Template system semantics and assets
- [examples-and-fixtures.context.md](examples-and-fixtures.context.md) — Guidance on updating/validating examples and fixtures
- [logging-and-errors.context.md](logging-and-errors.context.md) — Overview of logging (tracing) and error handling patterns
- [quality-gates.context.md](quality-gates.context.md) — Quality gates and command order
- [smoke-tests.context.md](smoke-tests.context.md) — CLI smoke tests overview; deterministic, no network
- [spec-core.context.md](spec-core.context.md) — Authoritative specs and instruction sources to consult
