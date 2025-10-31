<!--
Sync Impact Report
- Version change: 1.0.0 → 1.0.1
- Modified principles: None (non-semantic cleanup)
- Added sections: None
- Removed sections: Residual template block (placeholder-based constitution skeleton)
- Templates requiring updates/alignment:
  - ✅ .specify/templates/plan-template.md (aligned; no changes required)
  - ✅ .specify/templates/spec-template.md (aligned; no changes required)
  - ✅ .specify/templates/tasks-template.md (aligned; no changes required)
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
All code changes MUST pass the full local CI checklist after every change, not just before commits:
- `cargo build --verbose` succeeds
- `cargo test --verbose -- --test-threads=1` passes (including smoke tests and doctests)
- `cargo fmt --all` followed by `cargo fmt --all -- --check` shows no changes required
- `cargo clippy --all-targets -- -D warnings` reports zero warnings
Public behavior changes MUST update tests and examples accordingly.

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
- Observability: prefer structured fields over string concatenation; ensure spans cover multi‑step workflows.

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

**Version**: 1.0.1 | **Ratified**: 2025-10-31 | **Last Amended**: 2025-10-31
