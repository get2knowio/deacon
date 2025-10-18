---
description: 'Implement scoped Rust changes with tests-first and a green pipeline'
model: 'Claude Sonnet 4.5'
---
You are an agent—implement the requested change end-to-end with tight, verifiable loops. Keep diffs small and the build green throughout.

## Prime directives (repo-specific)
- Source of truth: `docs/subcommand-specs/*/SPEC.md`.
- Follow `.github/copilot-instructions.md` and `AGENTS.md` strictly.
- No silent fallbacks; fail fast with clear user-facing errors when a capability is unimplemented.
- Error handling: `thiserror` in core; `anyhow` at binary boundary only.
- Logging: `tracing` spans with structured fields aligned to workflows.
- No `unsafe` code. Prefer borrowing over cloning; avoid premature optimization.

## Tight feedback workflow (mandatory after each edit)
1. Write or update a minimal failing test capturing the intended behavior.
2. Implement the smallest code change to make the test pass.
3. Run the full validation suite:
   - cargo build --verbose
   - cargo test --verbose -- --test-threads=1
   - cargo test --doc
   - cargo fmt --all && cargo fmt --all -- --check
   - cargo clippy --all-targets -- -D warnings
4. If user-visible behavior or flags changed:
   - Update smoke tests under `crates/deacon/tests/` and relevant fixtures under `fixtures/`.
   - Update `examples/` and `examples/README.md`.
   - Update docs/comments where appropriate.

## Design and implementation guidance
- Keep binary crate (`crates/deacon`) focused on CLI and orchestration; move reusable logic toward core when it grows.
- Introduce traits for IO-bound backends (Docker/runtime, registry client) to enable test doubles in tests only.
- Keep pure logic synchronous; introduce async only for IO.
- For OCI/registry work, enforce:
  - HEAD-before-GET for blob existence checks.
  - Use Location header from POST /blobs/uploads responses.
  - Update all `HttpClient` implementations and test mocks if trait signatures change.
- Maintain import ordering and formatting; add trailing commas in multi-line literals.

## Rust research loop (required when touching external crates/APIs)
- Your knowledge may be stale; when integrating or modifying third‑party crates, research first:
   - Use the `fetch_webpage` tool to search docs.rs, crate READMEs, and upstream specs.
   - Follow relevant links recursively until usage is unambiguous.
   - Announce each tool call with a one‑line preface (what and why).

## Anti‑patterns to avoid
- Using `.clone()` where borrowing suffices; prefer `&str` over `String` when possible.
- Panicking in production (`unwrap`/`expect`); return errors with context.
- Collecting iterators early; prefer lazy adaptors and `Iterator` combinators.
- Introducing `unsafe` without necessity; do not add `unsafe` code.
- Over‑abstracting with traits; keep implementations simple and testable first.

## Done criteria
- All new/updated tests pass deterministically.
- No clippy warnings; formatting check passes with no changes required.
- Behavior aligns with the spec; deviations are called out explicitly.
- Examples and fixtures reflect the new behavior where applicable.

## Output style
- Provide a brief delta-focused summary of changes.
- Report quality gates with PASS/FAIL for: Build, Tests, Doctests, Format, Clippy.
- List any follow-ups as explicit next steps if larger refactors are deferred.
