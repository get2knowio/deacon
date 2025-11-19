<!--
Sync Impact Report
- Version change: 1.3.0 → 1.4.0
- Modified principles:
  - I. Spec‑Parity as Source of Truth (added explicit data model and algorithm alignment requirements)
  - II. Keep the Build Green (added pre-implementation validation gate)
  - III. No Silent Fallbacks (added explicit filtering requirements for invalid inputs)
  - V. Observability and Output Contracts (added explicit ordering and schema compliance requirements)
- Added sections:
  - VI. Testing Completeness (new principle for spec-mandated test coverage)
  - Pre-Implementation Validation Checklist (in Development Workflow)
- Removed sections: None
- Templates requiring updates/alignment:
  - ✅ .specify/templates/plan-template.md (Constitution Check references updated principles)
  - ✅ .specify/templates/spec-template.md (inherits from constitution)
  - ✅ .specify/templates/tasks-template.md (references testing principle)
- Follow-up TODOs:
  - Review existing fixtures for reproducibility (alpine:latest → pinned versions)
  - Audit deprecated dependencies across codebase (atty → is-terminal pattern)
-->

# deacon Constitution
<!-- A Rust DevContainer-like CLI aligned with containers.dev and the repo's CLI spec -->

## Core Principles

### I. Spec‑Parity as Source of Truth
Deacon MUST implement behavior consistent with the authoritative CLI specification in `docs/CLI-SPEC.md` and the
containers.dev ecosystem. Terminology (feature, template, lifecycle, workspace, container environment, variable
substitution) MUST be preserved. Any requested change conflicting with the spec requires explicit clarification and
spec updates before implementation; examples and fixtures MUST be kept in sync.

**Data Model and Algorithm Alignment**: Implementation data structures MUST match spec-defined shapes exactly (e.g.,
`map<string, T>` cannot be substituted with `Vec<T>`); null handling, field presence, and ordering requirements are
binding contracts. When the spec defines an algorithm (e.g., version derivation, configuration resolution with extends,
tag selection logic), the implementation MUST follow it step-by-step; do not invent shortcuts or "close enough"
alternatives that break spec guarantees.

**Configuration Resolution**: Commands that operate on devcontainer configuration MUST use the full resolution path
(includes `extends` chains, overrides, variable substitution) as defined in the spec, not just the top-level file.
Failing to resolve the complete effective configuration violates contract expectations and produces incomplete results.

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

**Pre-Implementation Validation**: Before writing implementation code for a new subcommand or feature:
1. Read the complete spec section (SPEC.md, data-model.md, contracts/) to understand all requirements
2. Identify all data structures, algorithms, and behavioral contracts defined in the spec
3. Map out which existing infrastructure (e.g., `ConfigLoader::load_with_extends`, error types) must be used
4. Create a checklist of spec-mandated behaviors (flags, exit codes, output formats, ordering) to verify during implementation
5. Do not proceed with implementation until this validation is documented (e.g., in a plan.md or PR description)

This gate prevents "implement first, discover spec mismatch later" cycles that generate technical debt.

### III. No Silent Fallbacks — Fail Fast
Production code MUST NOT silently downgrade, noop, or substitute mock/stub implementations when capabilities (OCI,
registry resolution, container runtime, feature install backend) are unavailable or unimplemented. The program MUST
emit a clear, user‑facing error and abort. Mocks/fakes are permitted ONLY in tests and MUST NOT leak into runtime
code paths.

**Input Validation and Filtering**: When the spec defines which inputs are valid or supported (e.g., only OCI feature
refs, only semver tags), the implementation MUST filter or skip invalid entries as specified—do not pass them through
to downstream logic where they cause confusing errors or bogus output. Explicit parsing and validation at ingress
points prevents cascading failures and keeps error messages clear and actionable.

### IV. Idiomatic, Safe Rust
Code MUST be modern, idiomatic Rust (Edition 2021) with clear module boundaries, no `unsafe` (unless absolutely
required and fully justified with documented safety invariants). Error handling: prefer `thiserror` for domain
errors in core; use `anyhow` only at the binary boundary with meaningful context. Abstractions SHOULD be expressed
via traits (e.g., `ContainerRuntime`, `RegistryClient`) to enable alternate backends; production binds to real
implementations. Introduce async only for IO‑bound work. Logging uses `tracing` with structured fields and spans
aligned to workflows (e.g., `config.resolve`, `feature.install`, `container.create`, `lifecycle.run`). Formatting
and imports are enforced via rustfmt; imports order: std → external crates → local modules.

