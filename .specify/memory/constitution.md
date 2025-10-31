<!--
Sync Impact Report
- Version change: 1.2.0 → 1.3.0
- Modified principles:
  - II. Keep the Build Green (clarified fast vs full test cadence)
- Added sections:
  - Agentic Fast Loop Mode (local-only)
- Removed sections: None
- Templates requiring updates/alignment:
  - ✅ .specify/templates/plan-template.md (no changes required)
  - ✅ .specify/templates/spec-template.md (no changes required)
  - ✅ .specify/templates/tasks-template.md (no changes required)
- Follow-up TODOs: None
-->

# deacon Constitution
<!-- A Rust DevContainer-like CLI aligned with containers.dev and the repo's CLI spec -->

## Core Principles

### I. Spec‑Parity as Source of Truth
Deacon MUST implement behavior consistent with the authoritative CLI specification in `docs/CLI-SPEC.md` and the
containers.dev ecosystem. Terminology (feature, template, lifecycle, workspace, container environment, variable
substitution) MUST be preserved. Any requested change conflicting with the spec requires explicit clarification and
spec updates before implementation; examples and fixtures MUST be kept in sync.

### II. Keep the Build Green (Non‑Negotiable)
All code changes MUST keep the build green with an explicit cadence for quick vs. full checks:
- Fast Loop (default during spec‑phase, local only):
  - `cargo fmt --all && cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
  - Fast tests only: unit/bins/examples + doctests (e.g., `make dev-fast`)
- Full Gate (periodic and before push/PR):
  - `cargo build --verbose`
  - `cargo test -- --test-threads=1` (all tests, including integration and smoke as applicable)
  - `cargo test --doc`
  - `cargo fmt --all && cargo fmt --all -- --check`
  - `cargo clippy --all-targets -- -D warnings`
Public behavior changes MUST update tests and examples accordingly. Use non‑smoke (`make test-non-smoke`) or
smoke‑only (`make test-smoke`) runs during development when touching relevant areas, and always run a full gate before PR.

### III. No Silent Fallbacks — Fail Fast
Production code MUST NOT silently downgrade, noop, or substitute mock/stub implementations when capabilities (OCI,
registry resolution, container runtime, feature install backend) are unavailable or unimplemented. The program MUST
emit a clear, user‑facing error and abort. Mocks/fakes are permitted ONLY in tests and MUST NOT leak into runtime
code paths.

### IV. Idiomatic, Safe Rust
Code MUST be modern, idiomatic Rust (Edition 2021) with clear module boundaries, no `unsafe` (unless absolutely
required and fully justified with documented safety invariants). Error handling: prefer `thiserror` for domain
errors in core; use `anyhow` only at the binary boundary with meaningful context. Abstractions SHOULD be expressed
via traits (e.g., `ContainerRuntime`, `RegistryClient`) to enable alternate backends; production binds to real
implementations. Introduce async only for IO‑bound work. Logging uses `tracing` with structured fields and spans
aligned to workflows (e.g., `config.resolve`, `feature.install`, `container.create`, `lifecycle.run`). Formatting
and imports are enforced via rustfmt; imports order: std → external crates → local modules.

### V. Observability and Output Contracts
Stdout/stderr separation is a contract:
- JSON modes (`--json`, `--output json`): stdout contains only the single JSON document; all logs go to stderr.
- Text modes: stdout contains human‑readable results; all logs/diagnostics go to stderr via `tracing`.
Log format and level are configurable; structured JSON logs are supported. Secret values MUST be redacted by
default in logs. Span names and fields MUST reflect spec workflows for traceability. Release hygiene follows
Conventional Commits; labels drive release notes; examples and fixtures MUST remain representative and pass parsing.

## Additional Constraints & Security

- Do not execute arbitrary shell from unvalidated input; surface destructive operations (e.g., container removal,
  volume pruning) behind explicit flags.
- Avoid leaking secrets in logs; maintain a redaction layer (if disabled explicitly, warn users).
- Tests MUST be deterministic and hermetic (no network); gate true integration behind CI‑only markers when needed.
- Prefer minimal, pinned dependencies; justify additions and keep the dependency set lean.

## Development Workflow & Quality Gates

- Small, reviewable changes; avoid large refactors unless explicitly requested. Keep the binary crate focused on the
  CLI entrypoint and orchestration; extract shared logic into core crates as needed.
- For each new module: add unit tests (pure logic) and integration tests when crossing process/runtime boundaries.
- Maintain smoke tests under `crates/deacon/tests/` covering: read‑configuration, build, up/exec, doctor.
- Doctests MUST compile and run; add missing trait imports, `Default` impls, or public visibility as needed.
- Examples under `examples/` and fixtures under `fixtures/` are living documentation; update them when user‑facing
  flags, schemas, or outputs change; keep `examples/README.md` curated and aligned with spec terminology.
- Use ast-grep tool (command 'sg') for searching or rewriting code instead of find or grep.
- Use context7 MCP server for retrieving up-to-date documentation for libraries and packages.
- Use github MCP server for interacting with GitHub repositories, managing issues, pull requests, and code searches.
- Observability: prefer structured fields over string concatenation; ensure spans cover multi‑step workflows.

### Agentic Fast Loop Mode (local‑only)

- Use `make dev-fast` for rapid iterations; it avoids Docker‑heavy suites and long‑running integration tests.
- Recommended cadence: run `make test-non-smoke` every few iterations if you touched parsing/validation; run
  `make test-smoke` when touching Docker lifecycle; run `make release-check` before commits/PRs.
- This preserves the “keep build green” principle while reducing iteration time.

## Governance

- This constitution supersedes other practice docs where conflicts arise for CLI behavior, quality gates, and
  engineering discipline.
- Amendments require a PR with: change rationale, mapping to `docs/CLI-SPEC.md` sections, risk assessment, and a
  version bump per rules below.
- Versioning of this document uses Semantic Versioning:
  - MAJOR: backward‑incompatible governance or principle removals/redefinitions
  - MINOR: new principles/sections or materially expanded guidance
  - PATCH: clarifications, wording, typo fixes, non‑semantic refinements
- Compliance Review: All PRs MUST include a quick constitution compliance check (in PR body or checklist). Reviewers
  SHALL block merges on violations of Principles II–V or on missing updates to tests/examples.
<!-- Cleanup: removed residual template; no behavioral changes -->

**Version**: 1.3.0 | **Ratified**: 2025-10-31 | **Last Amended**: 2025-10-31