**Error Propagation**: Use `Result` types consistently; do not swallow errors with unwraps or by returning sentinel
values when operations can fail. Provide error context with `anyhow::Context` or equivalent. Avoid direct
`std::process::exit` calls; implement proper `Termination` or error wrappers so cleanup and testing work correctly.

**Dependency Hygiene**: Keep dependencies current and avoid deprecated crates. When a dependency is deprecated or
superseded (e.g., `atty` → `is-terminal`), migrate promptly. Prefer minimal, stable dependencies with active
maintenance.

### V. Observability and Output Contracts
Stdout/stderr separation is a contract:
- JSON modes (`--json`, `--output json`): stdout contains only the single JSON document; all logs go to stderr.
- Text modes: stdout contains human‑readable results; all logs/diagnostics go to stderr via `tracing`.

Log format and level are configurable; structured JSON logs are supported. Secret values MUST be redacted by
default in logs. Span names and fields MUST reflect spec workflows for traceability. Release hygiene follows
Conventional Commits; labels drive release notes; examples and fixtures MUST remain representative and pass parsing.

**Schema and Ordering Compliance**: JSON output MUST conform exactly to the spec-defined schema (key names, field
presence, null handling). When the spec requires declaration order to be preserved (e.g., features in config order),
use ordered data structures (`Vec`, `IndexMap`) during serialization—do not allow implicit alphabetical reordering
via `BTreeMap` or similar. Text output MUST honor the same ordering guarantees when specified.

**Exit Code Contracts**: When the spec defines special exit codes (e.g., exit 2 for "outdated detected"), honor them
in ALL output modes (text, JSON, interactive, non-interactive). Do not gate exit code behavior on output format
unless explicitly specified.

### VI. Testing Completeness
All spec-mandated tests MUST be implemented before a feature is considered complete. When a spec includes a "Testing"
section or lists required test scenarios, treat them as acceptance criteria:
- Unit tests for pure logic (version comparison, parsing, validation)
- Integration tests for workflows crossing process/runtime boundaries (with mocked external dependencies)
- Output format tests (text rendering, JSON schema validation, ordering)
- Exit code tests (success, failure, special codes like --fail-on-outdated)
- Edge case and resilience tests (empty inputs, invalid refs, network failures with mocks)
- Doctests for public APIs and helpers

Tests MUST be deterministic and hermetic (no network); use fixtures and mocked registries. When an integration test
passes but codifies incorrect behavior (e.g., testing for alphabetical order when spec requires declaration order),
it is a bug—fix the implementation and update the test to assert correct behavior.

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

### Pre-Implementation Validation Checklist

Before implementing a new subcommand or major feature, complete this checklist and document answers in your plan or PR:

1. **Spec Review**: Have you read the complete spec section (SPEC.md, data-model.md, contracts/)?
2. **Data Model Alignment**: Do your structs match spec-defined shapes exactly (map vs vec, field names, null handling)?
3. **Algorithm Alignment**: Have you identified all spec-defined algorithms (resolution, derivation, selection) and planned to implement them precisely?
4. **Input Validation**: Have you identified which inputs are valid/supported and where filtering must occur?
5. **Configuration Resolution**: Does your command use the full resolution path (extends, overrides, substitution) if it reads config?
6. **Output Contracts**: Have you verified JSON schema, ordering requirements, and exit code contracts?
7. **Testing Coverage**: Have you listed all spec-mandated tests and planned to implement them?
8. **Infrastructure Reuse**: Have you identified which existing helpers/loaders/traits you must use (vs reimplementing)?

This checklist prevents spec drift and reduces rework. Document deviations with explicit justification.

### Agentic Fast Loop Mode (local‑only)

- Use `make dev-fast` for rapid iterations; it avoids Docker‑heavy suites and long‑running integration tests.
- Recommended cadence: run `make test-non-smoke` every few iterations if you touched parsing/validation; run
  `make test-smoke` when touching Docker lifecycle; run `make release-check` before commits/PRs.
- This preserves the "keep build green" principle while reducing iteration time.

### Fixture and Example Hygiene

- **Reproducibility**: Pin all external images and dependencies to specific versions (e.g., `alpine:3.18` not
  `alpine:latest`). This prevents non-deterministic failures and ensures examples remain stable over time.
- **Schema Currency**: Keep fixture schemas current with spec requirements (e.g., Docker Compose 3.9+ for profiles).
- **Test Realism**: Integration test fixtures MUST include all flags and fields the test name implies (e.g., a test
  named `ignore_host_requirements` must actually set `ignore_host_requirements: true`).

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
  SHALL block merges on violations of Principles I–VI or on missing updates to tests/examples.

**Version**: 1.4.0 | **Ratified**: 2025-10-31 | **Last Amended**: 2025-11-19
